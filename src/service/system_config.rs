use crate::model::system_config;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

pub async fn list(db: &DatabaseConnection) -> anyhow::Result<Vec<system_config::Model>> {
    Ok(system_config::Entity::find().all(db).await?)
}

pub async fn get(
    db: &DatabaseConnection,
    key: &str,
) -> anyhow::Result<Option<system_config::Model>> {
    Ok(system_config::Entity::find()
        .filter(system_config::Column::Key.eq(key))
        .one(db)
        .await?)
}

pub async fn set(
    db: &DatabaseConnection,
    key: String,
    content: String,
) -> anyhow::Result<system_config::Model> {
    let model = system_config::ActiveModel {
        key: Set(key),
        content: Set(content),
        ..Default::default()
    };
    Ok(model.insert(db).await?)
}

pub async fn update(
    db: &DatabaseConnection,
    key: &str,
    content: String,
) -> anyhow::Result<Option<system_config::Model>> {
    let Some(existing) = get(db, key).await? else {
        return Ok(None);
    };
    let mut active: system_config::ActiveModel = existing.into();
    active.content = Set(content);
    Ok(Some(active.update(db).await?))
}

pub async fn delete(db: &DatabaseConnection, key: &str) -> anyhow::Result<bool> {
    Ok(system_config::Entity::delete_many()
        .filter(system_config::Column::Key.eq(key))
        .exec(db)
        .await?
        .rows_affected
        > 0)
}
