use teloxide::prelude::*;

use std::sync::Arc;
use std::env;
use std::fs;

use anyhow::Error;
use crate::commands::Command;
use crate::handlers::{admin_command_handler, callback_handler, command_handler, link_handler, settings_text_handler, format_text_handler, subscription_text_handler, back_text_handler, set_quality_h265_text_handler, set_quality_h264_text_handler, set_quality_audio_text_handler, enable_subscription_text_handler, disable_subscription_text_handler};
use crate::yt_dlp_interface::{YoutubeFetcher, is_executable_present, ensure_binaries};
use crate::mtproto_uploader::MTProtoUploader;
use teloxide::dptree;

#[cfg(not(target_os = "android"))]

#[cfg(target_os = "android")]
use robius_directories::ProjectDirs;

mod commands;
mod database;
mod handlers;
pub mod mtproto_uploader;
mod yt_dlp_interface;
mod utils;
mod telegram_bot_api_uploader;
pub mod peers;

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();
    log::info!("Starting TikTok downloader bot...");

    dotenv::dotenv().ok();

    // Dynamic directory for libraries (yt-dlp and ffmpeg)
    let libraries_dir = std::env::current_dir()?.join("lib");

    // Dynamic directory for output
    let output_dir = std::env::current_dir()? // Consider making this configurable or user-specific
        .join("downloads");

    // Ensure required binaries are present before starting the async runtime
    if let Err(e) = ensure_binaries(&libraries_dir, &output_dir).await {
        log::error!("Failed to ensure binaries: {}", e);
        return Err(e.into());
    }

    log::info!("Libraries directory: {:?}", libraries_dir.canonicalize()?);
    log::info!("Contents of libraries directory: {:?}", fs::read_dir(&libraries_dir)?.map(|e| e.unwrap().file_name()).collect::<Vec<_>>());

    let yt_dlp_path = libraries_dir.join(if cfg!(target_os = "windows") { "yt-dlp.exe" } else { "yt-dlp" });
    let ffmpeg_dir = libraries_dir.join("ffmpeg");
    let ffmpeg_path = ffmpeg_dir.join(if cfg!(target_os = "windows") { "ffmpeg.exe" } else { "ffmpeg" });
    let ffprobe_path = ffmpeg_dir.join(if cfg!(target_os = "windows") { "ffprobe.exe" } else { "ffprobe" });

    if !is_executable_present(&yt_dlp_path) {
        log::error!("yt-dlp not found at {:?} after attempted download", yt_dlp_path);
        return Err(anyhow::Error::msg("yt-dlp not available"));
    } else {
        log::info!("yt-dlp found at {:?}", yt_dlp_path);
    }

    if !is_executable_present(&ffmpeg_path) {
        log::error!("ffmpeg not found at {:?} after attempted download", ffmpeg_path);
        return Err(anyhow::Error::msg("ffmpeg not available"));
    }

    if !is_executable_present(&ffprobe_path) {
        log::error!("ffprobe not found at {:?} after attempted download", ffprobe_path);
        return Err(anyhow::Error::msg("ffprobe not available"));
    }

    let db_path_env = env::var("DATABASE_PATH").unwrap_or_else(|_| {
        log::error!("DATABASE_PATH environment variable not set.");
        panic!("DATABASE_PATH must be set");
    });
    log::info!("DATABASE_PATH: {}", db_path_env);

    // Create directories if they don't exist
    fs::create_dir_all(&output_dir)?;
    log::info!("Contents of output directory: {:?}", fs::read_dir(&output_dir)?.map(|e| e.unwrap().file_name()).collect::<Vec<_>>());

    if let Err(e) = database::init_database() {
        log::error!("Failed to initialize the database at {}: {}", db_path_env, e);
        return Err(e.into());
    }
    log::info!("Database initialized successfully.");

    let fetcher = Arc::new(YoutubeFetcher::new(yt_dlp_path, output_dir.clone(), ffmpeg_dir.clone())?);
    let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN must be set");
    let mtproto_uploader = match MTProtoUploader::new(&bot_token, ffprobe_path.clone()).await {
        Ok(uploader) => Arc::new(uploader),
        Err(e) => {
            log::error!("Failed to create MTProtoUploader: {}", e);
            return Err(anyhow::anyhow!("{}", e));
        }
    };

    let bot = Bot::from_env();

    let handler = dptree::entry()
        .branch(Update::filter_message()
            .filter_async(|msg: Message| async move {
                msg.text().map_or(false, |text| text.starts_with("/addchannel") || text.starts_with("/delchannel") || text.starts_with("/listchannels"))
            })
            .endpoint(admin_command_handler)
        )
        .branch(Update::filter_message().filter_command::<Command>().endpoint(command_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("⚙️ Settings")).endpoint(settings_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("Format")).endpoint(format_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("Subscription")).endpoint(subscription_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("h265")).endpoint(set_quality_h265_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("h264")).endpoint(set_quality_h264_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("audio")).endpoint(set_quality_audio_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("Enable Subscription")).endpoint(enable_subscription_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("Disable Subscription")).endpoint(disable_subscription_text_handler))
        .branch(Update::filter_message().filter(|msg: Message| msg.text() == Some("Back")).endpoint(back_text_handler))
        .branch(Update::filter_message().endpoint(|msg: Message, bot: Bot, fetcher: Arc<YoutubeFetcher>, mtproto_uploader: Arc<MTProtoUploader>| async move {
            link_handler(bot, msg, fetcher, mtproto_uploader).await
        }))
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![fetcher, mtproto_uploader])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
