use anyhow::Result; 
use std::sync::Arc;
use teloxide::{
    prelude::*,
    requests::Requester,
    types::{ChatId, MessageId},
};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

#[derive(Clone)]
pub struct ProgressBar {
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    last_update: Option<tokio::time::Instant>, // Track last update time for throttling
}

impl ProgressBar {
    pub fn new(bot: Bot, chat_id: ChatId) -> Self {
        Self {
            bot,
            chat_id,
            message_id: None,
            last_update: None,
        }
    }

    pub fn new_silent() -> Self {
        Self::new(Bot::new("DUMMY_TOKEN"), ChatId(0))
    }

    pub async fn start(&mut self, initial_text: &str) -> Result<(), anyhow::Error> {
        let msg = self.bot.send_message(self.chat_id, initial_text).await?;
        self.message_id = Some(msg.id);
        self.last_update = Some(tokio::time::Instant::now());
        Ok(())
    }

    pub async fn update(
        &mut self,
        percentage: u8,
        extra_info: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        // Throttling: minimum 1000ms between updates to reduce API rate limiting
        const MIN_UPDATE_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_millis(1000);

        let now = tokio::time::Instant::now();

        // Check if enough time has passed since last update
        if let Some(last) = self.last_update {
            if now.duration_since(last) < MIN_UPDATE_INTERVAL && percentage < 100 {
                // Skip update if not enough time passed (except for 100% completion)
                // Additionally, skip updates that don't represent meaningful progress (at least 5% change)
                if percentage < 100 {
                    return Ok(());
                }
            }
        }

        // Update the time of last update
        self.last_update = Some(now);

        if let Some(message_id) = self.message_id {
            let progress_text = self.create_progress_bar(percentage, extra_info);
            let result = self
                .bot
                .edit_message_text(self.chat_id, message_id, progress_text)
                .await;

            // Handle API errors gracefully
            if let Err(e) = result {
                if !e.to_string().contains("message is not modified") {
                    log::warn!("Failed to update progress bar: {}", e);
                }
            }
        } else {
            // If there's no message ID yet, send a new message
            let progress_text = self.create_progress_bar(percentage, extra_info);
            let result = self.bot.send_message(self.chat_id, progress_text).await;
            if let Ok(msg) = result {
                self.message_id = Some(msg.id);
            } else {
                log::error!("Failed to send progress bar: {:?}", result.err());
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

    pub async fn delete(&mut self) -> Result<(), anyhow::Error> {
        if let Some(message_id) = self.message_id {
            let _ = self.bot.delete_message(self.chat_id, message_id).await;
            self.message_id = None;
        }
        Ok(())
    }
}
