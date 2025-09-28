use teloxide::prelude::*;

use std::fs;
use std::sync::Arc;
use uuid::Uuid;
use tokio::time::{Duration, timeout};
use std::path::PathBuf;

use crate::database::{update_user_activity, log_download, DatabasePool};
use crate::mtproto_uploader::MTProtoUploader;
use crate::yt_dlp_interface::YoutubeFetcher;
use crate::handlers::admin::is_admin;
use crate::handlers::subscription::check_subscription;
use crate::utils::progress_bar::ProgressBar;
use crate::utils::{retry, task_manager::TaskManager};
use crate::telegram_bot_api_uploader::{send_video_with_progress_botapi, send_audio_with_progress_botapi};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);   // 10 minutes
const TELEGRAM_BOT_API_FILE_LIMIT: u64 = 48 * 1024 * 1024; // 48MB

async fn get_subscription_required(db_pool: &DatabasePool) -> Result<bool, anyhow::Error> {
    let result = db_pool.execute_with_timeout(|conn| {
        match conn.query_row(
            "SELECT value FROM settings WHERE key = 'subscription_required'",
            [],
            |row| Ok(row.get::<_, String>(0)? == "true")
        ) {
            Ok(value) => Ok(value),
            Err(_) => Ok(true) // Default to true
        }
    }).await?;
    Ok(result)
}

pub async fn link_handler(
    bot: Bot,
    msg: Message,
    fetcher: Arc<YoutubeFetcher>,
    mtproto_uploader: Arc<MTProtoUploader>,
    db_pool: Arc<DatabasePool>,
    _task_manager: Arc<tokio::sync::Mutex<TaskManager>>,
    upload_semaphore: Arc<tokio::sync::Semaphore>
) -> Result<(), anyhow::Error> {
    if let Err(e) = update_user_activity(msg.chat.id.0) {
        log::error!("Failed to update user activity: {}", e);
    }

    let text = match msg.text() {
        Some(text) => text,
        None => return Ok(()),
    };

    if text.contains("tiktok.com") {
        let username: Option<String> = match msg.chat.username() {
            Some(un) => Some(un.to_string()),
            None => msg.from.clone().and_then(|u| u.username.clone()),
        };
        
        let mut progress_bar = ProgressBar::new(bot.clone(), msg.chat.id);
        progress_bar.start("🎬 Starting...").await?;
        
        // Get upload permit to limit concurrent uploads - must stay in scope for the entire function
        let _upload_permit = upload_semaphore.acquire().await.map_err(|e| anyhow::anyhow!("Semaphore error: {}", e))?;

        let subscription_required = get_subscription_required(&db_pool).await.unwrap_or(true);

        if subscription_required {
            let is_user_admin = is_admin(&msg).await;
            if !is_user_admin && !check_subscription(&bot, msg.chat.id.0).await.unwrap_or(false) {
                bot.send_message(msg.chat.id, "To use the bot, please subscribe to our channels.").await?;
                return Ok(())
            }
        }

        // Get user quality preference with caching
        let quality_preference = db_pool.get_user_quality(msg.chat.id.0).await.unwrap_or_else(|_| "best".to_string());

        let is_audio = quality_preference == "audio";

        // Download with timeout and retry - without progress bar during this since it causes borrowing issues
        let download_result = retry::retry_with_backoff(3, || async {
            timeout(DOWNLOAD_TIMEOUT, async {
                let file_stem = format!("output/{}", Uuid::new_v4());
                fetcher.download_video_from_url(text.to_string(), &file_stem, &quality_preference, &mut ProgressBar::new_silent()).await
            }).await
        }).await;
        
        let path = match download_result {
            Ok(path) => path?,
            Err(_) => { // This handles both timeout and retries failure
                progress_bar.delete().await?;
                bot.send_message(msg.chat.id, "⏰ Download timeout or failed - please check the link").await?;
                return Ok(());
            }
        };

        // Create RAII wrapper for file cleanup
        let _temp_file = TempFile::new(path.clone());

        let file_size = fs::metadata(&path)?.len();
        
        if file_size > TELEGRAM_BOT_API_FILE_LIMIT {
            // MTProto upload with timeout and retry
            progress_bar.update(85, Some("📤 Starting upload...")).await?;
            
            let upload_result = retry::retry_with_backoff(3, || async {
                timeout(UPLOAD_TIMEOUT, async {
                    if is_audio {
                        mtproto_uploader.upload_audio(msg.chat.id.0, username.clone(), &path, text, &mut ProgressBar::new_silent()).await
                    } else {
                        mtproto_uploader.upload_video(msg.chat.id.0, username.clone(), &path, text, &mut ProgressBar::new_silent()).await
                    }
                }).await
            }).await;
            
            match upload_result {
                Ok(_) => {
                    progress_bar.update(100, Some("✅ Done!")).await?;
                    tokio::time::sleep(Duration::from_millis(500)).await; // Brief pause to show completion
                    progress_bar.delete().await?;
                    log::info!("Video uploaded successfully for chat {}", msg.chat.id.0);
                }
                Err(e) => {
                    progress_bar.delete().await?;
                    let error_msg = if let Some(wait_seconds) = crate::utils::retry::extract_flood_wait(&e.to_string()) {
                        format!("⏳ Rate limited. Please wait {} seconds and try again.", wait_seconds)
                    } else {
                        "❌ Upload failed - please try again later".to_string()
                    };
                    bot.send_message(msg.chat.id, error_msg).await?;
                }
            }
        } else {
            // Regular upload via Bot API with timeout and retry - no progress bar to avoid borrowing issues
            let send_result = retry::retry_with_backoff(3, || async {
                timeout(UPLOAD_TIMEOUT, async {
                    if is_audio {
                        send_audio_with_progress_botapi(&bot.token(), msg.chat.id, &path, Some(text), &mut ProgressBar::new_silent()).await
                    } else {
                        send_video_with_progress_botapi(&bot.token(), msg.chat.id, &path, Some(text), &mut ProgressBar::new_silent()).await
                    }
                }).await
            }).await;
            
            match send_result {
                Ok(_) => {
                    // Progress bar already handled by send functions
                },
                Err(_e) => {
                    progress_bar.delete().await?;
                    bot.send_message(msg.chat.id, "❌ Send failed after retries").await?;
                }
            }
        }

        // Logging and cleanup
        if let Err(_e) = log_download(msg.chat.id.0, text) {
            log::error!("Failed to log download: {}", _e);
        }
    } else {
        bot.send_message(msg.chat.id, "Please send a valid TikTok link.").await?;
    }

    Ok(())
}

// RAII for automatic file cleanup
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let path = self.path.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                log::warn!("Failed to cleanup temp file {}: {}", path.display(), e);
            }
        });
    }
}