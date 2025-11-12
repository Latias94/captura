//! 测试工具库：统一提供数据库初始化与常用种子数据
//! 仅用于测试/CI，不参与生产构建逻辑。

use base64::Engine as _;
use captura_storage::connect as db_connect;
use chrono::{FixedOffset, Utc};
use migration::migrate;
use rand_core::{OsRng, RngCore};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use sha2::{Digest, Sha256};

/// 使用 sqlite::memory 初始化数据库并执行全部迁移
pub async fn setup_db() -> DatabaseConnection {
    let db = db_connect("sqlite::memory:")
        .await
        .expect("connect sqlite::memory");
    migrate(&db).await.expect("run migrations");
    db
}

/// 创建用户并颁发一个明文 token（仅测试使用）
pub async fn seed_user_and_token(db: &DatabaseConnection, username: &str) -> (i64, String) {
    use captura_storage::entity::{prelude::*, token, user};
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // 如果用户已存在则复用
    if let Ok(Some(u)) = User::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await
    {
        // 为该用户创建新 token
        let mut rand_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut rand_bytes);
        let token_plain = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
        let token_hash = format!("{:x}", Sha256::digest(token_plain.as_bytes()));
        let am = token::ActiveModel {
            user_id: Set(u.id),
            name: Set(Some("test".into())),
            token_hash: Set(token_hash),
            token_plain: Set(Some(token_plain.clone())),
            created_at: Set(now),
            last_used_at: Set(Some(now)),
            expires_at: Set(None),
            ..Default::default()
        };
        let _ = am.insert(db).await.expect("insert token");
        return (u.id, token_plain);
    }

    let u = user::ActiveModel {
        username: Set(username.to_string()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");

    let mut rand_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut rand_bytes);
    let token_plain = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_plain.as_bytes()));
    let am = token::ActiveModel {
        user_id: Set(u.id),
        name: Set(Some("test".into())),
        token_hash: Set(token_hash),
        token_plain: Set(Some(token_plain.clone())),
        created_at: Set(now),
        last_used_at: Set(Some(now)),
        expires_at: Set(None),
        ..Default::default()
    };
    let _ = am.insert(db).await.expect("insert token");
    (u.id, token_plain)
}
