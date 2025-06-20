use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use async_trait::async_trait;

// Task Type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    // One-time task, automatically deleted after execution
    OneTime,
    // Cyclic tasks are executed repeatedly at fixed intervals
    Recurring { interval_seconds: u64 },
    // Scheduled tasks are executed at a specified time
    Scheduled { next_run: DateTime<Utc> },
    // Persistent tasks, long-running
    Persistent,
}

// Task Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// Task Info
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub task_type: TaskType,
    pub last_run: Option<Instant>,
    pub next_run: Option<Instant>,
    pub status: TaskStatus,
    pub created_at: Instant,
    pub executor_type: String,
    pub data: Option<serde_json::Value>,
    pub max_concurrent: Option<usize>, // Maximum number of concurrent tasks
    pub timeout_seconds: Option<u64>,  // Timeout
    pub retry_count: u32,              // Retry times
    pub max_retries: u32,              // Maximum number of retries
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.next_run == other.next_run && self.id == other.id
    }
}
impl Eq for Task {}
impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // next_run small first (minimum heap)
        match (self.next_run, other.next_run) {
            (Some(a), Some(b)) => b.partial_cmp(&a), // On the contrary, BinaryHeap is the largest heap by default.
            (Some(_), None) => Some(Ordering::Less),
            (None, Some(_)) => Some(Ordering::Greater),
            (None, None) => self.id.partial_cmp(&other.id),
        }
    }
}
impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

// Task information for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub task_type: TaskType,
    pub last_run: Option<SystemTime>,
    pub next_run: Option<SystemTime>,
    pub status: TaskStatus,
    pub created_at: SystemTime,
    pub executor_type: String,
    pub data: Option<serde_json::Value>,
    pub max_concurrent: Option<usize>,
    pub timeout_seconds: Option<u64>,
    pub retry_count: u32,
    pub max_retries: u32,
}

impl Task {
    // Convert to serializable TaskInfo
    pub fn to_info(&self) -> TaskInfo {
        let now = SystemTime::now();
        TaskInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            task_type: self.task_type.clone(),
            last_run: self.last_run.map(|_| now),
            next_run: self.next_run.map(|_| now),
            status: self.status.clone(),
            created_at: now,
            executor_type: self.executor_type.clone(),
            data: self.data.clone(),
            max_concurrent: self.max_concurrent,
            timeout_seconds: self.timeout_seconds,
            retry_count: self.retry_count,
            max_retries: self.max_retries,
        }
    }

    // Check if the task should be executed
    pub fn should_run(&self, now: Instant) -> bool {
        if !matches!(self.status, TaskStatus::Pending) {
            return false;
        }

        match &self.task_type {
            TaskType::OneTime => true,
            TaskType::Recurring { .. } => {
                self.next_run.map_or(false, |next| next <= now)
            }
            TaskType::Scheduled { next_run } => {
                let next_instant = DateTime::<Utc>::from_naive_utc_and_offset(next_run.naive_utc(), Utc);
                // Simplified time comparison, should be more accurate in practice
                now >= Instant::now()
            }
            TaskType::Persistent => true,
        }
    }

    // Calculate the next execution time
    pub fn calculate_next_run(&mut self, now: Instant) {
        match &self.task_type {
            TaskType::OneTime => {
                // One-time task execution does not set the next execution time
                self.next_run = None;
            }
            TaskType::Recurring { interval_seconds } => {
                self.next_run = Some(now + Duration::from_secs(*interval_seconds));
            }
            TaskType::Scheduled { next_run } => {
                // Scheduled tasks need to recalculate the next time after execution
                // Here you can calculate according to cron expressions or other rules
                self.next_run = Some(now + Duration::from_secs(3600)); // Default 1 hour later
            }
            TaskType::Persistent => {
                // Persistent tasks do not set the next execution time
                self.next_run = None;
            }
        }
    }
}

#[async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(&self, task: &Task) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn get_name(&self) -> &str;
    fn supports_concurrent(&self) -> bool {
        true
    }
}

// Task scheduler
pub struct TaskScheduler {
    heap: Arc<RwLock<BinaryHeap<Task>>>,
    index: Arc<RwLock<HashMap<String, Task>>>,
    executors: Arc<RwLock<HashMap<String, Box<dyn TaskExecutor>>>>,
    running: Arc<RwLock<bool>>,
    max_concurrent_tasks: usize,
    task_semaphore: Arc<tokio::sync::Semaphore>,
}

