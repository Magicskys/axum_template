use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "task_executions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub task_id: String,
    pub attempt: i32,
    pub status: String,
    pub started_at: DateTimeUtc,
    pub finished_at: Option<DateTimeUtc>,
    pub error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::scheduler_task::Entity",
        from = "Column::TaskId",
        to = "super::scheduler_task::Column::Id",
        on_delete = "Cascade"
    )]
    SchedulerTask,
}

impl ActiveModelBehavior for ActiveModel {}
