use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    OneTime,
    Recurring {
        interval_seconds: u64,
    },
    Scheduled {
        run_at: DateTime<Utc>,
    },
    /// Starts once and remains running until the executor returns or the task is removed.
    Persistent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub task_type: TaskType,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub executor_type: String,
    pub data: Option<serde_json::Value>,
    pub timeout_seconds: Option<u64>,
    pub retry_count: u32,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub task_type: TaskType,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub executor_type: String,
    pub data: Option<serde_json::Value>,
    pub timeout_seconds: Option<u64>,
    pub retry_count: u32,
    pub max_retries: u32,
}

impl Task {
    pub fn to_info(&self) -> TaskInfo {
        TaskInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            task_type: self.task_type.clone(),
            last_run: self.last_run,
            next_run: self.next_run,
            status: self.status.clone(),
            created_at: self.created_at,
            executor_type: self.executor_type.clone(),
            data: self.data.clone(),
            timeout_seconds: self.timeout_seconds,
            retry_count: self.retry_count,
            max_retries: self.max_retries,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct QueueEntry {
    task_id: String,
    run_at: DateTime<Utc>,
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .run_at
            .cmp(&self.run_at)
            .then_with(|| other.task_id.cmp(&self.task_id))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(&self, task: &Task) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn get_name(&self) -> &str;
}

pub struct TaskScheduler {
    queue: Arc<Mutex<BinaryHeap<QueueEntry>>>,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    executors: Arc<RwLock<HashMap<String, Arc<dyn TaskExecutor>>>>,
    persistent_handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    running: Arc<RwLock<bool>>,
    max_concurrent_tasks: usize,
    task_semaphore: Arc<Semaphore>,
}

impl TaskScheduler {
    pub fn new(max_concurrent_tasks: usize) -> Self {
        assert!(
            max_concurrent_tasks > 0,
            "max_concurrent_tasks must be greater than zero"
        );
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            executors: Arc::new(RwLock::new(HashMap::new())),
            persistent_handles: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            max_concurrent_tasks,
            task_semaphore: Arc::new(Semaphore::new(max_concurrent_tasks)),
        }
    }

    pub async fn add_executor(&self, executor: Box<dyn TaskExecutor>) {
        let name = executor.get_name().to_string();
        self.executors
            .write()
            .await
            .insert(name, Arc::from(executor));
    }

    pub async fn add_one_time_task(
        &self,
        name: String,
        executor_type: String,
        data: Option<serde_json::Value>,
        timeout_seconds: Option<u64>,
    ) -> String {
        self.add_task_internal(
            name,
            TaskType::OneTime,
            executor_type,
            data,
            timeout_seconds,
            0,
        )
        .await
    }

    pub async fn add_recurring_task(
        &self,
        name: String,
        interval_seconds: u64,
        executor_type: String,
        data: Option<serde_json::Value>,
        timeout_seconds: Option<u64>,
        max_retries: u32,
    ) -> String {
        assert!(
            interval_seconds > 0,
            "interval_seconds must be greater than zero"
        );
        assert!(
            i64::try_from(interval_seconds).is_ok(),
            "interval_seconds is too large"
        );
        self.add_task_internal(
            name,
            TaskType::Recurring { interval_seconds },
            executor_type,
            data,
            timeout_seconds,
            max_retries,
        )
        .await
    }

    pub async fn add_scheduled_task(
        &self,
        name: String,
        run_at: DateTime<Utc>,
        executor_type: String,
        data: Option<serde_json::Value>,
        timeout_seconds: Option<u64>,
    ) -> String {
        self.add_task_internal(
            name,
            TaskType::Scheduled { run_at },
            executor_type,
            data,
            timeout_seconds,
            0,
        )
        .await
    }

    pub async fn add_persistent_task(
        &self,
        name: String,
        executor_type: String,
        data: Option<serde_json::Value>,
    ) -> String {
        self.add_task_internal(name, TaskType::Persistent, executor_type, data, None, 0)
            .await
    }

    async fn add_task_internal(
        &self,
        name: String,
        task_type: TaskType,
        executor_type: String,
        data: Option<serde_json::Value>,
        timeout_seconds: Option<u64>,
        max_retries: u32,
    ) -> String {
        let task_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let next_run = match &task_type {
            TaskType::Recurring { interval_seconds } => {
                Some(now + ChronoDuration::seconds(*interval_seconds as i64))
            }
            TaskType::Scheduled { run_at } => Some(*run_at),
            TaskType::OneTime | TaskType::Persistent => Some(now),
        };
        let task = Task {
            id: task_id.clone(),
            name,
            task_type,
            last_run: None,
            next_run,
            status: TaskStatus::Pending,
            created_at: now,
            executor_type,
            data,
            timeout_seconds,
            retry_count: 0,
            max_retries,
        };

        self.tasks.write().await.insert(task_id.clone(), task);
        self.enqueue(
            task_id.clone(),
            next_run.expect("new tasks always have a first run"),
        )
        .await;
        tracing::info!(task_id, ?next_run, "task added");
        task_id
    }

    async fn enqueue(&self, task_id: String, run_at: DateTime<Utc>) {
        self.queue.lock().await.push(QueueEntry { task_id, run_at });
    }

    pub async fn remove_task(&self, task_id: &str) -> bool {
        if let Some(handle) = self.persistent_handles.lock().await.remove(task_id) {
            handle.abort();
        }
        let removed = self.tasks.write().await.remove(task_id).is_some();
        if removed {
            tracing::info!(task_id, "task removed");
        }
        removed
    }

    pub async fn get_all_tasks(&self) -> Vec<Task> {
        self.tasks.read().await.values().cloned().collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.read().await.get(task_id).cloned()
    }

    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            tracing::warn!("scheduler is already running");
            return;
        }
        *running = true;
        drop(running);

