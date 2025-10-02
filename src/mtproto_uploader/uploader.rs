
use grammers_client::{Client, Config};
use grammers_session::Session;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use grammers_client::client::InitParams;
use tokio::sync::Mutex;

use crate::mtproto_uploader::constants::SESSION_FILE;

// Добавляем импорт для tl функций
use grammers_tl_types as tl;

#[derive(Clone)]
pub struct MTProtoUploader {
    pub client: Arc<Mutex<Client>>,
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

        // Обернем клиента в Arc<Mutex<>> для возможности переподключения
        let client = Arc::new(Mutex::new(client));

        // Запускаем keep-alive пинг в отдельной задаче
        let client_keepalive = client.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 минут
            loop {
                interval.tick().await;

                // Попробуем выполнить ping
                let result = {
                    let client_guard = client_keepalive.lock().await;
                    client_guard.invoke(&tl::functions::updates::GetState {}).await
                };

                match result {
                    Ok(_) => log::debug!("Keep-alive ping successful"),
                    Err(e) => {
                        log::error!("Keep-alive ping failed: {:?}, reconnecting...", e);
                        
                        // Попытка переподключения
                        if let Err(reconnect_err) = MTProtoUploader::reconnect_client(&client_keepalive).await {
                            log::error!("Reconnection failed: {:?}", reconnect_err);
                        } else {
                            log::info!("Client reconnected successfully");
                        }
                    }
                }
            }
        });

        Ok(Self { client, ffprobe_path, ffmpeg_path })
    }

    async fn reconnect_client(client: &Arc<Mutex<Client>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let bot_token = std::env::var("TELOXIDE_TOKEN")?;
        let api_id: i32 = env::var("TELEGRAM_API_ID")?.parse()?;
        let api_hash = env::var("TELEGRAM_API_HASH")?;
        
        let session = Session::load_file_or_create(SESSION_FILE)?;
        
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
        
        let new_client = Client::connect(Config {
            session,
            api_id,
            api_hash,
            params,
        }).await?;

        // Проверим авторизацию и при необходимости переподключимся как бот
        if !new_client.is_authorized().await? {
            new_client.bot_sign_in(&bot_token).await?;
        }
        new_client.session().save_to_file(SESSION_FILE)?;
        
        // Заменим старый клиент на новый
        {
            let mut client_guard = client.lock().await;
            *client_guard = new_client;
        }
        
        Ok(())
    }

    pub async fn with_reconnect_retry<T, F, Fut>(&self, operation: F) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let max_retries = 3;
        for attempt in 0..max_retries {
            let result = operation().await;
            
            match result {
                Ok(value) => return Ok(value),
                Err(e) if e.to_string().contains("read 0 bytes") || 
                          e.to_string().contains("ConnectionReset") ||
                          e.to_string().contains("Connection lost") => {
                    log::warn!("Connection lost, reconnecting... (attempt {}/{})", attempt + 1, max_retries);
                    
                    if let Err(reconnect_err) = Self::reconnect_client(&self.client).await {
                        log::error!("Reconnection failed: {:?}", reconnect_err);
                        if attempt == max_retries - 1 {
                            return Err(e);
                        }
                    } else {
                        log::info!("Client reconnected successfully");
                        if attempt < max_retries - 1 {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
        
        // Если мы дошли до этой точки, то это ошибка, которая не связана с подключением
        // или все попытки переподключения были безуспешны
        Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Operation failed after retries")))
    }
}