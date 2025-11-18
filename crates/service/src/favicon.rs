//! Favicon refresh logic shared by scheduler and API layers.
//!
//! The scheduler enqueues `JobType::Favicon` jobs; this module owns the
//! actual HTTP fetch + persistence so that the business logic lives in
//! the service layer instead of the scheduler crate.

use captura_common::{Error, Result};
use captura_storage::entity::{favicon as fv, feed};
use chrono::{FixedOffset, Utc};
use reqwest::Url;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};

/// Refresh favicon for a feed by id.
pub async fn refresh_for_feed_id(db: &DatabaseConnection, feed_id: i64) -> Result<()> {
    let Some(f) = feed::Entity::find_by_id(feed_id)
        .one(db)
        .await
        .map_err(|e| Error::Storage(e.to_string()))?
    else {
        return Err(Error::NotFound("feed missing".into()));
    };
    refresh_for_feed(db, &f).await
}

/// Refresh favicon for a given feed model.
pub async fn refresh_for_feed(db: &DatabaseConnection, f: &feed::Model) -> Result<()> {
    // Derive site URL from feed.site_url or fall back to feed_url.
    let site = f.site_url.clone().unwrap_or(f.feed_url.clone());
    let mut base =
        Url::parse(&site).map_err(|e| Error::Config(format!("invalid site url: {e}")))?;
    base.set_path("/favicon.ico");
    base.set_query(None);
    base.set_fragment(None);

    // Use the shared HTTP client so UA/timeout/proxy behaviour is consistent.
    let cli = crate::http_client_basic()?;
    let res = cli
        .get(base.as_str())
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    if !res.status().is_success() {
        return Err(Error::NotFound(format!("status {}", res.status())));
    }
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = res
        .bytes()
        .await
        .map_err(|e| Error::Network(e.to_string()))?
        .to_vec();

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = fv::ActiveModel {
        feed_id: Set(Some(f.id)),
        url: Set(Some(base.to_string())),
        mime: Set(mime),
        data: Set(Some(bytes)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let fav = am
        .insert(db)
        .await
        .map_err(|e| Error::Storage(e.to_string()))?;

    let mut fm: feed::ActiveModel = f.clone().into();
    fm.favicon_id = Set(Some(fav.id));
    let _ = fm
        .update(db)
        .await
        .map_err(|e| Error::Storage(e.to_string()))?;

    Ok(())
}

