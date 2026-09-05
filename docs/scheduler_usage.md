# Task Scheduler Usage Guide

This project includes a lightweight database-backed scheduler for one-time, recurring, scheduled, and long-lived connection tasks in a single service instance.

## Features
- Supports one-time, recurring, scheduled, and persistent tasks
- Efficient scheduling based on min-heap + HashMap, always keeping the soonest task at the top
- Max concurrency protection (semaphore-based, prevents resource exhaustion)
- Task timeout and retry mechanism
- Customizable task executors, async supported
- Persistent task definitions and execution history
- Panic isolation, graceful shutdown, and status tracking

## Task Types
- **OneTime**: Executes immediately once and retains its result status
- **Recurring**: Recurring task, executed repeatedly at fixed intervals
- **Scheduled**: Scheduled task, executed at a specified time
- **Persistent**: Keeps a connection or consumer running until cancellation. Unexpected returns and failures are reconnected automatically

A recurring task schedules its next run after the current execution completes, so the same task never overlaps itself. A persistent task normally remains `running`; it does not consume the semaphore slots reserved for transient jobs.

## Status and Time

- Status values are `pending`, `running`, `completed`, `failed`, and `cancelled`
- `created_at`, `last_run`, and `next_run` use UTC
- Scheduled tasks use the requested time and execute only once
- One-time tasks retain their final status
- Failed executions use exponential backoff from 1 to 64 seconds until `max_retries` is exhausted
- Persistent tasks reconnect without a retry limit; an unexpected successful return reconnects after one second
- Every attempt creates a `task_executions` row before executor code runs and records its finish status and error

Task definitions are stored in `scheduler_tasks`. On startup, tasks left in `pending` or `running` are restored after their executors have been registered. During graceful shutdown, queued tasks are not dispatched and remain pending for the next startup. Already-running finite tasks are allowed to finish, while Persistent executions are cancelled and their task definitions return to pending.

## Task Executor (TaskExecutor)
Each task type requires an executor. Implement the `TaskExecutor` trait to define your own logic.

```rust
use crate::scheduler::task_scheduler::{Task, TaskExecutor};
use tokio_util::sync::CancellationToken;

struct MyExecutor;

#[async_trait::async_trait]
impl TaskExecutor for MyExecutor {
    async fn execute(
        &self,
        task: &Task,
        cancellation: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Executing task: {}", task.name);
        tokio::select! {
            result = run_connection() => result?,
            () = cancellation.cancelled() => close_connection().await?,
        }
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

let scheduler = Arc::new(TaskScheduler::with_database(db.clone(), 100));
scheduler.add_executor(Box::new(MyExecutor)).await;
scheduler.restore_tasks().await?;
scheduler.start().await;
```

## Adding Tasks

```rust
// One-time task
let task_id = scheduler.add_one_time_task(
    "One-time Task".to_string(),
    "my_executor".to_string(),
    None,
    Some(30), // Timeout 30s
).await?;

// Recurring task
let task_id = scheduler.add_recurring_task(
    "Recurring Task".to_string(),
    60, // Every 60s
    "my_executor".to_string(),
    None,
    Some(20), // Timeout 20s
    3, // Max 3 retries
).await?;

// Scheduled task
use chrono::Utc;
let task_id = scheduler.add_scheduled_task(
    "Scheduled Task".to_string(),
    Utc::now() + chrono::Duration::seconds(120), // Execute after 2 min
    "my_executor".to_string(),
    None,
    Some(60),
).await?;

// Persistent task
let task_id = scheduler.add_persistent_task(
    "Persistent Task".to_string(),
    "my_executor".to_string(),
    None,
).await?;
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

## HTTP API

Scheduler endpoints require a Bearer token and the listed permission:

- `GET /scheduler/tasks`: `scheduler:read`
- `GET /scheduler/tasks/{id}`: `scheduler:read`
- `GET /scheduler/tasks/{id}/executions`: `scheduler:read`
- `POST /scheduler/tasks`: `scheduler:write`
- `POST /scheduler/tasks/{id}/run`: `scheduler:write`
- `DELETE /scheduler/tasks/{id}`: `scheduler:write`

`task_type` accepts `one_time`, `recurring`, `scheduled`, or `persistent`. Recurring tasks require a positive `interval_seconds`; scheduled tasks require `next_run` as a UTC RFC 3339 timestamp. See the [API guide](api.md) for full request examples.

## Notes
- `executor_type` is validated when the task is created and must match a registered executor name
- Scheduler is thread-safe (Arc+RwLock), can be shared across threads/tasks
- Task data can use serde_json::Value for custom parameters
- It is recommended that all task logic be idempotent to avoid side effects from retries
- Persistent executors should observe the supplied `CancellationToken`, close their external connection, and return
- `SIGINT` and `SIGTERM` stop new dispatches and wait for already-running OneTime, Scheduled, and Recurring executions to finish
- Persistent executions receive cancellation and have a five-second grace period before force-abort
- A finite task without `timeout_seconds` can therefore delay shutdown indefinitely; set a timeout when that is not acceptable
- `TaskScheduler::new` remains available for in-memory tests; the server uses `with_database`
- Use logging and tracing for troubleshooting

---
The scheduler is intended for one service instance. Distributed claiming and leases are still required before multiple instances can share the same scheduler tables safely.
