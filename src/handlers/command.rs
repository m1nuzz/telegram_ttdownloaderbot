use teloxide::prelude::*;
use teloxide::types::{KeyboardMarkup, KeyboardButton};
use teloxide::utils::command::BotCommands;

use crate::commands::Command;
use crate::database::DatabasePool;
use std::sync::Arc;

pub fn get_main_reply_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new(vec![vec![
        KeyboardButton::new("⚙️ Settings"),
    ]])
    .resize_keyboard()
    .one_time_keyboard()
}

pub fn get_format_reply_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new(vec![
        vec![
            KeyboardButton::new("h265"),
            KeyboardButton::new("h264"),
            KeyboardButton::new("audio"),
        ],
        vec![
            KeyboardButton::new("Back"),
        ]
    ])
    .resize_keyboard()
    .one_time_keyboard()
}

pub fn get_subscription_reply_keyboard(subscription_required: bool) -> KeyboardMarkup {
    let toggle_button = if subscription_required {
        KeyboardButton::new("Disable Subscription")
    } else {
        KeyboardButton::new("Enable Subscription")
    };

    KeyboardMarkup::new(vec![vec![toggle_button],
                                vec![KeyboardButton::new("Back")]])
        .resize_keyboard()
        .one_time_keyboard()
}

pub async fn command_handler(bot: Bot, msg: Message, cmd: Command, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    let user_id = msg.chat.id.0;
    let result = db_pool.execute_with_timeout(move |conn| {
        conn.execute("INSERT OR IGNORE INTO users (telegram_id) VALUES (?1)", [user_id])?;
        conn.execute("UPDATE users SET last_active = CURRENT_TIMESTAMP WHERE telegram_id = ?1", [user_id])?;
        Ok(())
    }).await;
    
    if let Err(e) = result {
        log::error!("Failed to update user activity: {}", e);
    }
    
    match cmd {
        Command::Start => {
            bot.send_message(msg.chat.id, "Welcome! Send me a TikTok link.").reply_markup(get_main_reply_keyboard()).await?;
        }
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
        }
    };
    Ok(())
}