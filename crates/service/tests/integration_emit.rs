use captura_common::Result;
use captura_storage::entity::{entry, feed, user};
use captura_types::EntryView;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};

/// Smoke test: `integration::emit_new_entries` should not panic or fail
/// even when there is no integration configuration present for the user.
#[tokio::test]
async fn integration_emit_new_entries_handles_missing_config() -> Result<()> {
    let db = captura_testkit::setup_db().await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed user.
    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("integ_new_entries_user".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Seed a simple feed for this user.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("integration feed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/integration.xml".into()),
        favicon_id: Set(None),
        rule_id: Set(None),
        rule_params_json: Set(None),
        user_agent: Set(None),
        username: Set(None),
        password: Set(None),
        headers_json: Set(None),
        cookies: Set(None),
        proxy_url: Set(None),
        fetch_via_proxy: Set(false),
        disable_http2: Set(false),
        allow_invalid_certs: Set(false),
        request_timeout_ms: Set(None),
        checked_at: Set(None),
        next_run_at: Set(None),
        etag: Set(None),
        last_modified: Set(None),
        last_status: Set(None),
        error_count: Set(0),
        last_error_message: Set(None),
        disabled: Set(false),
        view: Set(Some(EntryView::Articles.to_db())),
        scraper_rules: Set(None),
        rewrite_rules: Set(None),
        blocklist_rules: Set(None),
        keeplist_rules: Set(None),
        url_rewrite_rules: Set(None),
        block_filter_entry_rules: Set(None),
        keep_filter_entry_rules: Set(None),
        integrations_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Seed one entry id.
    let e = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("guid-integ-new".into())),
        url: Set(Some("https://example.com/entry".into())),
        title: Set(Some("integration entry".into())),
        summary: Set(None),
        content_html: Set(Some("<p>body</p>".into())),
        author: Set(None),
        published_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        hash: Set(None),
        is_read: Set(false),
        is_starred: Set(false),
        extras_json: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // No `integration` rows exist for this user; emit_new_entries should
    // degrade gracefully and not panic.
    captura_service::integration::emit_new_entries(&db, u.id, &f, &[e.id]).await;

    Ok(())
}

/// Smoke test: `integration::emit_save_entry` should likewise tolerate
/// missing integration configuration.
#[tokio::test]
async fn integration_emit_save_entry_handles_missing_config() -> Result<()> {
    let db = captura_testkit::setup_db().await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("integ_save_entry_user".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("integration feed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/integration.xml".into()),
        favicon_id: Set(None),
        rule_id: Set(None),
        rule_params_json: Set(None),
        user_agent: Set(None),
        username: Set(None),
        password: Set(None),
        headers_json: Set(None),
        cookies: Set(None),
        proxy_url: Set(None),
        fetch_via_proxy: Set(false),
        disable_http2: Set(false),
        allow_invalid_certs: Set(false),
        request_timeout_ms: Set(None),
        checked_at: Set(None),
        next_run_at: Set(None),
        etag: Set(None),
        last_modified: Set(None),
        last_status: Set(None),
        error_count: Set(0),
        last_error_message: Set(None),
        disabled: Set(false),
        view: Set(Some(EntryView::Articles.to_db())),
        scraper_rules: Set(None),
        rewrite_rules: Set(None),
        blocklist_rules: Set(None),
        keeplist_rules: Set(None),
        url_rewrite_rules: Set(None),
        block_filter_entry_rules: Set(None),
        keep_filter_entry_rules: Set(None),
        integrations_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let e = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("guid-integ-save".into())),
        url: Set(Some("https://example.com/save".into())),
        title: Set(Some("save entry".into())),
        summary: Set(None),
        content_html: Set(None),
        author: Set(None),
        published_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        hash: Set(None),
        is_read: Set(false),
        is_starred: Set(false),
        extras_json: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    captura_service::integration::emit_save_entry(&db, u.id, &e).await;

    Ok(())
}