impl TaskScheduler {
    /// Create a new task scheduler.
    /// max_concurrent_tasks: Limit the maximum number of concurrent tasks to prevent resource exhaustion.
    pub fn new(max_concurrent_tasks: usize) -> Self {
        Self {
            heap: Arc::new(RwLock::new(BinaryHeap::new())),
            index: Arc::new(RwLock::new(HashMap::new())),
            executors: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            max_concurrent_tasks,
            task_semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent_tasks)),
        }
    }

    // Adding a Task Executor
    pub async fn add_executor(&self, executor: Box<dyn TaskExecutor>) {
        let name = executor.get_name().to_string();
        let mut executors = self.executors.write().await;
        executors.insert(name, executor);
    }

    // Add a one-time task
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
            None,
            timeout_seconds,
            0,
        ).await
    }

    // Add a recurring task
    pub async fn add_recurring_task(
        &self,
        name: String,
        interval_seconds: u64,
        executor_type: String,
        data: Option<serde_json::Value>,
        max_concurrent: Option<usize>,
        timeout_seconds: Option<u64>,
        max_retries: u32,
    ) -> String {
        self.add_task_internal(
            name,
            TaskType::Recurring { interval_seconds },
            executor_type,
            data,
            max_concurrent,
            timeout_seconds,
            max_retries,
        ).await
    }

    // Add a scheduled task
    pub async fn add_scheduled_task(
        &self,
        name: String,
        next_run: DateTime<Utc>,
        executor_type: String,
        data: Option<serde_json::Value>,
        timeout_seconds: Option<u64>,
    ) -> String {
        self.add_task_internal(
            name,
            TaskType::Scheduled { next_run },
            executor_type,
            data,
            None,
            timeout_seconds,
            0,
        ).await
    }

    // Adding a persistence task
    pub async fn add_persistent_task(
        &self,
        name: String,
        executor_type: String,
        data: Option<serde_json::Value>,
        max_concurrent: Option<usize>,
    ) -> String {
        self.add_task_internal(
            name,
            TaskType::Persistent,
            executor_type,
            data,
            max_concurrent,
            None,
            0,
        ).await
    }

    // Internally add task method
    async fn add_task_internal(
        &self,
        name: String,
        task_type: TaskType,
        executor_type: String,
        data: Option<serde_json::Value>,
        max_concurrent: Option<usize>,
        timeout_seconds: Option<u64>,
        max_retries: u32,
    ) -> String {
        let task_id = Uuid::new_v4().to_string();
        let now = Instant::now();
        
        let mut next_run = None;
        match &task_type {
            TaskType::Recurring { interval_seconds } => {
                next_run = Some(now + Duration::from_secs(*interval_seconds));
            }
            TaskType::Scheduled { next_run: scheduled_time } => {
                // Here you need to convert DateTime to Instant
                next_run = Some(now + Duration::from_secs(1));
            }
            _ => {}
        }

        let print_task_type = task_type.clone();
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
            max_concurrent,
            timeout_seconds,
            retry_count: 0,
            max_retries,
        };

        let mut index = self.index.write().await;
        index.insert(task_id.clone(), task.clone());
        let mut heap = self.heap.write().await;
        heap.push(task.clone());

        tracing::info!("Add task: {} - {:?} Execute Time: {:?}", task_id, print_task_type, next_run);

        task_id
    }

    // Deleting a task
    pub async fn remove_task(&self, task_id: &str) -> bool {
        let mut index = self.index.write().await;
        let removed = index.remove(task_id).is_some();
        
        if removed {
            tracing::info!("Delete task: {}", task_id);
        } else {
            tracing::warn!("Task does not exist: {}", task_id);
        }
        
        removed
    }

    // Get all tasks
    pub async fn get_all_tasks(&self) -> Vec<Task> {
        let index = self.index.read().await;
        index.values().cloned().collect()
    }

    // Get a single task
    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        let index = self.index.read().await;
        index.get(task_id).cloned()
    }

    /// Start the scheduler main loop.
    /// Ticks every second, checks if the top task in the heap is due, pops and executes it concurrently if so.
    /// Uses a semaphore to limit the maximum concurrency.
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            tracing::warn!("Scheduler is already running");
            return;
        }
        *running = true;
        drop(running);

        tracing::info!("Start the task scheduler");
        
        let heap = Arc::clone(&self.heap);
        let index = Arc::clone(&self.index);
        let executors = Arc::clone(&self.executors);
        let running = Arc::clone(&self.running);
        let semaphore = Arc::clone(&self.task_semaphore);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                tracing::debug!("Loop Tick");
                interval.tick().await;
                // 1. Check if the scheduler should stop
                if !*running.read().await {
                    break;
                }

                let now = Instant::now();
                let mut tasks_to_run = Vec::new();
                // 2. Only process the top due tasks in the heap, pop them for mutable access
                {
                    let mut heap_guard = heap.write().await;
                    let mut index_guard = index.write().await;
                    while let Some(mut task) = heap_guard.pop() {
                        if task.should_run(now) {
                            // Mark as running, record last run time, calculate next run time
                            task.status = TaskStatus::Running;
                            task.last_run = Some(now);
                            task.calculate_next_run(now);
                            tasks_to_run.push(task.clone());
                            // After the OneTime task is executed, it is directly removed from the index and no longer pushed back to the heap
                            if matches!(task.task_type, TaskType::OneTime) {
                                index_guard.remove(&task.id);
                            } else {
                                // Non-one-time tasks, push back to the heap
                                heap_guard.push(task);
                            }
                        } else {
                            // Not due yet, put back; subsequent tasks are also not due, break
                            heap_guard.push(task);
                            break;
                        }
                    }
                }

                // 3. Execute due tasks concurrently, protected by semaphore
                let mut handles = Vec::new();
                for task in tasks_to_run {
                    let semaphore = Arc::clone(&semaphore);
                    let heap = Arc::clone(&heap);
                    let index = Arc::clone(&index);
                    let executors = Arc::clone(&executors);
                    // Spawn an async task for each, automatically acquires semaphore
                    let handle = tokio::spawn(async move {
                        let _permit = semaphore.acquire().await.unwrap();
                        let executors_guard = executors.read().await;
                        if let Some(executor) = executors_guard.get(&task.executor_type) {
                            // Support timeout control
                            let result = if let Some(timeout) = task.timeout_seconds {
                                tokio::time::timeout(
                                    Duration::from_secs(timeout),
                                    async { executor.execute(&task).await }
                                ).await
                            } else {
                                Ok(executor.execute(&task).await)
                            };
                            // 5. Update task status based on result, support retry
                            match result {
                                Ok(Ok(_)) => {
                                    let mut heap_guard = heap.write().await;
                                    let mut index_guard = index.write().await;
                                    if let Some(task_ref) = index_guard.get_mut(&task.id) {
                                        task_ref.status = TaskStatus::Completed;
                                        task_ref.retry_count = 0;
                                    }
                                    tracing::info!("Task execution successful: {} | Task Type: {:?} | Execute Type: {:?}", task.id, task.task_type, task.executor_type);
                                }
                                Ok(Err(e)) => {
                                    let mut heap_guard = heap.write().await;
                                    let mut index_guard = index.write().await;
                                    if let Some(task_ref) = index_guard.get_mut(&task.id) {
                                        if task_ref.retry_count < task_ref.max_retries {
                                            task_ref.retry_count += 1;
                                            task_ref.status = TaskStatus::Pending;
                                            tracing::warn!("Task execution failed, will be retried: {} - {}", task.id, e);
                                        } else {
                                            task_ref.status = TaskStatus::Failed;
                                            tracing::error!("Task execution failed, maximum number of retries reached: {} - {}", task.id, e);
                                        }
                                    }
                                }
                                Err(_) => {
                                    let mut heap_guard = heap.write().await;
                                    let mut index_guard = index.write().await;
                                    if let Some(task_ref) = index_guard.get_mut(&task.id) {
                                        task_ref.status = TaskStatus::Failed;
                                    }
                                    tracing::error!("Task execution timeout: {}", task.id);
                                }
                                Ok(_) => {
                                    // Compatible with other Ok branches
                                }
                            }
                        } else {
                            tracing::error!("Task executor not found: {}", task.executor_type);
                            let mut heap_guard = heap.write().await;
                            let mut index_guard = index.write().await;
                            if let Some(task_ref) = index_guard.get_mut(&task.id) {
                                task_ref.status = TaskStatus::Failed;
                            }
                        }
                    });
                    handles.push(handle);
                }
                // 4. Wait for all tasks to complete (optional)
                for handle in handles {
                    let _ = handle.await;
                }
            }
        });
    }

    // Stop the scheduler
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        tracing::info!("Stop the task scheduler");
    }

    // Check if the scheduler is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    // Immediately execute a task
    pub async fn execute_task_now(&self, task_id: &str) -> bool {
        let (task, executor_type) = {
            let index = self.index.read().await;
            if let Some(task) = index.get(task_id) {
                (Some(task.clone()), Some(task.executor_type.clone()))
            } else {
                (None, None)
            }
        };

        if let (Some(task), Some(executor_type)) = (task, executor_type) {
            let executors = self.executors.read().await;
            if let Some(executor) = executors.get(&executor_type) {
                let semaphore = Arc::clone(&self.task_semaphore);
                // executor 需要 clone 或 Arc 包裹，这里用 get_name 重新查找
                let executor_name = executor.get_name().to_string();
                drop(executors); // 释放锁
                let executors = Arc::clone(&self.executors);
                let task_id = task_id.to_string();
                let handle = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let executors_guard = executors.read().await;
                    if let Some(executor) = executors_guard.get(&executor_name) {
                        let result = if let Some(timeout) = task.timeout_seconds {
                            tokio::time::timeout(
                                Duration::from_secs(timeout),
                                async { executor.execute(&task).await }
                            ).await
                        } else {
                            Ok(executor.execute(&task).await)
                        };
                        match result {
                            Ok(Ok(_)) => {
                                tracing::info!("Immediately execute the task successfully: {}", task_id);
                                true
                            }
                            Ok(Err(e)) => {
                                tracing::error!("Immediately execute the task failed: {} - {}", task_id, e);
                                false
                            }
                            Err(_) => {
                                tracing::error!("Immediately execute the task timeout: {}", task_id);
                                false
                            }
                            Ok(_) => {
                                // Compatible with other Ok branches
                                false
                            }
                        }
                    } else {
                        tracing::error!("Task executor not found: {}", executor_name);
                        false
                    }
                });
                handle.await.unwrap_or(false)
            } else {
                tracing::error!("Task executor not found: {}", executor_type);
                false
            }
        } else {
            tracing::warn!("Task does not exist, cannot be executed immediately: {}", task_id);
            false
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(100) // Default maximum concurrency is 100 tasks
    }
} 