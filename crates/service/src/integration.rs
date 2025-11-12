use async_trait::async_trait;
use captura_storage::entity::integration;
use reqwest::Client;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

#[derive(Debug, Clone)]
pub struct IntegrationCtx<'a> {
    pub db: &'a DatabaseConnection,
    pub http: &'a Client,
}

#[async_trait]
pub trait Integration: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn on_new_entries(
        &self,
        _ctx: &IntegrationCtx<'_>,
        _user_id: i64,
        _feed: &captura_storage::entity::feed::Model,
        _entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_save_entry(
        &self,
        _ctx: &IntegrationCtx<'_>,
        _user_id: i64,
        _entry: &captura_storage::entity::entry::Model,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

// Wallabag: POST {base_url}/api/entries.json Authorization: Bearer {token} body: {url}
#[derive(Debug, Clone, serde::Deserialize)]
struct WallabagCfg {
    base_url: String,
    access_token: String,
}

struct Wallabag;
#[async_trait]
impl Integration for Wallabag {
    fn kind(&self) -> &'static str {
        "wallabag"
    }
    async fn on_save_entry(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: i64,
        entry: &captura_storage::entity::entry::Model,
    ) -> anyhow::Result<()> {
        let cfg: WallabagCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
        if let Some(url) = &entry.url {
            let api = format!("{}/api/entries.json", cfg.base_url.trim_end_matches('/'));
            let payload = serde_json::json!({ "url": url });
            let _ = ctx
                .http
                .post(api)
                .bearer_auth(cfg.access_token)
                .json(&payload)
                .send()
                .await;
        }
        Ok(())
    }
}

// Telegram: sendMessage
#[derive(Debug, Clone, serde::Deserialize)]
struct TelegramCfg {
    bot_token: String,
    chat_id: String,
}
struct Telegram;
#[async_trait]
impl Integration for Telegram {
    fn kind(&self) -> &'static str {
        "telegram"
    }
    async fn on_new_entries(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: i64,
        _feed: &captura_storage::entity::feed::Model,
        entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        let cfg: TelegramCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
        if entry_ids.is_empty() {
            return Ok(());
        }
        use captura_storage::entity::entry;
        let entries = entry::Entity::find()
            .filter(entry::Column::Id.is_in(entry_ids.to_vec()))
            .all(ctx.db)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        for e in entries {
            let text = format!(
                "{}\n{}",
                e.title.clone().unwrap_or_default(),
                e.url.clone().unwrap_or_default()
            );
            let api = format!("https://api.telegram.org/bot{}/sendMessage", cfg.bot_token);
            let payload = serde_json::json!({ "chat_id": cfg.chat_id, "text": text });
            let _ = ctx.http.post(api).json(&payload).send().await;
        }
        Ok(())
    }
}

async fn ctx_cfg<T: for<'de> serde::Deserialize<'de>>(
    db: &DatabaseConnection,
    user_id: i64,
    kind: &str,
) -> anyhow::Result<T> {
    let cfg = integration::Entity::find()
        .filter(integration::Column::UserId.eq(user_id))
        .filter(integration::Column::Kind.eq(kind))
        .filter(integration::Column::Enabled.eq(true))
        .one(db)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let cfg = cfg.ok_or_else(|| anyhow::anyhow!("integration not configured"))?;
    let json = cfg
        .config_json
        .ok_or_else(|| anyhow::anyhow!("config missing"))?;
    serde_json::from_value::<T>(json).map_err(|e| anyhow::anyhow!(e.to_string()))
}

pub async fn emit_new_entries(
    db: &DatabaseConnection,
    user_id: i64,
    feed: &captura_storage::entity::feed::Model,
    entry_ids: &[i64],
) {
    let http = Client::new();
    let ctx = IntegrationCtx { db, http: &http };
    let impls: Vec<Box<dyn Integration>> = vec![Box::new(Wallabag), Box::new(Telegram)];
    for integ in impls {
        if allowed_for_feed(integ.kind(), feed) {
            let _ = integ.on_new_entries(&ctx, user_id, feed, entry_ids).await;
        }
    }
}

pub async fn emit_save_entry(
    db: &DatabaseConnection,
    user_id: i64,
    entry: &captura_storage::entity::entry::Model,
) {
    let http = Client::new();
    let ctx = IntegrationCtx { db, http: &http };
    let impls: Vec<Box<dyn Integration>> = vec![Box::new(Wallabag), Box::new(Telegram)];
    // 加载 feed 以做每订阅启用判断
    let feed = captura_storage::entity::feed::Entity::find_by_id(entry.feed_id)
        .one(db)
        .await
        .ok()
        .flatten();
    for integ in impls {
        if feed
            .as_ref()
            .map(|f| allowed_for_feed(integ.kind(), f))
            .unwrap_or(true)
        {
            let _ = integ.on_save_entry(&ctx, user_id, entry).await;
        }
    }
}

fn allowed_for_feed(kind: &str, feed: &captura_storage::entity::feed::Model) -> bool {
    if let Some(ref json) = feed.integrations_json {
        if let Some(obj) = json.as_object() {
            if let Some(v) = obj.get(kind) {
                if let Some(b) = v.as_bool() {
                    return b;
                }
                if let Some(o) = v.as_object() {
                    if let Some(b) = o.get("enabled").and_then(|x| x.as_bool()) {
                        return b;
                    }
                }
                return false;
            }
            return false;
        }
    }
    true
}