        let queue = Arc::clone(&self.queue);
        let tasks = Arc::clone(&self.tasks);
        let executors = Arc::clone(&self.executors);
        let persistent_handles = Arc::clone(&self.persistent_handles);
        let running = Arc::clone(&self.running);
        let semaphore = Arc::clone(&self.task_semaphore);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));
            loop {
                interval.tick().await;
                if !*running.read().await {
                    break;
                }

                let due = Self::take_due_entries(&queue).await;
                for entry in due {
                    let task = {
                        let mut guard = tasks.write().await;
                        let Some(task) = guard.get_mut(&entry.task_id) else {
                            continue;
                        };
                        if task.status != TaskStatus::Pending || task.next_run != Some(entry.run_at)
                        {
                            continue;
                        }
                        task.status = TaskStatus::Running;
                        task.last_run = Some(Utc::now());
                        task.next_run = None;
                        task.clone()
                    };

                    let is_persistent = matches!(task.task_type, TaskType::Persistent);
                    let handle = Self::spawn_execution(
                        task,
                        Arc::clone(&tasks),
                        Arc::clone(&queue),
                        Arc::clone(&executors),
                        Arc::clone(&semaphore),
                        !is_persistent,
                    );
                    if is_persistent {
                        persistent_handles
                            .lock()
                            .await
                            .insert(entry.task_id, handle);
                    }
                }
            }
        });
        tracing::info!("task scheduler started");
    }

    async fn take_due_entries(queue: &Arc<Mutex<BinaryHeap<QueueEntry>>>) -> Vec<QueueEntry> {
        let now = Utc::now();
        let mut queue = queue.lock().await;
        let mut due = Vec::new();
        while queue.peek().is_some_and(|entry| entry.run_at <= now) {
            if let Some(entry) = queue.pop() {
                due.push(entry);
            }
        }
        due
    }

    fn spawn_execution(
        task: Task,
        tasks: Arc<RwLock<HashMap<String, Task>>>,
        queue: Arc<Mutex<BinaryHeap<QueueEntry>>>,
        executors: Arc<RwLock<HashMap<String, Arc<dyn TaskExecutor>>>>,
        semaphore: Arc<Semaphore>,
        use_semaphore: bool,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Persistent connections are long-lived and must not consume all transient task slots.
            let _permit = if use_semaphore {
                let Ok(permit) = semaphore.acquire().await else {
                    return;
                };
                Some(permit)
            } else {
                None
            };
            let executor = executors.read().await.get(&task.executor_type).cloned();
            let result = match executor {
                Some(executor) => {
                    if let Some(timeout) = task.timeout_seconds {
                        match tokio::time::timeout(
                            Duration::from_secs(timeout),
                            executor.execute(&task),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(error) => {
                                Err(Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
                            }
                        }
                    } else {
                        executor.execute(&task).await
                    }
                }
                None => Err(format!("task executor not found: {}", task.executor_type).into()),
            };

            let mut next_entry = None;
            let mut guard = tasks.write().await;
            let Some(current) = guard.get_mut(&task.id) else {
                return;
            };
            match result {
                Ok(()) => {
                    current.retry_count = 0;
                    match current.task_type {
                        TaskType::Recurring { interval_seconds } => {
                            let next_run =
                                Utc::now() + ChronoDuration::seconds(interval_seconds as i64);
                            current.status = TaskStatus::Pending;
                            current.next_run = Some(next_run);
                            next_entry = Some(QueueEntry {
                                task_id: current.id.clone(),
                                run_at: next_run,
                            });
                        }
                        TaskType::OneTime | TaskType::Scheduled { .. } | TaskType::Persistent => {
                            current.status = TaskStatus::Completed;
                            current.next_run = None;
                        }
                    }
                    tracing::info!(task_id = %task.id, "task execution completed");
                }
                Err(error) => {
                    if current.retry_count < current.max_retries {
                        current.retry_count += 1;
                        current.status = TaskStatus::Pending;
                        let next_run = Utc::now() + ChronoDuration::seconds(1);
                        current.next_run = Some(next_run);
                        next_entry = Some(QueueEntry {
                            task_id: current.id.clone(),
                            run_at: next_run,
                        });
                        tracing::warn!(task_id = %task.id, %error, "task failed and will retry");
                    } else {
                        current.status = TaskStatus::Failed;
                        current.next_run = None;
                        tracing::error!(task_id = %task.id, %error, "task execution failed");
                    }
                }
            }
            drop(guard);
            if let Some(entry) = next_entry {
                queue.lock().await.push(entry);
            }
        })
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        let mut handles = self.persistent_handles.lock().await;
        for (_, handle) in handles.drain() {
            handle.abort();
        }
        let mut tasks = self.tasks.write().await;
        for task in tasks.values_mut() {
            if matches!(task.task_type, TaskType::Persistent) && task.status == TaskStatus::Running
            {
                task.status = TaskStatus::Cancelled;
            }
        }
        tracing::info!("task scheduler stopped");
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub fn max_concurrent_tasks(&self) -> usize {
        self.max_concurrent_tasks
    }

    pub async fn execute_task_now(&self, task_id: &str) -> bool {
        let task = {
            let mut tasks = self.tasks.write().await;
            let Some(task) = tasks.get_mut(task_id) else {
                return false;
            };
            if task.status == TaskStatus::Running {
                return false;
            }
            task.status = TaskStatus::Running;
            task.last_run = Some(Utc::now());
            task.next_run = None;
            task.clone()
        };
        let is_persistent = matches!(task.task_type, TaskType::Persistent);
        let handle = Self::spawn_execution(
            task,
            Arc::clone(&self.tasks),
            Arc::clone(&self.queue),
            Arc::clone(&self.executors),
            Arc::clone(&self.task_semaphore),
            !is_persistent,
        );
        if is_persistent {
            self.persistent_handles
                .lock()
                .await
                .insert(task_id.to_string(), handle);
            true
        } else {
            handle.await.is_ok()
                && self
                    .tasks
                    .read()
                    .await
                    .get(task_id)
                    .is_some_and(|task| task.status != TaskStatus::Failed)
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingExecutor(Arc<AtomicUsize>);

    #[async_trait]
    impl TaskExecutor for CountingExecutor {
        async fn execute(
            &self,
            _task: &Task,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn get_name(&self) -> &str {
            "counter"
        }
    }

    async fn wait_for_count(counter: &AtomicUsize, expected: usize) {
        tokio::time::timeout(Duration::from_secs(4), async {
            while counter.load(Ordering::SeqCst) < expected {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("task was not executed in time");
    }

    #[tokio::test]
    async fn scheduled_task_uses_requested_time() {
        let scheduler = TaskScheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        scheduler
            .add_executor(Box::new(CountingExecutor(Arc::clone(&counter))))
            .await;
        scheduler.start().await;
        let run_at = Utc::now() + ChronoDuration::milliseconds(500);
        let id = scheduler
            .add_scheduled_task("once".into(), run_at, "counter".into(), None, None)
            .await;

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        wait_for_count(&counter, 1).await;
        assert_eq!(
            scheduler.get_task(&id).await.unwrap().status,
            TaskStatus::Completed
        );
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn removed_task_is_not_executed() {
        let scheduler = TaskScheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        scheduler
            .add_executor(Box::new(CountingExecutor(Arc::clone(&counter))))
            .await;
        scheduler.start().await;
        let id = scheduler
            .add_scheduled_task(
                "removed".into(),
                Utc::now() + ChronoDuration::milliseconds(300),
                "counter".into(),
                None,
                None,
            )
            .await;
        assert!(scheduler.remove_task(&id).await);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn recurring_task_runs_more_than_once() {
        let scheduler = TaskScheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        scheduler
            .add_executor(Box::new(CountingExecutor(Arc::clone(&counter))))
            .await;
        scheduler.start().await;
        scheduler
            .add_recurring_task("repeat".into(), 1, "counter".into(), None, None, 0)
            .await;
        wait_for_count(&counter, 2).await;
        scheduler.stop().await;
    }

    struct PersistentExecutor(Arc<AtomicUsize>);

    #[async_trait]
    impl TaskExecutor for PersistentExecutor {
        async fn execute(
            &self,
            _task: &Task,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await;
            Ok(())
        }

        fn get_name(&self) -> &str {
            "persistent"
        }
    }

    #[tokio::test]
    async fn persistent_task_stays_running_until_removed() {
        let scheduler = TaskScheduler::new(1);
        let counter = Arc::new(AtomicUsize::new(0));
        scheduler
            .add_executor(Box::new(PersistentExecutor(Arc::clone(&counter))))
            .await;
        scheduler.start().await;
        let id = scheduler
            .add_persistent_task("connection".into(), "persistent".into(), None)
            .await;

        wait_for_count(&counter, 1).await;
        assert_eq!(
            scheduler.get_task(&id).await.unwrap().status,
            TaskStatus::Running
        );
        assert!(scheduler.remove_task(&id).await);
        assert!(scheduler.get_task(&id).await.is_none());
        scheduler.stop().await;
    }
}
