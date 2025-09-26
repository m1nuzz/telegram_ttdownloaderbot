use anyhow::anyhow;
use grammers_client::{Client, Config};
use grammers_session::Session;
use grammers_tl_types as tl;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::env;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::utils::progress_bar::ProgressBar;

const SESSION_FILE: &str = "telegram.session";

#[derive(Clone)]
pub struct MTProtoUploader {
    client: Client,
}

impl MTProtoUploader {
    pub async fn new(bot_token: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(Self { client })
    }

    pub async fn upload_video(
        &self,
        chat_id: i64,
        file_path: &Path,
        caption: &str,
        progress_bar: &mut ProgressBar,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ... getting input_peer and initialization ...
        let user = self.client.get_me().await?;
        let input_peer = if chat_id == user.id() {
            tl::enums::InputPeer::PeerSelf
        } else {
            let mut dialogs = self.client.iter_dialogs();
            let mut target_peer = None;
            while let Some(dialog) = dialogs.next().await? {
                if let grammers_client::types::Chat::User(user_chat) = dialog.chat {
                    if user_chat.id() == chat_id {
                        target_peer = Some(tl::enums::InputPeer::User(tl::types::InputPeerUser {
                            user_id: user_chat.id(),
                            access_hash: user_chat.raw.access_hash.ok_or_else(|| anyhow!("User access hash not found"))?,
                        }));
                        break;
                    }
                }
            }
            target_peer.ok_or_else(|| anyhow!("User not found"))?
        };

        let file = File::open(file_path)?;
        let mut reader = BufReader::new(file);
        let file_size = file_path.metadata()?.len() as usize;
        let chunk_size = 512 * 1024; // 512 KB
        let total_parts = (file_size + chunk_size - 1) / chunk_size;

        let mut rng = ChaCha8Rng::from_os_rng();
        let file_id: i64 = rand::Rng::random(&mut rng);

        // Uploading file in parts
        for part in 0..total_parts {
            let mut bytes = vec![0; chunk_size];
            let bytes_read = reader.read(&mut bytes)?;
            bytes.truncate(bytes_read);

            let request = tl::functions::upload::SaveBigFilePart {
                file_id,
                file_part: part as i32,
                file_total_parts: total_parts as i32,
                bytes,
            };
            self.client.invoke(&request).await?;

            // 2) calculate overall progress (80..=99)
            let uploaded = part + 1;
            let overall = 80 + ((uploaded as f64 / total_parts as f64) * 19.0).floor() as u8;
            // showing "real" upload
            let info = format!("📤 Uploading... {}/{} parts", uploaded, total_parts);
            let _ = progress_bar.update(overall.min(99), Some(&info)).await;
        }

        // Creating media object
        let input_file = tl::enums::InputFile::Big(tl::types::InputFileBig {
            id: file_id,
            parts: total_parts as i32,
            name: file_path.file_name().unwrap().to_str().unwrap().to_string(),
        });
        let media = tl::enums::InputMedia::UploadedDocument(tl::types::InputMediaUploadedDocument {
            nosound_video: false,
            spoiler: false,
            file: input_file,
            thumb: None,
            mime_type: "video/mp4".to_string(),
            force_file: false,
            attributes: vec![tl::enums::DocumentAttribute::Video(tl::types::DocumentAttributeVideo {
                round_message: false,
                supports_streaming: true,
                nosound: false,
                duration: 0.0,
                w: 0,
                h: 0,
                preload_prefix_size: None,
                video_start_ts: None,
            })],
            stickers: Some(Vec::new()),
            ttl_seconds: None,
        });

        // Sending message
        let request = tl::functions::messages::SendMedia {
            silent: false,
            background: false,
            clear_draft: false,
            noforwards: false,
            update_stickersets_order: false,
            peer: input_peer,
            reply_to: None,
            media,
            message: caption.to_string(),
            random_id: rand::Rng::random(&mut rng),
            reply_markup: None,
            entities: Some(Vec::new()),
            schedule_date: None,
            send_as: None,
            effect: None,
            invert_media: false,
            quick_reply_shortcut: None,
        };
        self.client.invoke(&request).await?;

        // Set 100% and immediately delete the progress bar in the calling code (link.rs)
        Ok(())
    }

