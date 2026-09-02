use crate::model::{task, user};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, Set,
    rbac::{
        Action, RbacEngine, RbacUserId, Table,
        entity::{
            permission, resource, role, role_permission,
            user_role::{self, ActiveModel as UserRole},
        },
    },
};
use sea_orm_migration::SchemaManager;

pub const API_RESOURCE: &str = "api";
pub const ADMIN_ROLE: &str = "admin";
pub const USER_ROLE: &str = "user";

#[macro_export]
macro_rules! define_permissions {
    ($resource:ident => [$($action:ident),+ $(,)?]) => {
        pub const RBAC_PERMISSIONS: &[&str] = &[
            $(concat!(stringify!($resource), ":", stringify!($action)),)+
        ];

        ::inventory::submit! {
            $crate::service::rbac::PermissionGroup {
                permissions: RBAC_PERMISSIONS,
            }
        }
    };
}

pub struct PermissionGroup {
    pub permissions: &'static [&'static str],
}

inventory::collect!(PermissionGroup);

fn all_permissions() -> impl Iterator<Item = &'static str> {
    inventory::iter::<PermissionGroup>().flat_map(|group| group.permissions.iter().copied())
}

pub async fn initialize(db: &DatabaseConnection) -> Result<(), DbErr> {
    initialize_rbac_schema(db).await?;
    seed_permissions(db).await?;
    let resource = find_or_create_resource(db, API_RESOURCE).await?;
    let admin_role = find_or_create_role(db, ADMIN_ROLE).await?;
    let user_role = find_or_create_role(db, USER_ROLE).await?;

    grant_permissions(db, admin_role.id, resource.id, all_permissions()).await?;
    grant_permissions(
        db,
        user_role.id,
        resource.id,
        task::RBAC_PERMISSIONS.iter().copied(),
    )
    .await?;

    assign_unassigned_users(db).await
}

pub async fn permissions_for_user(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<Vec<String>, DbErr> {
    let engine = RbacEngine::load_from(db).await?;
    let user_id = RbacUserId(i64::from(user_id));
    Ok(all_permissions()
        .filter(|permission| {
            engine
                .user_can(user_id, Action(permission), Table(API_RESOURCE))
                .unwrap_or(false)
        })
        .map(str::to_string)
        .collect())
}

pub async fn assign_registration_role(db: &DatabaseConnection, user_id: i32) -> Result<(), DbErr> {
    let admin_role = find_role(db, ADMIN_ROLE).await?;
    let admin_exists = user_role::Entity::find()
        .filter(user_role::Column::RoleId.eq(admin_role.id))
        .count(db)
        .await?
        > 0;
    assign_role(
        db,
        user_id,
        if admin_exists { USER_ROLE } else { ADMIN_ROLE },
    )
    .await
}

pub async fn assign_role(
    db: &DatabaseConnection,
    user_id: i32,
    role_name: &str,
) -> Result<(), DbErr> {
    let role = find_role(db, role_name).await?;
    if let Some(existing) = user_role::Entity::find_by_id(RbacUserId(i64::from(user_id)))
        .one(db)
        .await?
    {
        let mut active: UserRole = existing.into();
        active.role_id = Set(role.id);
        active.update(db).await?;
    } else {
        user_role::Entity::insert(UserRole {
            user_id: Set(RbacUserId(i64::from(user_id))),
            role_id: Set(role.id),
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

pub async fn replace_role(
    db: &DatabaseConnection,
    user_id: i32,
    role_name: &str,
) -> anyhow::Result<()> {
    let current = user_role::Entity::find_by_id(RbacUserId(i64::from(user_id)))
        .one(db)
        .await?;
    let admin_role = find_role(db, ADMIN_ROLE).await?;
    if current.is_some_and(|assignment| assignment.role_id == admin_role.id)
        && role_name != ADMIN_ROLE
        && user_role::Entity::find()
            .filter(user_role::Column::RoleId.eq(admin_role.id))
            .count(db)
            .await?
            <= 1
    {
        anyhow::bail!("cannot remove the last admin role");
    }
    assign_role(db, user_id, role_name).await?;
    Ok(())
}

async fn initialize_rbac_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    let schema = SchemaManager::new(db);
    if !schema.has_table("sea_orm_permission").await? {
        sea_orm::rbac::schema::create_tables(db, Default::default()).await?;
    }
    Ok(())
}

async fn seed_permissions(db: &DatabaseConnection) -> Result<(), DbErr> {
    for action in all_permissions() {
        if permission::Entity::find()
            .filter(permission::Column::Action.eq(action))
            .one(db)
            .await?
            .is_none()
        {
            permission::Entity::insert(permission::ActiveModel {
                action: Set(action.to_string()),
                ..Default::default()
            })
            .exec(db)
            .await?;
        }
    }
    Ok(())
}

async fn find_or_create_resource(
    db: &DatabaseConnection,
    table_name: &str,
) -> Result<resource::Model, DbErr> {
    if let Some(resource) = resource::Entity::find()
        .filter(resource::Column::Table.eq(table_name))
        .one(db)
        .await?
    {
        return Ok(resource);
    }
    resource::Entity::insert(resource::ActiveModel {
        schema: Set(None),
        table: Set(table_name.to_string()),
        ..Default::default()
    })
    .exec_with_returning(db)
    .await
}

async fn find_or_create_role(
    db: &DatabaseConnection,
    role_name: &str,
) -> Result<role::Model, DbErr> {
    if let Some(role) = role::Entity::find()
        .filter(role::Column::Role.eq(role_name))
        .one(db)
        .await?
    {
        return Ok(role);
    }
    role::Entity::insert(role::ActiveModel {
        role: Set(role_name.to_string()),
        ..Default::default()
    })
    .exec_with_returning(db)
    .await
}

async fn grant_permissions<'a>(
    db: &DatabaseConnection,
    role_id: role::RoleId,
    resource_id: resource::ResourceId,
    actions: impl IntoIterator<Item = &'a str>,
) -> Result<(), DbErr> {
    for action in actions {
        let permission = permission::Entity::find()
            .filter(permission::Column::Action.eq(action))
            .one(db)
            .await?
            .ok_or_else(|| DbErr::RbacError(format!("permission not found: {action}")))?;
        let exists = role_permission::Entity::find()
            .filter(role_permission::Column::RoleId.eq(role_id))
            .filter(role_permission::Column::PermissionId.eq(permission.id))
            .filter(role_permission::Column::ResourceId.eq(resource_id))
            .one(db)
            .await?
            .is_some();
        if !exists {
            role_permission::Entity::insert(role_permission::ActiveModel {
                role_id: Set(role_id),
                permission_id: Set(permission.id),
                resource_id: Set(resource_id),
            })
            .exec(db)
            .await?;
        }
    }
    Ok(())
}

async fn assign_unassigned_users(db: &DatabaseConnection) -> Result<(), DbErr> {
    let users = user::Entity::find()
        .order_by_asc(user::Column::Id)
        .all(db)
        .await?;
    for (index, user) in users.into_iter().enumerate() {
        if user_role::Entity::find_by_id(RbacUserId(i64::from(user.id)))
            .one(db)
            .await?
            .is_none()
        {
            assign_role(db, user.id, if index == 0 { ADMIN_ROLE } else { USER_ROLE }).await?;
        }
    }
    Ok(())
}

async fn find_role(db: &DatabaseConnection, role_name: &str) -> Result<role::Model, DbErr> {
    role::Entity::find()
        .filter(role::Column::Role.eq(role_name))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::RbacError(format!("role not found: {role_name}")))
}
