use crate::{database_url_for_startup, init_database};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Database, Set};

#[test]
fn sqlite_file_url_enables_create_mode() {
    assert_eq!(
        database_url_for_startup("sqlite://fresh.db"),
        "sqlite://fresh.db?mode=rwc"
    );
    assert_eq!(
        database_url_for_startup("sqlite://fresh.db?cache=shared"),
        "sqlite://fresh.db?cache=shared&mode=rwc"
    );
    assert_eq!(
        database_url_for_startup("postgres://localhost/app"),
        "postgres://localhost/app"
    );
}

#[test]
fn generated_admin_password_is_alphanumeric() {
    let password = crate::generate_password();
    assert_eq!(password.len(), 20);
    assert!(password.bytes().all(|byte| byte.is_ascii_alphanumeric()));
}

#[tokio::test]
async fn fresh_database_is_initialized_once() {
    let db = Database::connect("sqlite::memory:").await.unwrap();

    init_database(&db).await.unwrap();
    init_database(&db).await.unwrap();

    let configs = crate::service::system_config::list(&db).await.unwrap();
    assert_eq!(configs.len(), 3);
    assert!(
        configs
            .iter()
            .any(|item| item.key == "user.registration_enabled")
    );
}

#[tokio::test]
async fn sea_orm_rbac_assigns_expected_permissions() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    init_database(&db).await.unwrap();

    let now = Utc::now();
    let admin = crate::model::user::ActiveModel {
        username: Set("admin".to_string()),
        password_hash: Set("hash".to_string()),
        email: Set(None),
        is_active: Set(true),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        last_login_at: Set(None),
        last_login_ip: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let user = crate::model::user::ActiveModel {
        username: Set("user".to_string()),
        password_hash: Set("hash".to_string()),
        email: Set(None),
        is_active: Set(true),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        last_login_at: Set(None),
        last_login_ip: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    init_database(&db).await.unwrap();

    let mut admin_permissions = crate::service::rbac::permissions_for_user(&db, admin.id)
        .await
        .unwrap();
    let user_permissions = crate::service::rbac::permissions_for_user(&db, user.id)
        .await
        .unwrap();
    admin_permissions.sort();
    assert_eq!(
        admin_permissions,
        vec![
            "login_log:read",
            "scheduler:read",
            "scheduler:write",
            "system_config:read",
            "system_config:write",
            "task:read",
            "task:write",
            "user:manage",
        ]
    );
    assert_eq!(user_permissions, vec!["task:read", "task:write"]);
}

#[tokio::test]
async fn missing_sqlite_file_is_created() {
    let path = std::env::temp_dir().join(format!("axum-template-{}.db", uuid::Uuid::new_v4()));
    let configured_url = format!("sqlite://{}", path.display());
    let db = Database::connect(database_url_for_startup(&configured_url))
        .await
        .unwrap();

    init_database(&db).await.unwrap();
    assert!(path.exists());
    assert_eq!(
        crate::service::system_config::list(&db)
            .await
            .unwrap()
            .len(),
        3
    );

    db.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}
