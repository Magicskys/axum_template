use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::FutureExt;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

crate::define_permissions!(scheduler => [read, write]);

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
    /// Maintains a long-lived executor and reconnects after an unexpected return or failure.
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
    async fn execute(
        &self,
        task: &Task,
        cancellation: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn get_name(&self) -> &str;
}

pub struct TaskScheduler {
    db: Option<DatabaseConnection>,
    queue: Arc<Mutex<BinaryHeap<QueueEntry>>>,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    executors: Arc<RwLock<HashMap<String, Arc<dyn TaskExecutor>>>>,
    execution_handles: Arc<Mutex<HashMap<String, RunningExecution>>>,
    scheduler_handle: Mutex<Option<JoinHandle<()>>>,
    running: Arc<RwLock<bool>>,
    max_concurrent_tasks: usize,
    task_semaphore: Arc<Semaphore>,
}

struct RunningExecution {
    cancellation: CancellationToken,
    handle: JoinHandle<()>,
}

impl TaskScheduler {
    pub fn new(max_concurrent_tasks: usize) -> Self {
        assert!(
            max_concurrent_tasks > 0,
            "max_concurrent_tasks must be greater than zero"
        );
        Self {
            db: None,
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            executors: Arc::new(RwLock::new(HashMap::new())),
            execution_handles: Arc::new(Mutex::new(HashMap::new())),
            scheduler_handle: Mutex::new(None),
            running: Arc::new(RwLock::new(false)),
            max_concurrent_tasks,
            task_semaphore: Arc::new(Semaphore::new(max_concurrent_tasks)),
        }
    }

    pub fn with_database(db: DatabaseConnection, max_concurrent_tasks: usize) -> Self {
        let mut scheduler = Self::new(max_concurrent_tasks);
        scheduler.db = Some(db);
        scheduler
    }

    pub async fn add_executor(&self, executor: Box<dyn TaskExecutor>) {
        let name = executor.get_name().to_string();
        self.executors
            .write()
            .await
            .insert(name, Arc::from(executor));
    }

    pub async fn has_executor(&self, name: &str) -> bool {
        self.executors.read().await.contains_key(name)
    }

    pub async fn restore_tasks(&self) -> Result<usize, String> {
        let Some(db) = &self.db else {
            return Ok(0);
        };
        let restored = crate::scheduler::store::load_recoverable_tasks(db)
            .await
            .map_err(|error| error.to_string())?;
        let mut count = 0;
        for mut task in restored {
            if !self.has_executor(&task.executor_type).await {
                tracing::warn!(task_id = %task.id, executor = %task.executor_type, "task restore skipped: executor not registered");
                continue;
            }
            let run_at = match task.task_type {
                TaskType::Persistent => Utc::now(),
                _ => task.next_run.unwrap_or_else(Utc::now),
            };
            task.next_run = Some(run_at);
            crate::scheduler::store::update_task(db, &task)
                .await
                .map_err(|error| error.to_string())?;
            self.tasks
                .write()
                .await
                .insert(task.id.clone(), task.clone());
            self.enqueue(task.id, run_at).await;
            count += 1;
        }
        Ok(count)
    }

    pub async fn add_one_time_task(
        &self,
        name: String,
        executor_type: String,
        data: Option<serde_json::Value>,
        timeout_seconds: Option<u64>,
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
        if interval_seconds == 0 {
            return Err("interval_seconds must be greater than zero".to_string());
        }
        if i64::try_from(interval_seconds).is_err() {
            return Err("interval_seconds is too large".to_string());
        }
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
        self.add_task_internal(
            name,
            TaskType::Persistent,
            executor_type,
            data,
            None,
            u32::MAX,
        )
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
    ) -> Result<String, String> {
        if !self.has_executor(&executor_type).await {
            return Err(format!("task executor not found: {executor_type}"));
        }
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

        if let Some(db) = &self.db {
            crate::scheduler::store::insert_task(db, &task)
                .await
                .map_err(|error| error.to_string())?;
        }
        self.tasks.write().await.insert(task_id.clone(), task);
        self.enqueue(
            task_id.clone(),
            next_run.expect("new tasks always have a first run"),
        )
        .await;
        tracing::info!(task_id, ?next_run, "task added");
        Ok(task_id)
    }

