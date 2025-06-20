# Task Scheduler Usage Guide

This project includes a high-performance task scheduler, supporting large-scale concurrency, persistence, one-time, recurring, and scheduled tasks. It is suitable for enterprise and personal backend services requiring scheduled or asynchronous jobs.

## Features
- Supports one-time, recurring, scheduled, and persistent tasks
- Efficient scheduling based on min-heap + HashMap, always keeping the soonest task at the top
- Max concurrency protection (semaphore-based, prevents resource exhaustion)
- Task timeout and retry mechanism
- Customizable task executors, async supported
- Detailed logging and status tracking

## Task Types
- **OneTime**: One-time task, automatically deleted after execution
- **Recurring**: Recurring task, executed repeatedly at fixed intervals
- **Scheduled**: Scheduled task, executed at a specified time
- **Persistent**: Persistent task, suitable for long-running jobs

## Task Executor (TaskExecutor)
Each task type requires an executor. Implement the `TaskExecutor` trait to define your own logic.

```rust
use crate::scheduler::task_scheduler::{Task, TaskExecutor};

struct MyExecutor;

#[async_trait::async_trait]
impl TaskExecutor for MyExecutor {
    async fn execute(&self, task: &Task) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Executing task: {}", task.name);
        // Your business logic ...
        Ok(())
    }
    fn get_name(&self) -> &str {
        "my_executor"
    }
}
```


## Scheduler Initialization & Registering Executors

```rust
use crate::scheduler::task_scheduler::{TaskScheduler, MyExecutor};
use std::sync::Arc;

let scheduler = Arc::new(TaskScheduler::new(100)); // Max 100 concurrent tasks
scheduler.add_executor(Box::new(MyExecutor)).await;
```

## Adding Tasks

```rust
// One-time task
let task_id = scheduler.add_one_time_task(
    "One-time Task".to_string(),
    "my_executor".to_string(),
    None,
    Some(30), // Timeout 30s
).await;

// Recurring task
let task_id = scheduler.add_recurring_task(
    "Recurring Task".to_string(),
    60, // Every 60s
    "my_executor".to_string(),
    None,
    Some(2), // Max 2 concurrent
    Some(20), // Timeout 20s
    3, // Max 3 retries
).await;

// Scheduled task
use chrono::Utc;
let task_id = scheduler.add_scheduled_task(
    "Scheduled Task".to_string(),
    Utc::now() + chrono::Duration::seconds(120), // Execute after 2 min
    "my_executor".to_string(),
    None,
    Some(60),
).await;

// Persistent task
let task_id = scheduler.add_persistent_task(
    "Persistent Task".to_string(),
    "my_executor".to_string(),
    None,
    Some(1),
).await;
```

## Start & Stop Scheduler

```rust
scheduler.start().await; // Start main loop
// ...
scheduler.stop().await;  // Stop scheduler
```

## Query & Manage Tasks

```rust
let all_tasks = scheduler.get_all_tasks().await;
let task = scheduler.get_task(&task_id).await;
scheduler.remove_task(&task_id).await;
```

## Execute a Task Immediately

```rust
scheduler.execute_task_now(&task_id).await;
```

## Notes
- `executor_type` must match the registered executor name
- Scheduler is thread-safe (Arc+RwLock), can be shared across threads/tasks
- Task data can use serde_json::Value for custom parameters
- It is recommended that all task logic be idempotent to avoid side effects from retries
- Use logging and tracing for troubleshooting

---
For advanced usage (e.g., persistence, distributed scheduling), refer to the source code for extension. 