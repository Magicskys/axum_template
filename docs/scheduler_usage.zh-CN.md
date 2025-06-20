# 定时任务调度器使用说明

本项目内置高性能定时任务调度器，支持大规模并发、持久化、一次性、循环等多种任务类型，适合企业级/个人后端服务的定时与异步任务场景。

## 主要特性
- 支持一次性、循环、定时、持久化任务
- 基于最小堆+HashMap高效调度，堆顶永远是最近要执行的任务
- 最大并发保护（信号量实现，防止资源耗尽）
- 任务超时、重试机制
- 任务执行器可自定义，支持异步
- 详细日志与状态追踪

## 任务类型说明
- **OneTime**：一次性任务，执行后自动删除
- **Recurring**：循环任务，按固定间隔反复执行
- **Scheduled**：定时任务，指定时间点执行
- **Persistent**：持久化任务，适合长时间运行

## 任务执行器（TaskExecutor）
每种任务类型都需指定执行器。实现 `TaskExecutor` trait 即可自定义任务逻辑。

```rust
use crate::scheduler::task_scheduler::{Task, TaskExecutor};

struct MyExecutor;

impl TaskExecutor for MyExecutor {
    fn execute(&self, task: &Task) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("执行任务: {}", task.name);
        // 业务逻辑 ...
        Ok(())
    }
    fn get_name(&self) -> &str {
        "my_executor"
    }
}
```


## 调度器初始化与注册执行器

```rust
use crate::scheduler::task_scheduler::{TaskScheduler, MyExecutor};
use std::sync::Arc;

let scheduler = Arc::new(TaskScheduler::new(100)); // 最大并发100
scheduler.add_executor(Box::new(MyExecutor)).await;
```

## 添加任务示例

```rust
// 一次性任务
let task_id = scheduler.add_one_time_task(
    "单次任务".to_string(),
    "my_executor".to_string(),
    None,
    Some(30), // 超时30秒
).await;

// 循环任务
let task_id = scheduler.add_recurring_task(
    "循环任务".to_string(),
    60, // 每60秒执行
    "my_executor".to_string(),
    None,
    Some(2), // 最大并发2
    Some(20), // 超时20秒
    3, // 最多重试3次
).await;

// 定时任务
use chrono::Utc;
let task_id = scheduler.add_scheduled_task(
    "定时任务".to_string(),
    Utc::now() + chrono::Duration::seconds(120), // 2分钟后执行
    "my_executor".to_string(),
    None,
    Some(60),
).await;

// 持久化任务
let task_id = scheduler.add_persistent_task(
    "持久化任务".to_string(),
    "my_executor".to_string(),
    None,
    Some(1),
).await;
```

## 启动与停止调度器

```rust
scheduler.start().await; // 启动主循环
// ...
scheduler.stop().await;  // 停止调度器
```

## 查询与管理任务

```rust
let all_tasks = scheduler.get_all_tasks().await;
let task = scheduler.get_task(&task_id).await;
scheduler.remove_task(&task_id).await;
```

## 立即执行某个任务

```rust
scheduler.execute_task_now(&task_id).await;
```

## 注意事项
- `executor_type` 必须与注册的执行器名称一致
- 调度器为线程安全（Arc+RwLock），可多线程/多协程共享
- 任务数据可用 serde_json::Value 传递自定义参数
- 推荐所有任务执行逻辑为幂等，避免重试带来副作用
- 日志和 tracing 建议结合使用，便于排查问题

---
如需更复杂的用法（如持久化、分布式调度等），可参考源码扩展。 