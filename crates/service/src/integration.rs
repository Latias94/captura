use async_trait::async_trait;
use captura_common::UserId;
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
        _user_id: UserId,
        _feed: &captura_storage::entity::feed::Model,
        _entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_save_entry(
        &self,
        _ctx: &IntegrationCtx<'_>,
        _user_id: UserId,
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
        user_id: UserId,
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
        user_id: UserId,
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
    user_id: UserId,
    kind: &str,
) -> anyhow::Result<T> {
    let cfg = integration::Entity::find()
        .filter(integration::Column::UserId.eq(user_id.0))
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
    let http = crate::http_client_basic().unwrap_or_else(|_| Client::new());
    let ctx = IntegrationCtx { db, http: &http };
    let impls: Vec<Box<dyn Integration>> = vec![
        Box::new(Wallabag),
        Box::new(Telegram),
        Box::new(Ntfy),
        Box::new(Slack),
        Box::new(Pocket),
        Box::new(Instapaper),
        Box::new(Pushover),
        Box::new(Matrix),
    ];
    for integ in impls {
        if allowed_for_feed(integ.kind(), feed) {
            let _ = integ
                .on_new_entries(&ctx, UserId(user_id), feed, entry_ids)
                .await;
        }
    }
}

pub async fn emit_save_entry(
    db: &DatabaseConnection,
    user_id: i64,
    entry: &captura_storage::entity::entry::Model,
) {
    let http = crate::http_client_basic().unwrap_or_else(|_| Client::new());
    let ctx = IntegrationCtx { db, http: &http };
    let impls: Vec<Box<dyn Integration>> = vec![
        Box::new(Wallabag),
        Box::new(Telegram),
        Box::new(Ntfy),
        Box::new(Slack),
        Box::new(Pocket),
        Box::new(Instapaper),
        Box::new(Pushover),
        Box::new(Matrix),
    ];
    // Load feed to determine per-subscription enablement
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
            let _ = integ.on_save_entry(&ctx, UserId(user_id), entry).await;
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

// ---------------- Ntfy ----------------
#[derive(Debug, Clone, serde::Deserialize)]
struct NtfyCfg {
    #[serde(default = "ntfy_default_base")]
    base_url: String, // e.g. https://ntfy.sh
    topic: String,
    token: Option<String>,
}
fn ntfy_default_base() -> String {
    "https://ntfy.sh".to_string()
}

struct Ntfy;
#[async_trait]
impl Integration for Ntfy {
    fn kind(&self) -> &'static str {
        "ntfy"
    }
    async fn on_new_entries(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        _feed: &captura_storage::entity::feed::Model,
        entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        let cfg: NtfyCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
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
            let url = format!(
                "{}/{}",
                cfg.base_url.trim_end_matches('/'),
                cfg.topic.trim_start_matches('/')
            );
            let mut rq = ctx.http.post(url).body(text);
            if let Some(tok) = cfg.token.as_ref() {
                rq = rq.header("Authorization", format!("Bearer {}", tok));
            }
            let _ = rq.header("X-Title", "Captura: New Entry").send().await;
        }
        Ok(())
    }
    async fn on_save_entry(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        entry: &captura_storage::entity::entry::Model,
    ) -> anyhow::Result<()> {
        let cfg: NtfyCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
        let text = format!(
            "Saved: {}\n{}",
            entry.title.clone().unwrap_or_default(),
            entry.url.clone().unwrap_or_default()
        );
        let url = format!(
            "{}/{}",
            cfg.base_url.trim_end_matches('/'),
            cfg.topic.trim_start_matches('/')
        );
        let mut rq = ctx.http.post(url).body(text);
        if let Some(tok) = cfg.token.as_ref() {
            rq = rq.header("Authorization", format!("Bearer {}", tok));
        }
        let _ = rq.header("X-Title", "Captura: Saved Entry").send().await;
        Ok(())
    }
}

// ---------------- Slack (Incoming Webhook) ----------------
#[derive(Debug, Clone, serde::Deserialize)]
struct SlackCfg {
    incoming_webhook_url: String,
}

// ---------------- Pocket ----------------
// Docs: https://getpocket.com/developer/docs/v3/add
#[derive(Debug, Clone, serde::Deserialize)]
struct PocketCfg {
    consumer_key: String,
    access_token: String,
}

