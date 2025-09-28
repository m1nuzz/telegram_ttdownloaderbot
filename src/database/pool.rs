use rusqlite::{Connection, Result as SqliteResult, params};
use tokio::sync::{Semaphore, Mutex};
use tokio::time::{timeout, Duration};
use std::sync::Arc;
use std::collections::HashMap;

pub struct DatabasePool {
    db_path: String,
    connection_semaphore: Arc<Semaphore>,
    // Cache for frequently used data
    user_cache: Arc<Mutex<HashMap<i64, UserInfo>>>,
}

#[derive(Clone)]
pub struct UserInfo {
    pub quality_preference: String,
    pub last_updated: tokio::time::Instant,
}

impl DatabasePool {
    pub fn new(db_path: String, max_connections: usize) -> Self {
        Self {
            db_path,
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            user_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute database operation with timeout and proper error handling
    pub async fn execute_with_timeout<F, R>(&self, operation: F) -> Result<R, anyhow::Error>
    where
        F: FnOnce(&Connection) -> SqliteResult<R> + Send + 'static,
        R: Send + 'static,
    {
        let _permit = timeout(
            Duration::from_secs(5),
            self.connection_semaphore.acquire()
        ).await??;
        
        let db_path = self.db_path.clone();
        let result = timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let conn = Connection::open(&db_path)?;
                
                // Optimize SQLite for concurrent access
                conn.execute_batch(
                    "PRAGMA journal_mode = WAL;
                     PRAGMA synchronous = NORMAL;
                     PRAGMA cache_size = 32000;
                     PRAGMA temp_store = MEMORY;
                     PRAGMA busy_timeout = 5000;"
                )?;
                
                operation(&conn)
            })
        ).await?;
        
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(anyhow::anyhow!(e)),
            Err(e) => Err(anyhow::anyhow!("Timeout: {}", e)),
        }
    }

    /// Get user quality preference with caching
    pub async fn get_user_quality(&self, user_id: i64) -> Result<String, anyhow::Error> {
        // Check cache
        {
            let mut cache = self.user_cache.lock().await;
            if let Some(user_info) = cache.get(&user_id) {
                // Cache is valid for 5 minutes
                if user_info.last_updated.elapsed() < Duration::from_secs(300) {
                    return Ok(user_info.quality_preference.clone());
                }
                // Remove expired entry
                cache.remove(&user_id);
            }
        }

        // Load from DB
        let quality = self.execute_with_timeout(move |conn| {
            match conn.query_row(
                "SELECT quality_preference FROM users WHERE telegram_id = ?1",
                params![user_id],
                |row| Ok(row.get::<_, String>(0)?)
            ) {
                Ok(quality) => Ok(quality),
                Err(_) => Ok("best".to_string()) // Default value
            }
        }).await?;

        // Update cache
        {
            let mut cache = self.user_cache.lock().await;
            cache.insert(
                user_id,
                UserInfo {
                    quality_preference: quality.clone(),
                    last_updated: tokio::time::Instant::now(),
                }
            );
        }
        
        Ok(quality)
    }
}