use teloxide::prelude::*;

use std::fs;
use std::sync::Arc;
use uuid::Uuid;
use tokio::time::{Duration, timeout};
use std::path::PathBuf;

use crate::database::DatabasePool;
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
    let user_id = msg.chat.id.0;
    
    // Update user activity using the database pool
    let result = db_pool.execute_with_timeout(move |conn| {
        conn.execute("INSERT OR IGNORE INTO users (telegram_id) VALUES (?1)", [user_id])?;
        conn.execute("UPDATE users SET last_active = CURRENT_TIMESTAMP WHERE telegram_id = ?1", [user_id])?;
        Ok(())
    }).await;
    
    if let Err(e) = result {
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
        let quality_preference = db_pool.get_user_quality(msg.chat.id.0).await.unwrap_or_else(|_| "best" .to_string());

        let is_audio = quality_preference == "audio";
        log::info!("Quality preference: {}, is_audio: {}", quality_preference, is_audio);

        // Download with timeout and retry - without progress bar during this since it causes borrowing issues
        let download_result = retry::retry_with_backoff(3, || async {
            timeout(DOWNLOAD_TIMEOUT, async {
                let file_stem = format!("output/{}", Uuid::new_v4());
                fetcher.download_video_from_url(text.to_string(), &file_stem, &quality_preference, &mut ProgressBar::new_silent()).await
            }).await
        }).await;
        
        let path = match download_result {
            Ok(path) => path?,
            Err(e) => { // This handles both timeout and retries failure
                progress_bar.delete().await?;
                
                // Analyze error type for more specific message
                let error_message = if e.to_string().contains("Sign in required") {
                    "🔒 Video requires sign in to TikTok - currently unavailable for download".to_string()
                } else if e.to_string().contains("Video unavailable") || e.to_string().contains("Requested format is not available") {
                    "🚫 Video is unavailable or has been removed".to_string()
                } else if e.to_string().contains("Private video") {
                    "🔒 Video is private and cannot be downloaded".to_string()
                } else if e.to_string().contains("This video is age-restricted") {
                    "🔞 Video is age-restricted and cannot be downloaded".to_string()
                } else if e.to_string().contains("Failed to parse") || e.to_string().contains("JSON") {
                    "🔧 Error processing TikTok API response. Please try again later.".to_string()
                } else if e.to_string().contains("timeout") {
                    "⏰ Download timeout - please try again".to_string()
                } else {
                    format!("❌ Failed to download video: {}", e.to_string().chars().take(100).collect::<String>())
                };
                
                bot.send_message(msg.chat.id, error_message).await?;
                return Ok(());
            }
        };

        // Create RAII wrapper for file cleanup
        let _temp_file = TempFile::new(path.clone());

        log::info!("Downloaded file path: {:?}, is_audio: {}, file_size: {}", 
                   path, is_audio, fs::metadata(&path)?.len());
        
        let file_size = fs::metadata(&path)?.len();
        
        if file_size > TELEGRAM_BOT_API_FILE_LIMIT {
            // MTProto upload with timeout and retry
            progress_bar.update(85, Some("📤 Starting upload...")).await?;
            
            let upload_result = retry::retry_with_backoff(3, || async {
                timeout(UPLOAD_TIMEOUT, async {
                    if is_audio {
                        log::info!("Uploading as audio file: {:?}", path);
                        mtproto_uploader.upload_audio(msg.chat.id.0, username.clone(), &path, text, &mut ProgressBar::new_silent()).await
                    } else {
                        log::info!("Uploading as video file: {:?}", path);
                        mtproto_uploader.upload_video(msg.chat.id.0, username.clone(), &path, text, &mut ProgressBar::new_silent()).await
                    }
                }).await
            }).await;
            
            match upload_result {
                Ok(_) => {
                    progress_bar.update(100, Some("✅ Done!")).await?;
                    tokio::time::sleep(Duration::from_millis(500)).await; // Brief pause to show completion
                    progress_bar.delete().await?;
                    log::info!("File uploaded successfully for chat {} (audio: {})", msg.chat.id.0, is_audio);
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
                        log::info!("Sending as audio file via Bot API: {:?}", path);
                        send_audio_with_progress_botapi(&bot.token(), msg.chat.id, &path, Some(text), &mut ProgressBar::new_silent()).await
                    } else {
                        log::info!("Sending as video file via Bot API: {:?}", path);
                        send_video_with_progress_botapi(&bot.token(), msg.chat.id, &path, Some(text), &mut ProgressBar::new_silent()).await
                    }
                }).await
            }).await;
            
            match send_result {
                Ok(_) => {
                    log::info!("File sent successfully via Bot API (audio: {})", is_audio);
                    // Progress bar already handled by send functions
                },
                Err(_e) => {
                    progress_bar.delete().await?;
                    bot.send_message(msg.chat.id, "❌ Send failed after retries").await?;
                }
            }
        }

        // Logging and cleanup
        let user_id = msg.chat.id.0;
        let video_url = text.to_string();
        let result = db_pool.execute_with_timeout(move |conn| {
            // Update user activity first (to ensure the user exists in the database)
            conn.execute("INSERT OR IGNORE INTO users (telegram_id) VALUES (?1)", [user_id])?;
            conn.execute("UPDATE users SET last_active = CURRENT_TIMESTAMP WHERE telegram_id = ?1", [user_id])?;
            conn.execute("INSERT INTO downloads (user_telegram_id, video_url) VALUES (?1, ?2)", (user_id, video_url))?;
            Ok(())
        }).await;
        
        if let Err(_e) = result {
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