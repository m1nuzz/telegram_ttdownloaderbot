use tokio::task::JoinSet;

pub struct TaskManager {
    progress_tasks: JoinSet<()>,
}

impl TaskManager {
    pub fn new(_max_concurrent_uploads: usize) -> Self {
        Self {
            progress_tasks: JoinSet::new(),
        }
    }

    /// Wait for all tasks to complete
    pub async fn shutdown(&mut self) {
        while let Some(_) = self.progress_tasks.join_next().await {
            // Wait for all tasks
        }
    }
}