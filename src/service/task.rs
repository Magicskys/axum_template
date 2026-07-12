use crate::model::task;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

pub async fn add_task(
    db: &DatabaseConnection,
    user_id: i32,
    action: &str,
    schedule_time: DateTime<Utc>,
) -> anyhow::Result<task::Model> {
    let new_task = task::ActiveModel {
        user_id: Set(user_id),
        action: Set(action.to_string()),
        schedule_time: Set(schedule_time),
        ..Default::default()
    };

    Ok(new_task.insert(db).await?)
}

pub async fn get_all_tasks(db: &DatabaseConnection) -> anyhow::Result<Vec<task::Model>> {
    Ok(task::Entity::find()
        .order_by_asc(task::Column::ScheduleTime)
        .all(db)
        .await?)
}

pub async fn get_task_by_id(
    db: &DatabaseConnection,
    id: i32,
) -> anyhow::Result<Option<task::Model>> {
    Ok(task::Entity::find_by_id(id).one(db).await?)
}

pub async fn get_tasks_by_user(
    db: &DatabaseConnection,
    user_id: i32,
) -> anyhow::Result<Vec<task::Model>> {
    Ok(task::Entity::find()
        .filter(task::Column::UserId.eq(user_id))
        .order_by_asc(task::Column::ScheduleTime)
        .all(db)
        .await?)
}

pub async fn update_task(
    db: &DatabaseConnection,
    id: i32,
    action: Option<String>,
    schedule_time: Option<DateTime<Utc>>,
) -> anyhow::Result<Option<task::Model>> {
    let Some(existing) = task::Entity::find_by_id(id).one(db).await? else {
        return Ok(None);
    };

    let mut active: task::ActiveModel = existing.into();
    if let Some(action) = action {
        active.action = Set(action);
    }
    if let Some(schedule_time) = schedule_time {
        active.schedule_time = Set(schedule_time);
    }

    Ok(Some(active.update(db).await?))
}

pub async fn delete_task(db: &DatabaseConnection, id: i32) -> anyhow::Result<bool> {
    let result = task::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}
