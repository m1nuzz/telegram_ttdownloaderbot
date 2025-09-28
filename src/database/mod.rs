mod pool;
mod old;

pub use pool::DatabasePool;
pub use old::{get_database_path, init_database, update_user_activity, log_download};