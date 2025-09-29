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
    chatid: ChatId,
    messageid: Option<MessageId>,
    lastupdate: Arc<Mutex<Instant>>,
    minupdateinterval: Duration,
}

impl ProgressBar {
    pub fn new(bot: Bot, chatid: ChatId) -> Self {
        Self {
            bot,
            chatid,
            messageid: None,
            lastupdate: Arc::new(Mutex::new(Instant::now())),
            minupdateinterval: Duration::from_millis(500),
        }
    }

    pub fn new_silent() -> Self {
        Self::new(Bot::new("DUMMY_TOKEN"), ChatId(0))
    }

    pub async fn start(&mut self, initial_text: &str) -> Result<(), anyhow::Error> {
        let msg = self.bot.send_message(self.chatid, initial_text).await?;
        self.messageid = Some(msg.id);
        *self.lastupdate.lock().await = Instant::now();
        Ok(())
    }

    pub async fn update(
        &mut self,
        percentage: u8,
        extra_info: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        let now = Instant::now();
        let mut last_update = self.lastupdate.lock().await;
        if self.messageid.is_none()
            || now.duration_since(*last_update) > self.minupdateinterval
            || percentage == 100
        {
            *last_update = now;
            if let Some(message_id) = self.messageid {
                let progress_text = self.create_progress_bar(percentage, extra_info);
                let _ = self
                    .bot
                    .edit_message_text(self.chatid, message_id, progress_text)
                    .await;
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
        if let Some(message_id) = self.messageid {
            let _ = self.bot.delete_message(self.chatid, message_id).await;
            self.messageid = None;
        }
        Ok(())
    }
}
