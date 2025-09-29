use teloxide::prelude::*;
use teloxide::types::{CallbackQuery, InlineKeyboardMarkup, InlineKeyboardButton, KeyboardButton};
use rusqlite::params;
use tokio::fs;
use std::sync::Arc;

use crate::database::DatabasePool;
use crate::handlers::admin::is_admin;
use crate::handlers::command::{get_main_reply_keyboard, get_format_reply_keyboard, get_subscription_reply_keyboard};

pub async fn callback_handler(bot: Bot, q: CallbackQuery, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    if let Some(data) = q.data {
        log::info!("Received callback query with data: {}", data);

        if let Some(maybe_message) = q.message {
            if let Some(message) = maybe_message.regular_message() {
                if data.starts_with("set_quality_") {
                    let quality = data.split_at("set_quality_".len()).1;
                    let user_id = message.chat.id.0;
                    let quality_string = quality.to_string(); // Make a string copy
                    
                    // Use database pool for quality preference update
                    let result = db_pool.execute_with_timeout(move |conn| {
                        conn.execute(
                            "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
                            params![quality_string, user_id],
                        )
                    }).await;
                    
                    match result {
        Ok(_) => {
            // Invalidate the cache for this user to ensure the new quality setting is picked up immediately
            db_pool.invalidate_user_quality_cache(user_id).await;
            bot.answer_callback_query(q.id).text(&format!("Quality set to {}", quality)).await?;
        },
        Err(e) => {
            log::error!("Failed to update quality preference: {}", e);
            bot.answer_callback_query(q.id).text("Failed to update quality preference").await?;
        }
    }
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
                        // Using database pool with timeout
                        let result = db_pool.execute_with_timeout(|conn| {
                            conn.execute(
                                "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
                                params!["true"],
                            )
                        }).await;
                        
                        match result {
                            Ok(_) => {
                                // Update the environment variable asynchronously
                                if let Err(e) = update_env_subscription_setting(true).await {
                                    log::error!("Failed to update .env file: {}", e);
                                }
                                bot.answer_callback_query(q.id).text("Mandatory subscription enabled.").await?;
                            },
                            Err(e) => {
                                log::error!("Database operation failed: {}", e);
                                bot.answer_callback_query(q.id).text("Operation failed - please try again.").await?;
                            }
                        }
                        
                        // Refresh the menu
                        let subscription_required = db_pool.execute_with_timeout(|conn| {
                            match conn.query_row(
                                "SELECT value FROM settings WHERE key = 'subscription_required'",
                                [],
                                |row| Ok(row.get::<_, String>(0)? == "true")
                            ) {
                                Ok(value) => Ok(value),
                                Err(_) => Ok(true) // Default to true
                            }
                        }).await.unwrap_or(true);

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
                        // Using database pool with timeout
                        let result = db_pool.execute_with_timeout(|conn| {
                            conn.execute(
                                "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
                                params!["false"],
                            )
                        }).await;
                        
                        match result {
                            Ok(_) => {
                                // Update the environment variable asynchronously
                                if let Err(e) = update_env_subscription_setting(false).await {
                                    log::error!("Failed to update .env file: {}", e);
                                }
                                bot.answer_callback_query(q.id).text("Mandatory subscription disabled.").await?;
                            },
                            Err(e) => {
                                log::error!("Database operation failed: {}", e);
                                bot.answer_callback_query(q.id).text("Operation failed - please try again.").await?;
                            }
                        }
                        
                        // Refresh the menu
                        let subscription_required = db_pool.execute_with_timeout(|conn| {
                            match conn.query_row(
                                "SELECT value FROM settings WHERE key = 'subscription_required'",
                                [],
                                |row| Ok(row.get::<_, String>(0)? == "true")
                            ) {
                                Ok(value) => Ok(value),
                                Err(_) => Ok(true) // Default to true
                            }
                        }).await.unwrap_or(true);

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
                        let subscription_required = db_pool.execute_with_timeout(|conn| {
                            match conn.query_row(
                                "SELECT value FROM settings WHERE key = 'subscription_required'",
                                [],
                                |row| Ok(row.get::<_, String>(0)? == "true")
                            ) {
                                Ok(value) => Ok(value),
                                Err(_) => Ok(true) // Default to true
                            }
                        }).await.unwrap_or(true);

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

pub async fn subscription_text_handler(bot: Bot, msg: Message, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    if !is_admin(&msg).await {
        bot.send_message(msg.chat.id, "This option is for admins only.").await?;
        return Ok(());
    }

    let subscription_required: bool = db_pool.execute_with_timeout(|conn| {
        match conn.query_row("SELECT value FROM settings WHERE key = 'subscription_required'", [], |row| {
            let value: String = row.get(0)?;
            Ok(value == "true")
        }) {
            Ok(value) => Ok(value),
            Err(_) => Ok(true) // Default to true
        }
    }).await.unwrap_or(true);

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

pub async fn set_quality_h265_text_handler(bot: Bot, msg: Message, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    let result = db_pool.execute_with_timeout(move |conn| {
        conn.execute(
            "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
            params!["h265", msg.chat.id.0],
        )
    }).await;

    match result {
        Ok(_) => {
            // Invalidate the cache for this user to ensure the new quality setting is picked up immediately
            db_pool.invalidate_user_quality_cache(msg.chat.id.0).await;
            bot.send_message(msg.chat.id, "Quality set to h265.").reply_markup(get_format_reply_keyboard()).await?;
        },
        Err(e) => {
            log::error!("Failed to update quality preference to h265: {}", e);
            bot.send_message(msg.chat.id, "Failed to update quality preference.").await?;
        }
    }
    Ok(())
}

pub async fn set_quality_h264_text_handler(bot: Bot, msg: Message, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    let result = db_pool.execute_with_timeout(move |conn| {
        conn.execute(
            "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
            params!["h264", msg.chat.id.0],
        )
    }).await;

    match result {
        Ok(_) => {
            // Invalidate the cache for this user to ensure the new quality setting is picked up immediately
            db_pool.invalidate_user_quality_cache(msg.chat.id.0).await;
            bot.send_message(msg.chat.id, "Quality set to h264.").reply_markup(get_format_reply_keyboard()).await?;
        },
        Err(e) => {
            log::error!("Failed to update quality preference to h264: {}", e);
            bot.send_message(msg.chat.id, "Failed to update quality preference.").await?;
        }
    }
    Ok(())
}

pub async fn set_quality_audio_text_handler(bot: Bot, msg: Message, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    let result = db_pool.execute_with_timeout(move |conn| {
        conn.execute(
            "UPDATE users SET quality_preference = ?1 WHERE telegram_id = ?2",
            params!["audio", msg.chat.id.0],
        )
    }).await;

    match result {
        Ok(_) => {
            // Invalidate the cache for this user to ensure the new quality setting is picked up immediately
            db_pool.invalidate_user_quality_cache(msg.chat.id.0).await;
            bot.send_message(msg.chat.id, "Quality set to audio.").reply_markup(get_format_reply_keyboard()).await?;
        },
        Err(e) => {
            log::error!("Failed to update quality preference to audio: {}", e);
            bot.send_message(msg.chat.id, "Failed to update quality preference.").await?;
        }
    }
    Ok(())
}

pub async fn enable_subscription_text_handler(bot: Bot, msg: Message, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    let result = db_pool.execute_with_timeout(|conn| {
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
            params!["true"],
        )
    }).await;

    match result {
        Ok(_) => {
            if let Err(e) = update_env_subscription_setting(true).await {
                log::error!("Failed to update .env file: {}", e);
            }
            bot.send_message(msg.chat.id, "Mandatory subscription enabled.").reply_markup(get_subscription_reply_keyboard(true)).await?;
        },
        Err(e) => {
            log::error!("Database operation failed: {}", e);
            bot.send_message(msg.chat.id, "Operation failed - please try again.").await?;
        }
    }
    Ok(())
}

pub async fn disable_subscription_text_handler(bot: Bot, msg: Message, db_pool: Arc<DatabasePool>) -> Result<(), anyhow::Error> {
    let result = db_pool.execute_with_timeout(|conn| {
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'subscription_required'",
            params!["false"],
        )
    }).await;

    match result {
        Ok(_) => {
            if let Err(e) = update_env_subscription_setting(false).await {
                log::error!("Failed to update .env file: {}", e);
            }
            bot.send_message(msg.chat.id, "Mandatory subscription disabled.").reply_markup(get_subscription_reply_keyboard(false)).await?;
        },
        Err(e) => {
            log::error!("Database operation failed: {}", e);
            bot.send_message(msg.chat.id, "Operation failed - please try again.").await?;
        }
    }
    Ok(())
}
