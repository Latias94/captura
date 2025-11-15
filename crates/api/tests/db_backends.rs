//! 可选的多数据库后端迁移与基本 CRUD 冒烟
//! 默认忽略，设置环境变量后运行：
//!   cargo test -p captura-api --test db_backends -- --ignored

use captura_storage::connect as db_connect;
use chrono::{FixedOffset, Utc};
use migration::migrate;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

#[tokio::test]
#[ignore]
async fn postgres_migration_and_crud() {
    let Some(url) = std::env::var("CAPTURA_TEST_PG_URL").ok() else {
        eprintln!("skip: set CAPTURA_TEST_PG_URL to run");
        return;
    };
    let db = db_connect(&url).await.expect("connect pg");
    migrate(&db).await.expect("migrate pg");
    basic_user_roundtrip(&db).await;
}

#[tokio::test]
#[ignore]
async fn mysql_migration_and_crud() {
    let Some(url) = std::env::var("CAPTURA_TEST_MY_URL").ok() else {
        eprintln!("skip: set CAPTURA_TEST_MY_URL to run");
        return;
    };
    let db = db_connect(&url).await.expect("connect mysql");
    migrate(&db).await.expect("migrate mysql");
    basic_user_roundtrip(&db).await;
}

async fn basic_user_roundtrip(db: &sea_orm::DatabaseConnection) {
    use captura_storage::entity::user;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let name = format!("test_{}", now.timestamp_nanos());
    let _u = user::ActiveModel {
        username: Set(name.clone()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let got = user::Entity::find()
        .filter(user::Column::Username.eq(name))
        .one(db)
        .await
        .expect("select");
    assert!(got.is_some());
}
