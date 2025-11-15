//! Optional smoke tests for multi-database backends (Postgres/MySQL) migrations and basic CRUD.
//! Ignored by default; run explicitly with:
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
    let db = match db_connect(&url).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("skip: connect pg failed: {e}");
            return;
        }
    };
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
    let db = match db_connect(&url).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("skip: connect mysql failed: {e}");
            return;
        }
    };
    migrate(&db).await.expect("migrate mysql");
    basic_user_roundtrip(&db).await;
}

async fn basic_user_roundtrip(db: &sea_orm::DatabaseConnection) {
    use captura_storage::entity::user;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let name = format!("test_{}", now.timestamp_micros());
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
