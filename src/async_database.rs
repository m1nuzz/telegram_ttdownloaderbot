use anyhow::Result;
use chrono::Utc;
use tokio_rusqlite::{Connection, rusqlite};

#[derive(Debug, PartialEq)]
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub username: Option<String>,
    pub quality_preference: String,
    pub subscription_enabled: bool,
}

pub async fn init_database() -> Result<()> {
    let db_path = std::env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path).await?;
    
    conn.call(|conn| {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY, 
                telegram_id BIGINT UNIQUE NOT NULL, 
                last_active DATETIME DEFAULT CURRENT_TIMESTAMP, 
                quality_preference TEXT DEFAULT 'h264'
            )",
            (),
        )?;
        
        // Add the quality_preference column to the users table if it doesn't exist
        let _ = conn.execute("ALTER TABLE users ADD COLUMN quality_preference TEXT DEFAULT 'h264'", ());

        // Create the table with the new format
        conn.execute(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY, 
                user_telegram_id BIGINT, 
                video_url TEXT NOT NULL, 
                download_date DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            (),
        )?;
        
        // Check if the old format table exists
        let has_old_format: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='downloads' AND sql LIKE '%user_id INTEGER%'",
            (),
            |row| row.get(0)
        ).unwrap_or(0) > 0;
        
        if has_old_format {
            // Check if we need to migrate (if there's data in the old format)
            let has_data: bool = conn.query_row(
                "SELECT COUNT(*) FROM downloads",
                (),
                |row| row.get(0)
            ).unwrap_or(0) > 0;
            
            if has_data {
                // Create a temporary table with the new structure
                conn.execute(
                    "CREATE TEMPORARY TABLE downloads_migrated AS SELECT d.id, u.telegram_id as user_telegram_id, d.video_url, d.download_date FROM downloads d JOIN users u ON d.user_id = u.id",
                    (),
                )?;
                
                // Drop the old table
                conn.execute("DROP TABLE downloads", ())?;
                
                // Recreate with new format
                conn.execute(
                    "CREATE TABLE downloads (
                        id INTEGER PRIMARY KEY, 
                        user_telegram_id BIGINT, 
                        video_url TEXT NOT NULL, 
                        download_date DATETIME DEFAULT CURRENT_TIMESTAMP
                    )",
                    (),
                )?;
                
                // Copy data from temporary table
                conn.execute(
                    "INSERT INTO downloads (id, user_telegram_id, video_url, download_date) SELECT id, user_telegram_id, video_url, download_date FROM downloads_migrated",
                    (),
                )?;
            } else {
                // If no data in old format, just drop and recreate
                conn.execute("DROP TABLE downloads", ())?;
                conn.execute(
                    "CREATE TABLE downloads (
                        id INTEGER PRIMARY KEY, 
                        user_telegram_id BIGINT, 
                        video_url TEXT NOT NULL, 
                        download_date DATETIME DEFAULT CURRENT_TIMESTAMP
                    )",
                    (),
                )?;
            }
        }
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS admins (
                id INTEGER PRIMARY KEY, 
                admin_telegram_id BIGINT UNIQUE NOT NULL
            )",
            (),
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channels (
                id INTEGER PRIMARY KEY, 
                channel_id TEXT UNIQUE NOT NULL, 
                channel_name TEXT
            )",
            (),
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY, 
                value TEXT NOT NULL
            )",
            (),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('subscription_required', 'true')",
            (),
        )?;
        Ok(())
    }).await?;
    
    Ok(())
}

pub async fn update_user_activity(user_id: i64) -> Result<()> {
    let db_path = std::env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path).await?;
    
    conn.call(move |conn| {
        conn.execute("INSERT OR IGNORE INTO users (telegram_id) VALUES (?1)", [user_id])?;
        conn.execute("UPDATE users SET last_active = CURRENT_TIMESTAMP WHERE telegram_id = ?1", [user_id])?;
        Ok(())
    }).await?;
    
    Ok(())
}

pub async fn log_download(telegram_id: i64, video_url: &str) -> Result<()> {
    let db_path = std::env::var("DATABASE_PATH").expect("DATABASE_PATH must be set");
    let conn = Connection::open(db_path).await?;
    
    conn.call(move |conn| {
        // Update user activity first (to ensure the user exists in the database)
        conn.execute("INSERT OR IGNORE INTO users (telegram_id) VALUES (?1)", [telegram_id])?;
        conn.execute("UPDATE users SET last_active = CURRENT_TIMESTAMP WHERE telegram_id = ?1", [telegram_id])?;
        conn.execute("INSERT INTO downloads (user_telegram_id, video_url) VALUES (?1, ?2)", (telegram_id, video_url))?;
        Ok(())
    }).await?;
    
    Ok(())
}