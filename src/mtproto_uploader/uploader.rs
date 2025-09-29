
use grammers_client::{Client, Config};
use grammers_session::Session;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use grammers_client::client::InitParams;

use crate::mtproto_uploader::constants::SESSION_FILE;

// Добавляем импорт для tl функций
use grammers_tl_types as tl;

#[derive(Clone)]
pub struct MTProtoUploader {
    pub client: Client,
    pub ffprobe_path: PathBuf,
    pub ffmpeg_path: PathBuf,
}

impl MTProtoUploader {
    pub async fn new(bot_token: &str, ffprobe_path: PathBuf, ffmpeg_path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let api_id: i32 = env::var("TELEGRAM_API_ID")?.parse()?;
        let api_hash = env::var("TELEGRAM_API_HASH")?;

        let session = Session::load_file_or_create(SESSION_FILE)?;
        
        // Настройка параметров инициализации
        let params = InitParams {
            device_model: "Desktop".to_string(),
            system_version: "Windows 10".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            system_lang_code: "en".to_string(),
            lang_code: "en".to_string(),
            catch_up: false,
            server_addr: None,
            flood_sleep_threshold: 60,
            update_queue_limit: Some(100),
            ..Default::default()
        };
        
        let client = Client::connect(Config {
            session,
            api_id,
            api_hash: api_hash.clone(),
            params,
        }).await?;

        if !client.is_authorized().await? {
            client.bot_sign_in(bot_token).await?;
        }
        client.session().save_to_file(SESSION_FILE)?;

        // Keep-alive механизм каждые 30 минут (меньше чем 1 час жизни соли)
        let client_keepalive = client.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1800)); // 30 минут
            loop {
                interval.tick().await;
                match client_keepalive.invoke(&tl::functions::updates::GetState {}).await {
                    Ok(_) => log::debug!("Keep-alive ping successful"),
                    Err(e) => log::warn!("Keep-alive ping failed: {:?}, connection may be stale", e),
                }
            }
        });

        Ok(Self { client, ffprobe_path, ffmpeg_path })
    }
}