struct Pocket;
#[async_trait]
impl Integration for Pocket {
    fn kind(&self) -> &'static str {
        "pocket"
    }
    async fn on_save_entry(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        entry: &captura_storage::entity::entry::Model,
    ) -> anyhow::Result<()> {
        let cfg: PocketCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
        if let Some(url) = &entry.url {
            let _ = ctx
                .http
                .post("https://getpocket.com/v3/add")
                .header("Content-Type", "application/json; charset=UTF-8")
                .json(&serde_json::json!({
                    "consumer_key": cfg.consumer_key,
                    "access_token": cfg.access_token,
                    "url": url,
                    "title": entry.title.clone().unwrap_or_default(),
                }))
                .send()
                .await;
        }
        Ok(())
    }
}

// ---------------- Instapaper ----------------
// Simple API: https://www.instapaper.com/api (add)
#[derive(Debug, Clone, serde::Deserialize)]
struct InstapaperCfg {
    username: String,
    password: String,
}

struct Instapaper;
#[async_trait]
impl Integration for Instapaper {
    fn kind(&self) -> &'static str {
        "instapaper"
    }
    async fn on_save_entry(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        entry: &captura_storage::entity::entry::Model,
    ) -> anyhow::Result<()> {
        let cfg: InstapaperCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
        if let Some(url) = &entry.url {
            let mut form = std::collections::HashMap::new();
            form.insert("username", cfg.username);
            form.insert("password", cfg.password);
            form.insert("url", url.clone());
            if let Some(t) = &entry.title {
                form.insert("title", t.clone());
            }
            let _ = ctx
                .http
                .post("https://www.instapaper.com/api/add")
                .form(&form)
                .send()
                .await;
        }
        Ok(())
    }
}

// ---------------- Pushover ----------------
// Docs: https://pushover.net/api
#[derive(Debug, Clone, serde::Deserialize)]
struct PushoverCfg {
    token: String,
    user: String,
}

struct Pushover;
#[async_trait]
impl Integration for Pushover {
    fn kind(&self) -> &'static str {
        "pushover"
    }
    async fn on_new_entries(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        _feed: &captura_storage::entity::feed::Model,
        entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        let cfg: PushoverCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
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
            let mut body = e.title.clone().unwrap_or_default();
            if let Some(u) = &e.url {
                body.push('\n');
                body.push_str(u);
            }
            let _ = ctx
                .http
                .post("https://api.pushover.net/1/messages.json")
                .form(&serde_json::json!({
                    "token": cfg.token,
                    "user": cfg.user,
                    "message": body,
                }))
                .send()
                .await;
        }
        Ok(())
    }
}

// ---------------- Matrix ----------------
// Minimal send message: POST /_matrix/client/v3/rooms/{roomId}/send/m.room.message?access_token=...
#[derive(Debug, Clone, serde::Deserialize)]
struct MatrixCfg {
    homeserver: String, // e.g. https://matrix-client.matrix.org
    access_token: String,
    room_id: String, // encoded room id or alias
}

struct Matrix;
#[async_trait]
impl Integration for Matrix {
    fn kind(&self) -> &'static str {
        "matrix"
    }
    async fn on_new_entries(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        _feed: &captura_storage::entity::feed::Model,
        entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        let cfg: MatrixCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
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
            let url = format!(
                "{}/_matrix/client/v3/rooms/{}/send/m.room.message",
                cfg.homeserver.trim_end_matches('/'),
                urlencoding::encode(&cfg.room_id)
            );
            let _ = ctx
                .http
                .post(url)
                .bearer_auth(&cfg.access_token)
                .json(&serde_json::json!({"msgtype":"m.text","body": text}))
                .send()
                .await;
        }
        Ok(())
    }
}

struct Slack;
#[async_trait]
impl Integration for Slack {
    fn kind(&self) -> &'static str {
        "slack"
    }
    async fn on_new_entries(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        _feed: &captura_storage::entity::feed::Model,
        entry_ids: &[i64],
    ) -> anyhow::Result<()> {
        let cfg: SlackCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
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
                "*{}*\n{}",
                e.title.clone().unwrap_or_default(),
                e.url.clone().unwrap_or_default()
            );
            let _ = ctx
                .http
                .post(&cfg.incoming_webhook_url)
                .json(&serde_json::json!({"text": text}))
                .send()
                .await;
        }
        Ok(())
    }
    async fn on_save_entry(
        &self,
        ctx: &IntegrationCtx<'_>,
        user_id: UserId,
        entry: &captura_storage::entity::entry::Model,
    ) -> anyhow::Result<()> {
        let cfg: SlackCfg = ctx_cfg(ctx.db, user_id, self.kind()).await?;
        let text = format!(
            "Saved: *{}*\n{}",
            entry.title.clone().unwrap_or_default(),
            entry.url.clone().unwrap_or_default()
        );
        let _ = ctx
            .http
            .post(&cfg.incoming_webhook_url)
            .json(&serde_json::json!({"text": text}))
            .send()
            .await;
        Ok(())
    }
}