    async fn enqueue(&self, task_id: String, run_at: DateTime<Utc>) {
        self.queue.lock().await.push(QueueEntry { task_id, run_at });
    }

    pub async fn remove_task(&self, task_id: &str) -> bool {
        if let Some(mut execution) = self.execution_handles.lock().await.remove(task_id) {
            execution.cancellation.cancel();
            if tokio::time::timeout(Duration::from_secs(5), &mut execution.handle)
                .await
                .is_err()
            {
                execution.handle.abort();
            }
        }
        let removed = self.tasks.write().await.remove(task_id);
        if let Some(mut task) = removed {
            task.status = TaskStatus::Cancelled;
            task.next_run = None;
            if let Some(db) = &self.db {
                let _ = crate::scheduler::store::update_task(db, &task).await;
                let _ = crate::scheduler::store::cancel_running_executions(
                    db,
                    std::slice::from_ref(&task.id),
                )
                .await;
            }
            tracing::info!(task_id, "task removed");
            return true;
        }
        false
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
        let execution_handles = Arc::clone(&self.execution_handles);
        let running = Arc::clone(&self.running);
        let semaphore = Arc::clone(&self.task_semaphore);
        let db = self.db.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));
            loop {
                interval.tick().await;
                if !*running.read().await {
                    break;
                }

                execution_handles
                    .lock()
                    .await
                    .retain(|_, execution| !execution.handle.is_finished());

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
                    let (cancellation, handle) = Self::spawn_execution(
                        task,
                        db.clone(),
                        Arc::clone(&tasks),
                        Arc::clone(&queue),
                        Arc::clone(&executors),
                        Arc::clone(&semaphore),
                        !is_persistent,
                    );
                    execution_handles.lock().await.insert(
                        entry.task_id,
                        RunningExecution {
                            cancellation,
                            handle,
                        },
                    );
                }
            }
        });
        *self.scheduler_handle.lock().await = Some(handle);
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
        db: Option<DatabaseConnection>,
        tasks: Arc<RwLock<HashMap<String, Task>>>,
        queue: Arc<Mutex<BinaryHeap<QueueEntry>>>,
        executors: Arc<RwLock<HashMap<String, Arc<dyn TaskExecutor>>>>,
        semaphore: Arc<Semaphore>,
        use_semaphore: bool,
    ) -> (CancellationToken, JoinHandle<()>) {
        let cancellation = CancellationToken::new();
        let execution_cancellation = cancellation.clone();
        let handle = tokio::spawn(async move {
            // Persistent connections are long-lived and must not consume all transient task slots.
            let _permit = if use_semaphore {
                let Ok(permit) = semaphore.acquire().await else {
                    return;
                };
                Some(permit)
            } else {
                None
            };
            let execution_id = if let Some(db) = &db {
                match crate::scheduler::store::start_execution(db, &task).await {
                    Ok(id) => {
                        let _ = crate::scheduler::store::update_task(db, &task).await;
                        Some(id)
                    }
                    Err(error) => {
                        tracing::error!(task_id = %task.id, %error, "failed to create execution record");
                        let failed = if let Some(current) = tasks.write().await.get_mut(&task.id) {
                            current.status = TaskStatus::Failed;
                            current.next_run = None;
                            Some(current.clone())
                        } else {
                            None
                        };
                        if let Some(failed) = failed {
                            let _ = crate::scheduler::store::update_task(db, &failed).await;
                        }
                        return;
                    }
                }
            } else {
                None
            };
            let executor = executors.read().await.get(&task.executor_type).cloned();
            let result: Result<(), String> = match executor {
                Some(executor) => {
                    let future =
                        AssertUnwindSafe(executor.execute(&task, execution_cancellation.clone()))
                            .catch_unwind();
                    let outcome = if let Some(timeout) = task.timeout_seconds {
                        tokio::time::timeout(Duration::from_secs(timeout), future)
                            .await
                            .map_err(|error| error.to_string())
                    } else {
                        Ok(future.await)
                    };
                    match outcome {
                        Ok(Ok(Ok(()))) => Ok(()),
                        Ok(Ok(Err(error))) => Err(error.to_string()),
                        Ok(Err(_)) => Err("task executor panicked".to_string()),
                        Err(error) => Err(error),
                    }
                }
                None => Err(format!("task executor not found: {}", task.executor_type)),
            };

            let mut next_entry = None;
            let mut guard = tasks.write().await;
            let Some(current) = guard.get_mut(&task.id) else {
                return;
            };
            let execution_error = result.as_ref().err().cloned();
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
                        TaskType::Persistent if !execution_cancellation.is_cancelled() => {
                            current.retry_count = current.retry_count.saturating_add(1);
                            let next_run = Utc::now() + ChronoDuration::seconds(1);
                            current.status = TaskStatus::Pending;
                            current.next_run = Some(next_run);
                            next_entry = Some(QueueEntry {
                                task_id: current.id.clone(),
                                run_at: next_run,
                            });
                        }
                        TaskType::Persistent => {
                            current.status = TaskStatus::Cancelled;
                            current.next_run = None;
                        }
                        TaskType::OneTime | TaskType::Scheduled { .. } => {
                            current.status = TaskStatus::Completed;
                            current.next_run = None;
                        }
                    }
                    tracing::info!(task_id = %task.id, "task execution completed");
                }
                Err(error) => {
                    if current.retry_count < current.max_retries
                        && !execution_cancellation.is_cancelled()
                    {
                        current.retry_count += 1;
                        current.status = TaskStatus::Pending;
                        let delay = 1_u64 << current.retry_count.saturating_sub(1).min(6);
                        let next_run = Utc::now() + ChronoDuration::seconds(delay as i64);
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
            let persisted = current.clone();
            drop(guard);
            if let Some(db) = &db {
                if let Some(execution_id) = execution_id {
                    let status = if execution_cancellation.is_cancelled() {
                        "cancelled"
                    } else if execution_error.is_some() {
                        "failed"
                    } else {
                        "completed"
                    };
                    let _ = crate::scheduler::store::finish_execution(
                        db,
                        &execution_id,
                        status,
                        execution_error,
                    )
                    .await;
                }
                let _ = crate::scheduler::store::update_task(db, &persisted).await;
            }
            if let Some(entry) = next_entry {
                queue.lock().await.push(entry);
            }
        });
        (cancellation, handle)
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        if let Some(handle) = self.scheduler_handle.lock().await.take() {
            let _ = handle.await;
        }

        let executions: Vec<_> = self.execution_handles.lock().await.drain().collect();
        let tasks = self.tasks.read().await;
        let (mut persistent, mut finite): (Vec<_>, Vec<_>) =
            executions.into_iter().partition(|(task_id, _)| {
                tasks
                    .get(task_id)
                    .is_some_and(|task| matches!(task.task_type, TaskType::Persistent))
            });
        drop(tasks);

        for (_, execution) in &persistent {
            execution.cancellation.cancel();
        }
        let graceful = tokio::time::timeout(Duration::from_secs(5), async {
            for (_, execution) in &mut persistent {
                let _ = (&mut execution.handle).await;
            }
        })
        .await;
        if graceful.is_err() {
            for (_, execution) in &persistent {
                execution.handle.abort();
            }
            for (_, execution) in &mut persistent {
                let _ = (&mut execution.handle).await;
            }
        }

        // Finite executions are already committed work. Let them finish instead of cancelling
        // them; their configured task timeout still applies inside spawn_execution.
        for (_, execution) in &mut finite {
            let _ = (&mut execution.handle).await;
        }

        let interrupted_task_ids: Vec<_> = persistent
            .iter()
            .map(|(task_id, _)| task_id.clone())
            .collect();
        let mut tasks = self.tasks.write().await;
        let mut changed = Vec::new();
        for task_id in &interrupted_task_ids {
            if let Some(task) = tasks.get_mut(task_id) {
                task.status = TaskStatus::Pending;
                task.next_run = Some(Utc::now());
                changed.push(task.clone());
            }
        }
        drop(tasks);
        if let Some(db) = &self.db {
            for task in &changed {
                let _ = crate::scheduler::store::update_task(db, task).await;
            }
            let _ =
                crate::scheduler::store::cancel_running_executions(db, &interrupted_task_ids).await;
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
        let (cancellation, handle) = Self::spawn_execution(
            task,
            self.db.clone(),
            Arc::clone(&self.tasks),
            Arc::clone(&self.queue),
            Arc::clone(&self.executors),
            Arc::clone(&self.task_semaphore),
            !is_persistent,
        );
        self.execution_handles.lock().await.insert(
            task_id.to_string(),
            RunningExecution {
                cancellation,
                handle,
            },
        );
        if is_persistent {
            true
        } else {
            loop {
                let status = self
                    .tasks
                    .read()
                    .await
                    .get(task_id)
                    .map(|task| task.status.clone());
                match status {
                    Some(TaskStatus::Running) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Some(TaskStatus::Failed | TaskStatus::Cancelled) | None => return false,
                    Some(_) => return true,
                }
            }
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Notify;

    struct CountingExecutor(Arc<AtomicUsize>);

    #[async_trait]
    impl TaskExecutor for CountingExecutor {
        async fn execute(
            &self,
            _task: &Task,
            _cancellation: CancellationToken,
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
            .await
            .unwrap();

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
            .await
            .unwrap();
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
            .await
            .unwrap();
        wait_for_count(&counter, 2).await;
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn unknown_executor_is_rejected() {
        let scheduler = TaskScheduler::new(1);
        let result = scheduler
            .add_one_time_task("missing".into(), "missing".into(), None, None)
            .await;
        assert!(result.is_err());
    }

    struct PanicExecutor;

    #[async_trait]
    impl TaskExecutor for PanicExecutor {
        async fn execute(
            &self,
            _task: &Task,
            _cancellation: CancellationToken,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            panic!("executor panic");
        }

        fn get_name(&self) -> &str {
            "panic"
        }
    }

    #[tokio::test]
    async fn executor_panic_marks_task_and_execution_failed() {
        use sea_orm::{Database, EntityTrait};

        let db = Database::connect("sqlite::memory:").await.unwrap();
        crate::init_database(&db).await.unwrap();
        let scheduler = TaskScheduler::with_database(db.clone(), 1);
        scheduler.add_executor(Box::new(PanicExecutor)).await;
        scheduler.start().await;
        let id = scheduler
            .add_one_time_task("panic".into(), "panic".into(), None, None)
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if scheduler.get_task(&id).await.unwrap().status == TaskStatus::Failed {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let executions = crate::model::task_execution::Entity::find()
            .all(&db)
            .await
            .unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].status, "failed");
        assert_eq!(
            executions[0].error.as_deref(),
            Some("task executor panicked")
        );
        scheduler.stop().await;
    }

    struct PersistentExecutor(Arc<AtomicUsize>);

    #[async_trait]
    impl TaskExecutor for PersistentExecutor {
        async fn execute(
            &self,
            _task: &Task,
            cancellation: CancellationToken,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            cancellation.cancelled().await;
            Ok(())
        }

        fn get_name(&self) -> &str {
            "persistent"
        }
    }

    struct DisconnectingExecutor(Arc<AtomicUsize>);

    #[async_trait]
    impl TaskExecutor for DisconnectingExecutor {
        async fn execute(
            &self,
            _task: &Task,
            _cancellation: CancellationToken,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err("connection lost".into())
        }

        fn get_name(&self) -> &str {
            "disconnecting"
        }
    }

    #[tokio::test]
    async fn persistent_task_reconnects_after_failure() {
        let scheduler = TaskScheduler::new(1);
        let counter = Arc::new(AtomicUsize::new(0));
        scheduler
            .add_executor(Box::new(DisconnectingExecutor(Arc::clone(&counter))))
            .await;
        scheduler.start().await;
        let id = scheduler
            .add_persistent_task("connection".into(), "disconnecting".into(), None)
            .await
            .unwrap();

        wait_for_count(&counter, 2).await;
        assert!(scheduler.remove_task(&id).await);
        scheduler.stop().await;
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
            .await
            .unwrap();

        wait_for_count(&counter, 1).await;
        assert_eq!(
            scheduler.get_task(&id).await.unwrap().status,
            TaskStatus::Running
        );
        assert!(scheduler.remove_task(&id).await);
        assert!(scheduler.get_task(&id).await.is_none());
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn pending_task_is_restored_from_database() {
        use sea_orm::Database;

        let db = Database::connect("sqlite::memory:").await.unwrap();
        crate::init_database(&db).await.unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let scheduler = TaskScheduler::with_database(db.clone(), 1);
        scheduler
            .add_executor(Box::new(CountingExecutor(Arc::clone(&counter))))
            .await;
        let id = scheduler
            .add_scheduled_task(
                "restored".into(),
                Utc::now() + ChronoDuration::milliseconds(500),
                "counter".into(),
                None,
                None,
            )
            .await
            .unwrap();
        drop(scheduler);

        let restored = TaskScheduler::with_database(db, 1);
        restored
            .add_executor(Box::new(CountingExecutor(Arc::clone(&counter))))
            .await;
        assert_eq!(restored.restore_tasks().await.unwrap(), 1);
        assert!(restored.get_task(&id).await.is_some());
        restored.start().await;
        wait_for_count(&counter, 1).await;
        assert_eq!(
            restored.get_task(&id).await.unwrap().status,
            TaskStatus::Completed
        );
        restored.stop().await;
    }

    #[tokio::test]
    async fn stopping_persistent_task_finishes_execution_record() {
        use sea_orm::{ColumnTrait, Database, EntityTrait, QueryFilter};

        let db = Database::connect("sqlite::memory:").await.unwrap();
        crate::init_database(&db).await.unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let scheduler = TaskScheduler::with_database(db.clone(), 1);
        scheduler
            .add_executor(Box::new(PersistentExecutor(Arc::clone(&counter))))
            .await;
        scheduler.start().await;
        let id = scheduler
            .add_persistent_task("connection".into(), "persistent".into(), None)
            .await
            .unwrap();
        wait_for_count(&counter, 1).await;

        scheduler.stop().await;

        let execution = crate::model::task_execution::Entity::find()
            .filter(crate::model::task_execution::Column::TaskId.eq(id.clone()))
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(execution.status, "cancelled");
        assert!(execution.finished_at.is_some());
        let task = crate::model::scheduler_task::Entity::find_by_id(id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, "pending");
    }

    struct BlockingFiniteExecutor {
        started: Arc<Notify>,
        release: Arc<Notify>,
        cancellation_observed: Arc<AtomicBool>,
        count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TaskExecutor for BlockingFiniteExecutor {
        async fn execute(
            &self,
            _task: &Task,
            cancellation: CancellationToken,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.count.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            tokio::select! {
                () = self.release.notified() => Ok(()),
                () = cancellation.cancelled() => {
                    self.cancellation_observed.store(true, Ordering::SeqCst);
                    Err("finite task was cancelled".into())
                }
            }
        }

        fn get_name(&self) -> &str {
            "blocking_finite"
        }
    }

    #[tokio::test]
    async fn shutdown_waits_for_running_finite_task_and_leaves_queued_task_pending() {
        use sea_orm::{ColumnTrait, Database, EntityTrait, QueryFilter};

        let db = Database::connect("sqlite::memory:").await.unwrap();
        crate::init_database(&db).await.unwrap();
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let cancellation_observed = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicUsize::new(0));
        let scheduler = Arc::new(TaskScheduler::with_database(db.clone(), 1));
        scheduler
            .add_executor(Box::new(BlockingFiniteExecutor {
                started: Arc::clone(&started),
                release: Arc::clone(&release),
                cancellation_observed: Arc::clone(&cancellation_observed),
                count: Arc::clone(&count),
            }))
            .await;
        scheduler.start().await;
        let running_id = scheduler
            .add_one_time_task("running".into(), "blocking_finite".into(), None, None)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), started.notified())
            .await
            .unwrap();
        let queued_id = scheduler
            .add_scheduled_task(
                "queued".into(),
                Utc::now() + ChronoDuration::seconds(60),
                "blocking_finite".into(),
                None,
                None,
            )
            .await
            .unwrap();

        let stopping = tokio::spawn({
            let scheduler = Arc::clone(&scheduler);
            async move { scheduler.stop().await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!stopping.is_finished());
        assert!(!cancellation_observed.load(Ordering::SeqCst));
        assert_eq!(count.load(Ordering::SeqCst), 1);

        release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), stopping)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            scheduler.get_task(&running_id).await.unwrap().status,
            TaskStatus::Completed
        );
        assert_eq!(
            scheduler.get_task(&queued_id).await.unwrap().status,
            TaskStatus::Pending
        );
        let queued_executions = crate::model::task_execution::Entity::find()
            .filter(crate::model::task_execution::Column::TaskId.eq(queued_id))
            .all(&db)
            .await
            .unwrap();
        assert!(queued_executions.is_empty());
    }
}
