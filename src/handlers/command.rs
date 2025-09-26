use teloxide::prelude::*;
use teloxide::types::{KeyboardMarkup, KeyboardButton};
use teloxide::utils::command::BotCommands;

use crate::commands::Command;
use crate::database::update_user_activity;

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

pub async fn command_handler(bot: Bot, msg: Message, cmd: Command) -> Result<(), anyhow::Error> {
    if let Err(e) = update_user_activity(msg.chat.id.0) { log::error!("Failed to update user activity: {}", e); }
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