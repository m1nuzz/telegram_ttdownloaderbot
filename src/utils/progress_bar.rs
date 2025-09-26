use teloxide::{prelude::*, types::{MessageId, ChatId}, requests::Requester};
use tokio::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;

#[derive(Clone)]
pub struct ProgressBar {
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    last_update: Arc<Mutex<Instant>>,
    min_update_interval: Duration,
}

impl ProgressBar {
    pub fn new(bot: Bot, chat_id: ChatId) -> Self {
        Self {
            bot,
            chat_id,
            message_id: None,
            last_update: Arc::new(Mutex::new(Instant::now())),
            min_update_interval: Duration::from_millis(500), // Update more frequently for smoothness
        }
    }

    pub fn new_silent() -> Self {
        Self {
            bot: Bot::new("DUMMY_TOKEN"), // Dummy token, as it won't be used
            chat_id: ChatId(0), // Dummy chat_id
            message_id: None,
            last_update: Arc::new(Mutex::new(Instant::now())),
            min_update_interval: Duration::from_millis(500),
        }
    }

    pub async fn start(&mut self, initial_text: &str) -> Result<(), anyhow::Error> {
        let msg = self.bot.send_message(self.chat_id, initial_text).await?;
        self.message_id = Some(msg.id);
        *self.last_update.lock().await = Instant::now();
        Ok(())
    }

    pub async fn update(&mut self, overall_percentage: u8, extra_info: Option<&str>) -> Result<(), anyhow::Error> {
        self.update_internal(overall_percentage, extra_info).await
    }

    async fn update_internal(&mut self, percentage: u8, extra_info: Option<&str>) -> Result<(), anyhow::Error> {
        let now = Instant::now();
        let mut last_update = self.last_update.lock().await;
        if self.message_id.is_none() || (now.duration_since(*last_update) >= self.min_update_interval) || percentage >= 100 {
            *last_update = now;
            if let Some(message_id) = self.message_id {
                let progress_text = self.create_progress_bar(percentage, extra_info);
                let _ = self.bot.edit_message_text(self.chat_id, message_id, progress_text).await;
            }
        }
        Ok(())
    }

    fn create_progress_bar(&self, percentage: u8, extra_info: Option<&str>) -> String {
        let bar_length = 20;
        let filled_length = (percentage as f32 / 100.0 * bar_length as f32) as usize;
        let mut bar = String::new();
        bar.push('[');
        for i in 0..bar_length {
            if i < filled_length {
                bar.push('█');
            } else {
                bar.push('░');
            }
        }
        bar.push(']');
        let mut result = format!("🎬 Processing: {}%\n{}", percentage, bar);
        if let Some(info) = extra_info {
            result.push_str(&format!("\n{}", info));
        }
        result
    }

    // Deletes the progress bar (used when everything is ready)
    pub async fn delete(&mut self) -> Result<(), anyhow::Error> {
        if let Some(message_id) = self.message_id {
            let _ = self.bot.delete_message(self.chat_id, message_id).await;
            self.message_id = None;
        }
        Ok(())
    }
}