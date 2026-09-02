use crate::model::{login_log, session, system_config, task, user};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Schema, Set};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_entity_table(manager, user::Entity).await?;
        create_entity_table(manager, task::Entity).await?;
        create_entity_table(manager, system_config::Entity).await?;
        create_entity_table(manager, session::Entity).await?;
        create_entity_table(manager, login_log::Entity).await?;

        create_index(manager, "idx_tasks_user_id", "tasks", "user_id").await?;
        create_index(manager, "idx_tasks_schedule_time", "tasks", "schedule_time").await?;
        create_index(manager, "idx_sessions_user_id", "sessions", "user_id").await?;
        create_index(manager, "idx_sessions_expires_at", "sessions", "expires_at").await?;
        create_index(manager, "idx_login_logs_user", "login_logs", "user_id").await?;
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

async fn create_entity_table<E>(manager: &SchemaManager<'_>, entity: E) -> Result<(), DbErr>
where
    E: EntityTrait,
{
    let schema = Schema::new(manager.get_database_backend());
    let mut table = schema.create_table_from_entity(entity);
    table.if_not_exists();
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
