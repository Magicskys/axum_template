// Here we will put the sea-orm scheduled task entity in the future

use sea_orm::entity::prelude::*;

crate::define_permissions!(task => [read, write]);

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub action: String,
    pub schedule_time: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl RelationTrait for Relation {
    fn def(&self) -> sea_orm::entity::RelationDef {
        panic!("No Relation")
    }
}

impl ActiveModelBehavior for ActiveModel {}
