use tokio::time::{sleep, Duration};
use std::future::Future;
use regex::Regex;

pub async fn retry_with_backoff<F, Fut, T, E>(max_retries: u32, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut retries = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                retries += 1;
                if retries >= max_retries {
                    log::error!("Operation failed after {} retries: {:?}", max_retries, e);
                    return Err(e);
                }
                
                // Exponential backoff: 1s, 2s, 4s, 8s, but no more than 30s
                let delay_ms = (1000 * 2_u64.pow(retries - 1)).min(30000);
                log::warn!("Operation failed (attempt {}/{}): {:?}, retrying in {}ms", 
                          retries, max_retries, e, delay_ms);
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

/// Extract FLOOD_WAIT seconds from Telegram error
pub fn extract_flood_wait(error_str: &str) -> Option<u64> {
    let re = Regex::new(r"FLOOD_WAIT_(\d+)").unwrap();
    re.captures(error_str)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse().ok())
}