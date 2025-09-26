use reqwest::multipart::{Form, Part};
use tokio::fs::File;
use teloxide::types::ChatId;
use crate::utils::progress_bar::ProgressBar;
use crate::utils::progress_reader::ProgressReader;
use tokio_util::io::ReaderStream;

pub async fn send_video_with_progress_botapi(
    bot_token: &str,
    chat_id: ChatId,
    file_path: &std::path::Path,
    caption: Option<&str>,
    progress_bar: &mut ProgressBar,
) -> anyhow::Result<()> {
    let file = File::open(file_path).await?;
    let len = file.metadata().await?.len();

    // 80..=100% - actual Bot API upload
    let pb_clone = progress_bar.clone();
    let reader = ProgressReader::new(file, len, move |uploaded, total| {
        let overall = 80.0 + (uploaded as f64 / total as f64) * 20.0;
        // Without await inside callback: move to task
        let mut pb2 = pb_clone.clone();
        let text = format!("📤 Uploading... {:.1}/{:.1} MB",
            uploaded as f64 / 1_048_576.0,
            total as f64 / 1_048_576.0);
        tokio::spawn(async move {
            let _ = pb2.update(overall.min(100.0) as u8, Some(&text)).await;
        });
    });

    let stream_reader = ReaderStream::new(reader);

    let part = Part::stream_with_length(reqwest::Body::wrap_stream(stream_reader), len)
        .file_name(file_path.file_name().unwrap().to_string_lossy().to_string())
        .mime_str("video/mp4")?;

    let form = Form::new()
        .text("chat_id", chat_id.0.to_string())
        .part("video", part)
        .text("supports_streaming", "true");

    let form = if let Some(c) = caption {
        form.text("caption", c.to_string())
    } else { form };

    let url = format!("https://api.telegram.org/bot{}/sendVideo", bot_token);
    let client = reqwest::Client::new();
    let resp = client.post(&url).multipart(form).send().await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Bot API sendVideo failed: {}", resp.status()));
    }

    // Success: hide progress bar immediately
    progress_bar.delete().await?;
    Ok(())
}

pub async fn send_audio_with_progress_botapi(
    bot_token: &str,
    chat_id: ChatId,
    file_path: &std::path::Path,
    caption: Option<&str>,
    progress_bar: &mut ProgressBar,
) -> anyhow::Result<()> {
    use reqwest::multipart::{Form, Part};
    use tokio_util::io::ReaderStream;
    use crate::utils::progress_reader::ProgressReader;
    use tokio::fs::File;

    let file = File::open(file_path).await?;
    let len = file.metadata().await?.len();

    let pb_clone = progress_bar.clone();
    let reader = ProgressReader::new(file, len, move |uploaded, total| {
        let overall = 80.0 + (uploaded as f64 / total as f64) * 20.0;
        let mut pb2 = pb_clone.clone();
        let text = format!("📤 Uploading... {:.1}/{:.1} MB",
            uploaded as f64 / 1_048_576.0,
            total as f64 / 1_048_576.0);
        tokio::spawn(async move {
            let _ = pb2.update(overall.min(100.0) as u8, Some(&text)).await;
        });
    });

    let stream_reader = ReaderStream::new(reader);

    let ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_lowercase();
    let mime = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" => "audio/ogg",
        _ => "audio/mpeg",
    };

    let part = Part::stream_with_length(reqwest::Body::wrap_stream(stream_reader), len)
        .file_name(file_path.file_name().unwrap().to_string_lossy().to_string())
        .mime_str(mime)?;

    let mut form = Form::new()
        .text("chat_id", chat_id.0.to_string())
        .part("audio", part);

    if let Some(c) = caption {
        form = form.text("caption", c.to_string());
    }

    let url = format!("https://api.telegram.org/bot{}/sendAudio", bot_token);
    let client = reqwest::Client::new();
    let resp = client.post(&url).multipart(form).send().await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Bot API sendAudio failed: {}", resp.status()));
    }

    progress_bar.delete().await?;
    Ok(())
}

