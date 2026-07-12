# 定时任务调度器使用说明

本项目内置轻量级内存任务调度器，支持一次性、循环、定时和长连接任务，适合单实例后端服务的定时与异步任务场景。

## 主要特性
- 支持一次性、循环、定时、长连接任务
- 基于最小堆+HashMap高效调度，堆顶永远是最近要执行的任务
- 最大并发保护（信号量实现，防止资源耗尽）
- 任务超时、重试机制
- 任务执行器可自定义，支持异步
- 详细日志与状态追踪

## 任务类型说明
- **OneTime**：立即执行一次，执行后保留结果状态
- **Recurring**：循环任务，按固定间隔反复执行
- **Scheduled**：定时任务，指定时间点执行
- **Persistent**：启动一次并持续运行，适合 WebSocket、消息队列消费者等长连接组件；删除任务或停止调度器时会中止执行

`Recurring` 会等待本次执行结束，再从完成时间计算下一次运行时间，因此同一个循环任务不会重叠执行。`Persistent` 只启动一次，正常情况下长期保持 `Running`；它不占用普通短任务的全局并发信号量。

## 状态与时间

- 状态包括 `pending`、`running`、`completed`、`failed`、`cancelled`
- `created_at`、`last_run`、`next_run` 均使用 UTC 时间
- `Scheduled` 严格使用传入的时间点，只执行一次
- `OneTime` 执行后保留结果状态，不会立即从任务表删除
- 失败重试会在 1 秒后重新入队；超过 `max_retries` 后标记为 `failed`

## 任务执行器（TaskExecutor）
每种任务类型都需指定执行器。实现 `TaskExecutor` trait 即可自定义任务逻辑。

```rust
use crate::scheduler::task_scheduler::{Task, TaskExecutor};

struct MyExecutor;

#[async_trait::async_trait]
impl TaskExecutor for MyExecutor {
    async fn execute(&self, task: &Task) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

## HTTP API

调度器接口位于 `/scheduler`，需要 Bearer token 和对应权限：

- `GET /scheduler/tasks`：`scheduler:read`
- `GET /scheduler/tasks/{id}`：`scheduler:read`
- `POST /scheduler/tasks`：`scheduler:write`
- `POST /scheduler/tasks/{id}/run`：`scheduler:write`
- `DELETE /scheduler/tasks/{id}`：`scheduler:write`

创建任务的 `task_type` 可为 `one_time`、`recurring`、`scheduled`、`persistent`。Recurring 必须提供大于 0 的 `interval_seconds`，Scheduled 必须提供 UTC RFC 3339 格式的 `next_run`。完整请求示例见 [API 文档](api.zh-CN.md)。

## 注意事项
- `executor_type` 必须与注册的执行器名称一致
- 调度器为线程安全（Arc+RwLock），可多线程/多协程共享
- 任务数据可用 serde_json::Value 传递自定义参数
- 推荐所有任务执行逻辑为幂等，避免重试带来副作用
- `Persistent` 执行器应正确处理 future 被取消的情况，确保连接和其他资源能随 future 一起释放
- 删除 Persistent 任务或调用 `scheduler.stop()` 会中止执行 future；需要发送关闭协议时，应在 executor 内加入显式取消处理
- 当前任务状态保存在内存中，进程重启后不会恢复；需要跨重启恢复时应接入数据库任务仓库
- 日志和 tracing 建议结合使用，便于排查问题

---
如需数据库持久化或分布式调度，可在当前执行器和任务仓库边界上继续扩展。
