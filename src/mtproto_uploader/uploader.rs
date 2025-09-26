
use grammers_client::{Client, Config};
use grammers_session::Session;
use std::env;
use std::path::PathBuf;

use crate::mtproto_uploader::constants::SESSION_FILE;

#[derive(Clone)]
pub struct MTProtoUploader {
    pub client: Client,
    pub ffprobe_path: PathBuf,
}

impl MTProtoUploader {
    pub async fn new(bot_token: &str, ffprobe_path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let api_id: i32 = env::var("TELEGRAM_API_ID")?.parse()?;
        let api_hash = env::var("TELEGRAM_API_HASH")?;

        let session = Session::load_file_or_create(SESSION_FILE)?;
        let client = Client::connect(Config {
            session,
            api_id,
            api_hash: api_hash.clone(),
            params: Default::default(),
        }).await?;

        if !client.is_authorized().await? {
            client.bot_sign_in(bot_token).await?;
        }
        client.session().save_to_file(SESSION_FILE)?;

        Ok(Self { client, ffprobe_path })
    }
}