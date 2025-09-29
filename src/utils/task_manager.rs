use tokio::task::{JoinSet, AbortHandle};
use std::future::Future;

pub struct TaskManager {
    tasks: JoinSet<()>,
    max_concurrent: usize,
}

impl TaskManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            tasks: JoinSet::new(),
            max_concurrent,
        }
    }

    /// Spawns a new task and automatically cleans up completed tasks
    pub fn spawn<F>(&mut self, task: F) -> AbortHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Automatically clean up completed tasks before adding new one
        self.cleanup_completed();

        // If reached limit, wait for completion of at least one task
        if self.tasks.len() >= self.max_concurrent {
            log::warn!("TaskManager at capacity ({}), waiting for task completion", self.max_concurrent);
            // Force cleanup of one completed task
            while self.tasks.len() >= self.max_concurrent {
                if !self.try_join_one() {
                    // If no completed tasks, let the system continue anyway
                    log::warn!("All {} tasks still running, continuing anyway", self.max_concurrent);
                    break;
                }
            }
        }
        self.tasks.spawn(task)
    }

    /// Cleans up all completed tasks without blocking
    fn cleanup_completed(&mut self) {
        let mut cleaned = 0;
        while self.try_join_one() {
            cleaned += 1;
        }
        if cleaned > 0 {
            log::debug!("Cleaned up {} completed tasks", cleaned);
        }
    }

    /// Tries to get one completed task (non-blocking)
    fn try_join_one(&mut self) -> bool {
        // Use poll to check for completed tasks non-blocking
        use std::task::{Context, Poll};
        use std::future::Future;
        use std::pin::Pin;
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        match Pin::new(&mut self.tasks).poll_join_next(&mut cx) {
            Poll::Ready(Some(result)) => {
                if let Err(e) = result {
                    log::error!("Background task failed: {:?}", e);
                }
                true
            }
            Poll::Ready(None) | Poll::Pending => false,
        }
    }

    /// Waits for all tasks to complete
    pub async fn shutdown(&mut self) {
        log::info!("Shutting down TaskManager, waiting for {} tasks", self.tasks.len());
        while let Some(result) = self.tasks.join_next().await {
            if let Err(e) = result {
                log::error!("Task failed during shutdown: {:?}", e);
            }
        }
        log::info!("TaskManager shutdown complete");
    }

    /// Aborts all tasks without waiting
    pub fn abort_all(&mut self) {
        log::warn!("Aborting all {} tasks", self.tasks.len());
        self.tasks.abort_all();
    }

    /// Returns number of active tasks
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Checks if tasks set is empty
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

impl Drop for TaskManager {
    fn drop(&mut self) {
        if !self.tasks.is_empty() {
            log::warn!("TaskManager dropped with {} active tasks, aborting them", self.tasks.len());
            self.abort_all();
        }
    }
}