    pub async fn upload_audio(
        &self,
        chat_id: i64,
        file_path: &Path,
        caption: &str,
        progress_bar: &mut ProgressBar,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ... same code for input_peer and initialization ...
        let user = self.client.get_me().await?;
        let input_peer = if chat_id == user.id() {
            tl::enums::InputPeer::PeerSelf
        } else {
            let mut dialogs = self.client.iter_dialogs();
            let mut target_peer = None;
            while let Some(dialog) = dialogs.next().await? {
                if let grammers_client::types::Chat::User(user_chat) = dialog.chat {
                    if user_chat.id() == chat_id {
                        target_peer = Some(tl::enums::InputPeer::User(tl::types::InputPeerUser {
                            user_id: user_chat.id(),
                            access_hash: user_chat.raw.access_hash.ok_or_else(|| anyhow!("User access hash not found"))?,
                        }));
                        break;
                    }
                }
            }
            target_peer.ok_or_else(|| anyhow!("User not found"))?
        };

        let file = File::open(file_path)?;
        let mut reader = BufReader::new(file);
        let file_size = file_path.metadata()?.len() as usize;
        let chunk_size = 512 * 1024; // 512 KB
        let total_parts = (file_size + chunk_size - 1) / chunk_size;

        let mut rng = ChaCha8Rng::from_os_rng();
        let file_id: i64 = rand::Rng::random(&mut rng);

        // Uploading file in parts
        for part in 0..total_parts {
            let mut bytes = vec![0; chunk_size];
            let bytes_read = reader.read(&mut bytes)?;
            bytes.truncate(bytes_read);

            let request = tl::functions::upload::SaveBigFilePart {
                file_id,
                file_part: part as i32,
                file_total_parts: total_parts as i32,
                bytes,
            };
            self.client.invoke(&request).await?;

            // 2) calculate overall progress (80..=99)
            let uploaded = part + 1;
            let overall = 80 + ((uploaded as f64 / total_parts as f64) * 19.0).floor() as u8;
            // showing "real" upload
            let info = format!("📤 Uploading... {}/{} parts", uploaded, total_parts);
            let _ = progress_bar.update(overall.min(99), Some(&info)).await;
        }

        let input_file = tl::enums::InputFile::Big(tl::types::InputFileBig {
            id: file_id,
            parts: total_parts as i32,
            name: file_path.file_name().unwrap().to_str().unwrap().to_string(),
        });

        let ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let mime = match ext.as_str() {
            "mp3" => "audio/mpeg",
            "m4a" => "audio/mp4",
            "aac" => "audio/aac",
            "ogg" => "audio/ogg",
            _ => "audio/mpeg",
        }.to_string();

        let audio_attr = tl::enums::DocumentAttribute::Audio(tl::types::DocumentAttributeAudio {
            voice: false,
            duration: 0,              // optionally calculate beforehand
            title: None,
            performer: None,
            waveform: None,
        });

        let media = tl::enums::InputMedia::UploadedDocument(tl::types::InputMediaUploadedDocument {
            nosound_video: false,
            spoiler: false,
            file: input_file,
            thumb: None,
            mime_type: mime,
            force_file: false,
            attributes: vec![audio_attr],
            stickers: Some(Vec::new()),
            ttl_seconds: None,
        });

        // Sending message
        let request = tl::functions::messages::SendMedia {
            silent: false,
            background: false,
            clear_draft: false,
            noforwards: false,
            update_stickersets_order: false,
            peer: input_peer,
            reply_to: None,
            media,
            message: caption.to_string(),
            random_id: rand::Rng::random(&mut rng),
            reply_markup: None,
            entities: Some(Vec::new()),
            schedule_date: None,
            send_as: None,
            effect: None,
            invert_media: false,
            quick_reply_shortcut: None,
        };
        self.client.invoke(&request).await?;

        // Set 100% and immediately delete the progress bar in the calling code (link.rs)
        Ok(())
    }
}