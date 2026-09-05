use crate::model::{login_log, scheduler_task, session, system_config, task, task_execution, user};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, RelationDef, RelationTrait, Schema,
    Set,
};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_entity_table(manager, user::Entity, None).await?;
        create_entity_table(manager, task::Entity, Some(task::Relation::User.def())).await?;
        create_entity_table(manager, system_config::Entity, None).await?;
        create_entity_table(
            manager,
            session::Entity,
            Some(session::Relation::User.def()),
        )
        .await?;
        create_entity_table(
            manager,
            login_log::Entity,
            Some(login_log::Relation::User.def()),
        )
        .await?;
        create_entity_table(manager, scheduler_task::Entity, None).await?;
        create_entity_table(
            manager,
            task_execution::Entity,
            Some(task_execution::Relation::SchedulerTask.def()),
        )
        .await?;

        create_index(manager, "idx_tasks_user_id", "tasks", "user_id").await?;
        create_index(manager, "idx_tasks_schedule_time", "tasks", "schedule_time").await?;
        create_index(manager, "idx_sessions_user_id", "sessions", "user_id").await?;
        create_index(manager, "idx_sessions_expires_at", "sessions", "expires_at").await?;
        create_index(manager, "idx_login_logs_user", "login_logs", "user_id").await?;
        create_index(
            manager,
            "idx_scheduler_tasks_next_run",
            "scheduler_tasks",
            "next_run",
        )
        .await?;
        create_index(
            manager,
            "idx_task_executions_task",
            "task_executions",
            "task_id",
        )
        .await?;
        create_index(
            manager,
            "idx_task_executions_started",
            "task_executions",
            "started_at",
        )
        .await?;
        create_index(
            manager,
            "idx_login_logs_created",
            "login_logs",
            "created_at",
        )
        .await?;

        for (key, content) in [
            ("user.registration_enabled", "true"),
            ("session.ttl_hours", "24"),
            ("scheduler.max_concurrent_tasks", "100"),
        ] {
            if system_config::Entity::find()
                .filter(system_config::Column::Key.eq(key))
                .one(manager.get_connection())
                .await?
                .is_none()
            {
                system_config::ActiveModel {
                    key: Set(key.to_string()),
                    content: Set(content.to_string()),
                    ..Default::default()
                }
                .insert(manager.get_connection())
                .await?;
            }
        }

        Ok(())
    }
}

async fn create_entity_table<E>(
    manager: &SchemaManager<'_>,
    entity: E,
    relation: Option<RelationDef>,
) -> Result<(), DbErr>
where
    E: EntityTrait,
{
    let schema = Schema::new(manager.get_database_backend());
    let mut table = schema.create_table_from_entity(entity);
    table.if_not_exists();
    if let Some(relation) = relation {
        table.foreign_key(&mut relation.into());
    }
    manager.create_table(table).await
}

async fn create_index(
    manager: &SchemaManager<'_>,
    name: &'static str,
    table: &'static str,
    column: &'static str,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name(name)
                .table(table)
                .col(column)
                .if_not_exists()
                .to_owned(),
        )
        .await
}
