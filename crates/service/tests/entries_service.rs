use captura_common::{Result, UserId};
use captura_service::entries;
use captura_storage::entity::{entry, entry_label, feed, label, user};
use captura_types::EntryView;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

/// Verify that set_entry_saved toggles extras_json and preserves core fields.
#[tokio::test]
async fn set_entry_saved_toggles_extras_json() -> Result<()> {
    let db = captura_testkit::setup_db().await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("saved_user".into()),
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
        title: Set(Some("saved feed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/saved.xml".into()),
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
        guid: Set(Some("guid-saved".into())),
        url: Set(Some("https://example.com/saved".into())),
        title: Set(Some("saved entry".into())),
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

    // Mark as saved.
    let updated = entries::set_entry_saved(&db, &e, true).await?;
    let extras = updated.extras_json.clone().unwrap();
    assert_eq!(extras.get("saved").and_then(|v| v.as_bool()), Some(true));

    // Clear saved flag.
    let updated2 = entries::set_entry_saved(&db, &updated, false).await?;
    assert!(updated2.extras_json.is_none());

    Ok(())
}

/// Verify that add/remove tag helpers create and delete relations as expected.
#[tokio::test]
async fn add_and_remove_tags_roundtrip() -> Result<()> {
    let db = captura_testkit::setup_db().await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("tags_user".into()),
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
        title: Set(Some("tags feed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/tags.xml".into()),
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
        guid: Set(Some("guid-tags".into())),
        url: Set(Some("https://example.com/tags_entry".into())),
        title: Set(Some("tags entry".into())),
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

    // Add tags; this should create labels and relations.
    entries::add_tags_to_entry(&db, UserId(u.id), &e, vec!["rust".into(), " news ".into()]).await?;

    let labels: Vec<label::Model> = label::Entity::find()
        .filter(label::Column::UserId.eq(u.id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(labels.len(), 2);

    let rels: Vec<entry_label::Model> = entry_label::Entity::find()
        .filter(entry_label::Column::EntryId.eq(e.id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(rels.len(), 2);

    // Remove one tag and ensure corresponding relation is removed.
    entries::remove_tags_from_entry(&db, UserId(u.id), &e, vec!["rust".into()]).await?;

    let remaining: Vec<entry_label::Model> = entry_label::Entity::find()
        .filter(entry_label::Column::EntryId.eq(e.id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);

    Ok(())
}
