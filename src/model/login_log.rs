use sea_orm::entity::prelude::*;

crate::define_permissions!(login_log => [read]);

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "login_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: Option<i32>,
    pub username: String,
    pub success: bool,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_delete = "SetNull"
    )]
    User,
}

impl ActiveModelBehavior for ActiveModel {}
