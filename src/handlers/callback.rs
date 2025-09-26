use teloxide::prelude::*;
use teloxide::types::{CallbackQuery, InlineKeyboardMarkup, InlineKeyboardButton, KeyboardButton};
use rusqlite::{Connection, params};
use std::env;
use tokio::fs;
use std::sync::Arc;

use crate::handlers::admin::is_admin;
use crate::handlers::command::{get_main_reply_keyboard, get_format_reply_keyboard, get_subscription_reply_keyboard};

pub async fn callback_handler(bot: Bot, q: CallbackQuery) -> Result<(), anyhow::Error> {
    if let Some(data) = q.data {
        log::info!("Received callback query with data: {}", data);

        if let Some(maybe_message) = q.message {
            if let Some(message) = maybe_message.regular_message() {
                if data.starts_with("set_quality_") {
                    let quality = data.split_at("set_quality_".len()).1;
                    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
                    let conn = Connection::open(db_path).unwrap();
                    conn.execute(
                        "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
                        params![quality, message.chat.id.0],
                    ).unwrap();
                    bot.answer_callback_query(q.id).text(&format!("Quality set to {}", quality)).await?;
                } else {
                    match data.as_str() {
                        "settings" => {
                            let mut keyboard_rows = vec![vec![
                                InlineKeyboardButton::callback("Format", "format_menu"),
                            ]];

                            if is_admin(&message).await {
                                keyboard_rows.push(vec![
                                    InlineKeyboardButton::callback("Subscription", "subscription_menu"),
                                ]);
                            }

                            keyboard_rows.push(vec![
                                InlineKeyboardButton::callback("Back", "back_to_main"),
                            ]);

                            let keyboard = InlineKeyboardMarkup::new(keyboard_rows);

                            bot.edit_message_text(message.chat.id, message.id, "Settings").await?;
                            bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                        }
                        "format_menu" => {
                            let keyboard = InlineKeyboardMarkup::new(vec![ 
                                vec![ 
                                    InlineKeyboardButton::callback("h265", "set_quality_h265"),
                                    InlineKeyboardButton::callback("h264", "set_quality_h264"),
                                    InlineKeyboardButton::callback("audio", "set_quality_audio"),
                                ],
                                vec![ 
                                    InlineKeyboardButton::callback("Back", "back_to_settings"),
                                ]
                            ]);
                            let text = "h265: best quality, but may not work on some devices.\nh264: worse quality, but works on many devices.\naudio: audio only";
                            bot.edit_message_text(message.chat.id, message.id, text).await?;
                            bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                        }
                        "back_to_main" => {
                            let keyboard = InlineKeyboardMarkup::new(vec![vec![ 
                                InlineKeyboardButton::callback("Settings", "settings"),
                            ]]);
                            bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                            bot.send_message(message.chat.id, "").reply_markup(get_main_reply_keyboard()).await?;
                        }
                        "back_to_settings" => {
                            let keyboard = InlineKeyboardMarkup::new(vec![vec![ 
                                InlineKeyboardButton::callback("Format", "format_menu"),
                            ],
                            vec![ 
                                InlineKeyboardButton::callback("Back", "back_to_main"),
                            ]]);

                            bot.edit_message_text(message.chat.id, message.id, "Settings").await?;
                            bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                        }
                    "toggle_subscription" => {
                        // This arm is no longer needed as toggle logic is handled by enable/disable
                        bot.answer_callback_query(q.id).text("Action not available.").await?;
                    }
                    "enable_subscription" => {
                        let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
                        let db_path_cloned = Arc::new(db_path.clone());
                        let _result: Result<bool, rusqlite::Error> = tokio::task::spawn_blocking(move || {
                            let conn = Connection::open(&*db_path_cloned)?;
                            conn.execute(
                                "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
                                params!["true"],
                            )?;
                            Ok(true)
                        }).await.unwrap();
                        update_env_subscription_setting(true).await?;
                        bot.answer_callback_query(q.id).text("Mandatory subscription enabled.").await?;
                        // Refresh the menu
                        let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
                        let subscription_required: bool = {
                            let conn = Connection::open(&db_path).unwrap();
                            conn.query_row("SELECT value FROM settings WHERE key = 'subscription_required'", [], |row| {
                                let value: String = row.get(0)?;
                                Ok(value == "true")
                            }).unwrap_or(true)
                        };

                        let toggle_button = if subscription_required {
                            InlineKeyboardButton::callback("Disable Subscription", "disable_subscription")
                        } else {
                            InlineKeyboardButton::callback("Enable Subscription", "enable_subscription")
                        };

                        let keyboard = InlineKeyboardMarkup::new(vec![vec![toggle_button],
                                                                    vec![InlineKeyboardButton::callback("Back", "back_to_settings")]]);

                        bot.edit_message_text(message.chat.id, message.id, "Manage Subscription").await?;
                        bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                    }
                    "disable_subscription" => {
                        let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
                        let db_path_cloned = Arc::new(db_path.clone());
                        let _result: Result<bool, rusqlite::Error> = tokio::task::spawn_blocking(move || {
                            let conn = Connection::open(&*db_path_cloned)?;
                            conn.execute(
                                "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
                                params!["false"],
                            )?;
                            Ok(false)
                        }).await.unwrap();
                        update_env_subscription_setting(false).await?;
                        bot.answer_callback_query(q.id).text("Mandatory subscription disabled.").await?;
                        // Refresh the menu
                        let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
                        let subscription_required: bool = {
                            let conn = Connection::open(&db_path).unwrap();
                            conn.query_row("SELECT value FROM settings WHERE key = 'subscription_required'", [], |row| {
                                let value: String = row.get(0)?;
                                Ok(value == "true")
                            }).unwrap_or(true)
                        };

                        let toggle_button = if subscription_required {
                            InlineKeyboardButton::callback("Disable Subscription", "disable_subscription")
                        } else {
                            InlineKeyboardButton::callback("Enable Subscription", "enable_subscription")
                        };

                        let keyboard = InlineKeyboardMarkup::new(vec![vec![toggle_button],
                                                                    vec![InlineKeyboardButton::callback("Back", "back_to_settings")]]);

                        bot.edit_message_text(message.chat.id, message.id, "Manage Subscription").await?;
                        bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                    }
                    "subscription_menu" => {
                        let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
                        let subscription_required: bool = {
                            let conn = Connection::open(&db_path).unwrap();
                            conn.query_row("SELECT value FROM settings WHERE key = 'subscription_required'", [], |row| {
                                let value: String = row.get(0)?;
                                Ok(value == "true")
                            }).unwrap_or(true)
                        };

                        let toggle_button = if subscription_required {
                            InlineKeyboardButton::callback("Disable Subscription", "disable_subscription")
                        } else {
                            InlineKeyboardButton::callback("Enable Subscription", "enable_subscription")
                        };

                        let keyboard = InlineKeyboardMarkup::new(vec![vec![toggle_button],
                                                                    vec![InlineKeyboardButton::callback("Back", "back_to_settings")]]);

                        bot.edit_message_text(message.chat.id, message.id, "Manage Subscription").await?;
                        bot.edit_message_reply_markup(message.chat.id, message.id).reply_markup(keyboard).await?;
                    }
                    _ => {}                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn update_env_subscription_setting(enable: bool) -> Result<(), anyhow::Error> {
    let env_path = ".env";
    let content = fs::read_to_string(env_path).await?;
    let mut new_content = String::new();
    let mut found = false;

    for line in content.lines() {
        if line.starts_with("SUBSCRIPTION_REQUIRED=") {
            new_content.push_str(&format!("SUBSCRIPTION_REQUIRED={}", enable));
            found = true;
        } else {
            new_content.push_str(line);
        }
        new_content.push_str("\n");
    }

    if !found {
        new_content.push_str(&format!("SUBSCRIPTION_REQUIRED={}\n", enable));
    }

    fs::write(env_path, new_content).await?;
    Ok(())
}

pub async fn settings_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let mut keyboard_rows = vec![vec![
        KeyboardButton::new("Format"),
    ]];

    if is_admin(&msg).await {
        keyboard_rows.push(vec![
            KeyboardButton::new("Subscription"),
        ]);
    }

    keyboard_rows.push(vec![
        KeyboardButton::new("Back"),
    ]);

    let keyboard = teloxide::types::KeyboardMarkup::new(keyboard_rows)
        .resize_keyboard()
        .one_time_keyboard();

    bot.send_message(msg.chat.id, "Settings").reply_markup(keyboard).await?;

    Ok(())
}

pub async fn format_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let keyboard = teloxide::types::KeyboardMarkup::new(vec![
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
    .one_time_keyboard();

    let text = "h265: best quality, but may not work on some devices.\nh264: worse quality, but works on many devices.\naudio: audio only";
    bot.send_message(msg.chat.id, text).reply_markup(keyboard).await?;

    Ok(())
}

pub async fn subscription_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    if !is_admin(&msg).await {
        bot.send_message(msg.chat.id, "This option is for admins only.").await?;
        return Ok(());
    }

    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let subscription_required: bool = {
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row("SELECT value FROM settings WHERE key = 'subscription_required'", [], |row| {
            let value: String = row.get(0)?;
            Ok(value == "true")
        }).unwrap_or(true)
    };

    let toggle_button = if subscription_required {
        KeyboardButton::new("Disable Subscription")
    } else {
        KeyboardButton::new("Enable Subscription")
    };

    let keyboard = teloxide::types::KeyboardMarkup::new(vec![vec![toggle_button],
                                                                vec![KeyboardButton::new("Back")]])
        .resize_keyboard()
        .one_time_keyboard();

    bot.send_message(msg.chat.id, "Manage Subscription").reply_markup(keyboard).await?;

    Ok(())
}

pub async fn back_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    bot.send_message(msg.chat.id, "Returning to main menu.").reply_markup(get_main_reply_keyboard()).await?;
    Ok(())
}

pub async fn set_quality_h265_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
        params!["h265", msg.chat.id.0],
    ).unwrap();
    bot.send_message(msg.chat.id, "Quality set to h265.").reply_markup(get_format_reply_keyboard()).await?;
    Ok(())
}

pub async fn set_quality_h264_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
        params!["h264", msg.chat.id.0],
    ).unwrap();
    bot.send_message(msg.chat.id, "Quality set to h264.").reply_markup(get_format_reply_keyboard()).await?;
    Ok(())
}

pub async fn set_quality_audio_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
        params!["audio", msg.chat.id.0],
    ).unwrap();
    bot.send_message(msg.chat.id, "Quality set to audio.").reply_markup(get_format_reply_keyboard()).await?;
    Ok(())
}

pub async fn enable_subscription_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let db_path_cloned = Arc::new(db_path.clone());
    let _result: Result<bool, rusqlite::Error> = tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path_cloned)?;
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
            params!["true"],
        )?;
        Ok(true)
    }).await.unwrap();
    update_env_subscription_setting(true).await?;
    bot.send_message(msg.chat.id, "Mandatory subscription enabled.").reply_markup(get_subscription_reply_keyboard(true)).await?;
    Ok(())
}

pub async fn disable_subscription_text_handler(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let db_path_cloned = Arc::new(db_path.clone());
    let _result: Result<bool, rusqlite::Error> = tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path_cloned)?;
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
            params!["false"],
        )?;
        Ok(false)
    }).await.unwrap();
    update_env_subscription_setting(false).await?;
    bot.send_message(msg.chat.id, "Mandatory subscription disabled.").reply_markup(get_subscription_reply_keyboard(false)).await?;
    Ok(())
}
