use crate::peers::resolve_peer;

use anyhow;
use grammers_tl_types as tl;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use log;

use crate::utils::progress_bar::ProgressBar;

use crate::mtproto_uploader::uploader::MTProtoUploader; // Import MTProtoUploader

impl MTProtoUploader {
    pub async fn upload_audio(
        &self,
        chat_id: i64,
        username: Option<String>,
        file_path: &Path,
        caption: &str,
        progress_bar: &mut ProgressBar,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let input_peer = resolve_peer(&self.client, chat_id, username.as_deref()).await?;

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
            self.client.invoke(&request).await.map_err(|e| anyhow::anyhow!("saveBigFilePart {} failed: {:?}", part, e))?;

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
            name: file_path
                .file_name()
                .and_then(|os_str| os_str.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    log::error!("Failed to extract file name from path: {:?}", file_path);
                    anyhow::anyhow!("Failed to extract file name from path")
                })?,
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
        Ok(())
    }
}