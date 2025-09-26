use rusqlite::{Connection, Result};
use std::env;

pub fn init_database() -> Result<()> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, telegram_id BIGINT UNIQUE NOT NULL, last_active DATETIME DEFAULT CURRENT_TIMESTAMP, quality_preference TEXT DEFAULT 'h264')",
        (),
    )?;
    // Add the quality_preference column to the users table if it doesn't exist, ignoring the error if it does.
    let _ = conn.execute("ALTER TABLE users ADD COLUMN quality_preference TEXT DEFAULT 'h264'", ());

    conn.execute(
        "CREATE TABLE IF NOT EXISTS downloads (id INTEGER PRIMARY KEY, user_id INTEGER, video_url TEXT NOT NULL, download_date DATETIME DEFAULT CURRENT_TIMESTAMP, FOREIGN KEY (user_id) REFERENCES users (id))",
        (),
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS admins (id INTEGER PRIMARY KEY, admin_telegram_id BIGINT UNIQUE NOT NULL)",
        (),
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS channels (id INTEGER PRIMARY KEY, channel_id TEXT UNIQUE NOT NULL, channel_name TEXT)",
        (),
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        (),
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('subscription_required', 'true')",
        (),
    )?;
    Ok(())
}

pub fn update_user_activity(user_id: i64) -> Result<()> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path)?;
    conn.execute("INSERT OR IGNORE INTO users (telegram_id) VALUES (?1)", [user_id])?;
    conn.execute("UPDATE users SET last_active = CURRENT_TIMESTAMP WHERE telegram_id = ?1", [user_id])?;
    Ok(())
}

pub fn log_download(telegram_id: i64, video_url: &str) -> Result<()> {
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path)?;
    let user_id: i64 = conn.query_row("SELECT id FROM users WHERE telegram_id = ?1", [telegram_id], |row| row.get(0))?;
    conn.execute("INSERT INTO downloads (user_id, video_url) VALUES (?1, ?2)", (user_id, video_url))?;
    Ok(())
}