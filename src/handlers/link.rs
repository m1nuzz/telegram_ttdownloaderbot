use teloxide::prelude::*;
use rusqlite::{Connection, Result, params};
use std::env;
use std::fs;
use std::sync::Arc;
use uuid::Uuid;
use tokio::time::Duration;

use crate::database::{update_user_activity, log_download};
use crate::mtproto_uploader::MTProtoUploader;
use crate::yt_dlp_interface::YoutubeFetcher;
use crate::handlers::admin::is_admin;
use crate::handlers::subscription::check_subscription;
use crate::utils::progress_bar::ProgressBar;
use crate::telegram_bot_api_uploader::{send_video_with_progress_botapi, send_audio_with_progress_botapi};

const TELEGRAM_BOT_API_FILE_LIMIT: u64 = 48 * 1024 * 1024; // 48MB

pub async fn link_handler(bot: Bot, msg: Message, fetcher: Arc<YoutubeFetcher>, mtproto_uploader: Arc<MTProtoUploader>) -> Result<(), anyhow::Error> {
    if let Err(e) = update_user_activity(msg.chat.id.0) { log::error!("Failed to update user activity: {}", e); }

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

        let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
        let subscription_required: bool = {
            let conn = Connection::open(&db_path).unwrap();
            conn.query_row("SELECT value FROM settings WHERE key = 'subscription_required'", [], |row| {
                let value: String = row.get(0)?;
                Ok(value == "true")
            }).unwrap_or(true)
        };

        if subscription_required {
            let is_user_admin = is_admin(&msg).await;
            if !is_user_admin && !check_subscription(&bot, msg.chat.id.0).await.unwrap_or(false) {
                bot.send_message(msg.chat.id, "To use the bot, please subscribe to our channels.").await?;
                return Ok(())
            }
        }

        let quality_preference: String = {
            let conn = Connection::open(&db_path).unwrap();
            conn.query_row(
                "SELECT quality_preference FROM users WHERE telegram_id = ?1",
                params![msg.chat.id.0],
                |row| row.get(0),
            ).unwrap_or("best".to_string())
        };

        let is_audio = quality_preference == "audio";

        let filename_stem = format!("output/{}", Uuid::new_v4());
        match fetcher.download_video_from_url(text.to_string(), &filename_stem, &quality_preference, &mut progress_bar).await {
            Ok(path) => {
                log::info!("Video downloaded to: {:?}", path);
                let file_size = fs::metadata(&path)?.len();
                if file_size > TELEGRAM_BOT_API_FILE_LIMIT {
                    // MTProto upload
                    progress_bar.update(85, Some("📤 Starting upload...")).await?;
                    // Start chat action
                    let chat_action_task = tokio::spawn({
                        let bot = bot.clone();
                        let chat_id = msg.chat.id;
                        async move {
                            loop {
                                let action = if is_audio {
                                    teloxide::types::ChatAction::UploadDocument
                                } else {
                                    teloxide::types::ChatAction::UploadVideo
                                };
                                if bot.send_chat_action(chat_id, action).await.is_err() {
                                    break;
                                }
                                tokio::time::sleep(Duration::from_secs(4)).await;
                            }
                        }
                    });
                    // Realistic progress simulation in parallel with download
                    let progress_simulation = {
                        let mut pb = progress_bar.clone();
                        let size = file_size;
                        tokio::spawn(async move {
                            let estimated_seconds = (size as f64 / 1_500_000.0) as u64; // ~1.5MB/s
                            let steps = std::cmp::min(12, estimated_seconds); // 12 шагов максимум
                            for i in 1..=steps {
                                let percentage = 85 + ((i as f64 / steps as f64) * 13.0) as u8; // 85-98%
                                let info = format!("📤 Uploading... {:.0}%", ((i as f64 / steps as f64) * 100.0));
                                let _ = pb.update(percentage, Some(&info)).await;
                                let delay = std::cmp::max(400, estimated_seconds * 1000 / steps);
                                tokio::time::sleep(Duration::from_millis(delay)).await;
                            }
                        })
                    };
                    // Actual download without progress
                    let upload_result = if is_audio {
                        mtproto_uploader.upload_audio(msg.chat.id.0, username.clone(), &path, text, &mut ProgressBar::new_silent()).await
                    } else {
                        mtproto_uploader.upload_video(msg.chat.id.0, username.clone(), &path, text, &mut ProgressBar::new_silent()).await
                    };
                    // Stop simulation and chat action
                    progress_simulation.abort();
                    chat_action_task.abort();
                    match upload_result {
                        Ok(_) => {
                            // IMMEDIATELY delete the progress bar - video already sent via MTProto
                            progress_bar.delete().await?;
                        }
                        Err(_e) => {
                            progress_bar.delete().await?;
                            bot.send_message(msg.chat.id, "❌ Upload failed").await?;
                        }
                    }
                } else {
                    // Regular upload via Bot API
                    progress_bar.update(90, Some("📤 Sending...")).await?;
                    let send_res = if is_audio {
                        send_audio_with_progress_botapi(&bot.token(), msg.chat.id, &path, Some(text), &mut progress_bar).await
                    } else {
                        send_video_with_progress_botapi(&bot.token(), msg.chat.id, &path, Some(text), &mut progress_bar).await
                    };
                    match send_res {
                        Ok(_) => {
                            // IMMEDIATELY delete the progress bar - video sent
                            // progress_bar.delete().await?;
                            // send_video_with_progress_botapi already deletes the progress bar
                        },
                        Err(_e) => {
                            progress_bar.delete().await?;
                            bot.send_message(msg.chat.id, "❌ Send failed").await?;
                        }
                    }
                }
                // Logging and cleanup
                if let Err(_e) = log_download(msg.chat.id.0, text) {
                    log::error!("Failed to log download: {}", _e);
                }
                let _ = fs::remove_file(&path);
            }
            Err(_e) => {
                progress_bar.delete().await?;
                bot.send_message(msg.chat.id, "❌ Download failed").await?;
            }
        }
    } else {
        bot.send_message(msg.chat.id, "Please send a valid TikTok link.").await?;
    }

    Ok(())
}