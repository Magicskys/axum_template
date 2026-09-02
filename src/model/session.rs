use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub token: String,
    pub user_id: i32,
    pub expires_at: DateTimeUtc,
    pub created_at: Option<DateTimeUtc>,
    pub last_seen_at: Option<DateTimeUtc>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub revoked_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No Relation")
    }
}

impl ActiveModelBehavior for ActiveModel {}
