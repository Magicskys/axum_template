use crate::{
    model::{scheduler_task, task_execution},
    scheduler::task_scheduler::{Task, TaskStatus},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    sea_query::Expr,
};
use uuid::Uuid;

pub async fn insert_task(db: &DatabaseConnection, task: &Task) -> Result<(), sea_orm::DbErr> {
    scheduler_task::ActiveModel {
        id: Set(task.id.clone()),
        name: Set(task.name.clone()),
        task_type: Set(serde_json::to_string(&task.task_type).unwrap_or_default()),
        last_run: Set(task.last_run),
        next_run: Set(task.next_run),
        status: Set(status_name(&task.status).to_string()),
        created_at: Set(task.created_at),
        executor_type: Set(task.executor_type.clone()),
        data: Set(task.data.clone()),
        timeout_seconds: Set(task
            .timeout_seconds
            .and_then(|value| i64::try_from(value).ok())),
        retry_count: Set(task.retry_count as i32),
        max_retries: Set(task.max_retries.min(i32::MAX as u32) as i32),
    }
    .insert(db)
    .await?;
    Ok(())
}

pub async fn load_recoverable_tasks(db: &DatabaseConnection) -> Result<Vec<Task>, sea_orm::DbErr> {
    let models = scheduler_task::Entity::find().all(db).await?;
    models
        .into_iter()
        .filter(|model| model.status == "pending" || model.status == "running")
        .map(|model| {
            let task_type = serde_json::from_str(&model.task_type)
                .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))?;
            Ok(Task {
                id: model.id,
                name: model.name,
                task_type,
                last_run: model.last_run,
                next_run: model.next_run,
                status: TaskStatus::Pending,
                created_at: model.created_at,
                executor_type: model.executor_type,
                data: model.data,
                timeout_seconds: model
                    .timeout_seconds
                    .and_then(|value| u64::try_from(value).ok()),
                retry_count: model.retry_count.max(0) as u32,
                max_retries: model.max_retries.max(0) as u32,
            })
        })
        .collect()
}

pub async fn update_task(db: &DatabaseConnection, task: &Task) -> Result<(), sea_orm::DbErr> {
    let Some(model) = scheduler_task::Entity::find_by_id(&task.id).one(db).await? else {
        return Ok(());
    };
    let mut active: scheduler_task::ActiveModel = model.into();
    active.last_run = Set(task.last_run);
    active.next_run = Set(task.next_run);
    active.status = Set(status_name(&task.status).to_string());
    active.retry_count = Set(task.retry_count as i32);
    active.update(db).await?;
    Ok(())
}

pub async fn start_execution(
    db: &DatabaseConnection,
    task: &Task,
) -> Result<String, sea_orm::DbErr> {
    let id = Uuid::new_v4().to_string();
    task_execution::ActiveModel {
        id: Set(id.clone()),
        task_id: Set(task.id.clone()),
        attempt: Set(task.retry_count as i32 + 1),
        status: Set("running".to_string()),
        started_at: Set(Utc::now()),
        finished_at: Set(None),
        error: Set(None),
    }
    .insert(db)
    .await?;
    Ok(id)
}

pub async fn finish_execution(
    db: &DatabaseConnection,
    execution_id: &str,
    status: &str,
    error: Option<String>,
) -> Result<(), sea_orm::DbErr> {
    let Some(model) = task_execution::Entity::find_by_id(execution_id)
        .one(db)
        .await?
    else {
        return Ok(());
    };
    let mut active: task_execution::ActiveModel = model.into();
    active.status = Set(status.to_string());
    active.finished_at = Set(Some(Utc::now()));
    active.error = Set(error);
    active.update(db).await?;
    Ok(())
}

pub async fn list_executions(
    db: &DatabaseConnection,
    task_id: &str,
) -> Result<Vec<task_execution::Model>, sea_orm::DbErr> {
    task_execution::Entity::find()
        .filter(task_execution::Column::TaskId.eq(task_id))
        .order_by_desc(task_execution::Column::StartedAt)
        .all(db)
        .await
}

pub async fn cancel_running_executions(
    db: &DatabaseConnection,
    task_ids: &[String],
) -> Result<(), sea_orm::DbErr> {
    if task_ids.is_empty() {
        return Ok(());
    }
    task_execution::Entity::update_many()
        .col_expr(task_execution::Column::Status, Expr::value("cancelled"))
        .col_expr(
            task_execution::Column::FinishedAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(task_execution::Column::TaskId.is_in(task_ids.iter().cloned()))
        .filter(task_execution::Column::Status.eq("running"))
        .exec(db)
        .await?;
    Ok(())
}

fn status_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}
