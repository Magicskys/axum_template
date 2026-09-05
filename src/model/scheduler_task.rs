use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "scheduler_tasks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub last_run: Option<DateTimeUtc>,
    pub next_run: Option<DateTimeUtc>,
    pub status: String,
    pub created_at: DateTimeUtc,
    pub executor_type: String,
    pub data: Option<Json>,
    pub timeout_seconds: Option<i64>,
    pub retry_count: i32,
    pub max_retries: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
