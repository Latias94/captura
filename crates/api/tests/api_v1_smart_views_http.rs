use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::{entry, entry_label, feed, label};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Ensure SmartView combines view + feed_ids + status correctly.
///
/// Scenario:
/// - f_articles (view=articles) with one unread entry;
/// - f_pictures (view=pictures) with one unread and one read entry;
/// - SmartView: view=pictures, filters.feed_ids=[f_articles,f_pictures], filters.status="unread";
/// - Expected: only the unread entry from f_pictures is returned.
#[tokio::test]
async fn api_v1_smart_view_combines_view_feed_and_status() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "smart_view_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed two feeds owned by this user, with different views.
    let f_articles = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("articles feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/articles.xml".into()),
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
        view: Set(Some(captura_types::EntryView::Articles.to_db())),
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

    let f_pictures = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("pictures feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/pictures.xml".into()),
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
        view: Set(Some(captura_types::EntryView::Pictures.to_db())),
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

    // Entries: unread in both feeds, plus one read in pictures.
    let e_articles_unread = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f_articles.id),
        guid: Set(Some("guid-articles-unread".into())),
        url: Set(Some("https://example.com/articles/1".into())),
        title: Set(Some("Articles unread".into())),
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
    let e_pics_unread = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f_pictures.id),
        guid: Set(Some("guid-pics-unread".into())),
        url: Set(Some("https://example.com/pictures/1".into())),
        title: Set(Some("Pictures unread".into())),
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
    let _e_pics_read = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f_pictures.id),
        guid: Set(Some("guid-pics-read".into())),
        url: Set(Some("https://example.com/pictures/2".into())),
        title: Set(Some("Pictures read".into())),
        summary: Set(None),
        content_html: Set(None),
        author: Set(None),
        published_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        hash: Set(None),
        is_read: Set(true),
        is_starred: Set(false),
        extras_json: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // Create SmartView via API: view=pictures, filters on both feeds + status=unread.
    let body = serde_json::json!({
        "name": "pics-only-unread",
        "view": "pictures",
        "filters": {
            "feed_ids": [f_articles.id, f_pictures.id],
            "status": "unread"
        },
        "sort_by": "published_at",
        "sort_order": "asc",
        "pinned": false
    });
    let req = Request::post("/api/v1/smart-views")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "create_smart_view failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let sv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sv_id = sv["id"].as_i64().expect("smart_view id");

    // Query entries for this SmartView.
    let req = Request::get(format!(
        "/api/v1/smart-views/{}/entries?limit=10&offset=0&sort_by=published_at&order=asc",
        sv_id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "smart_view entries failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        arr.len(),
        1,
        "expected only one unread entry from pictures feed"
    );
    let eid = arr[0]["id"].as_i64().unwrap();
    let fid = arr[0]["feed_id"].as_i64().unwrap();
    assert_eq!(eid, e_pics_unread.id);
    assert_eq!(fid, f_pictures.id);
    // Ensure the articles-unread entry is not part of this pictures SmartView.
    assert_ne!(eid, e_articles_unread.id);
}

/// Ensure SmartView combining view + label_ids + status behaves as expected.
///
/// Scenario:
/// - e_rust (articles view) tagged "rust", unread;
/// - e_news (pictures view) tagged "news", unread;
/// SmartViews:
/// - sv_articles_rust: view=articles + label_ids=[rust] + status=unread → only e_rust;
/// - sv_pictures_both: view=pictures + label_ids=[rust,news] + status=unread → only e_news.
#[tokio::test]
async fn api_v1_smart_view_combines_view_labels_and_status() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) =
        captura_testkit::seed_user_and_token(&db, "smart_view_labels_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Feeds: one articles, one pictures.
    let f_articles = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("articles feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/articles2.xml".into()),
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
        view: Set(Some(captura_types::EntryView::Articles.to_db())),
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
    let f_pictures = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("pictures feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/pictures2.xml".into()),
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
        view: Set(Some(captura_types::EntryView::Pictures.to_db())),
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

    // Entries: both unread.
    let e_rust = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f_articles.id),
        guid: Set(Some("guid-rust".into())),
        url: Set(Some("https://example.com/rust".into())),
        title: Set(Some("Rust entry".into())),
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
    let e_news = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f_pictures.id),
        guid: Set(Some("guid-news".into())),
        url: Set(Some("https://example.com/news".into())),
        title: Set(Some("News entry".into())),
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

    // Labels and relations.
    let l_rust = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("rust".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let l_news = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("news".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e_rust.id),
        label_id: Set(l_rust.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e_news.id),
        label_id: Set(l_news.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // SmartView 1: view=articles + label_ids=[rust] + status=unread.
    let body1 = serde_json::json!({
        "name": "articles-rust-unread",
        "view": "articles",
        "filters": {
            "label_ids": [l_rust.id],
            "status": "unread"
        },
        "sort_by": "published_at",
        "sort_order": "asc",
        "pinned": false
    });
    let req = Request::post("/api/v1/smart-views")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(body1.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "create_smart_view 1 failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let sv1: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sv1_id = sv1["id"].as_i64().expect("sv1 id");

    let req = Request::get(format!(
        "/api/v1/smart-views/{}/entries?limit=10&offset=0&sort_by=published_at&order=asc",
        sv1_id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    if !resp.status().is_success() {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        panic!(
            "sv1 entries failed with status {} and body: {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1, "sv1 should only see rust-tagged article");
    let eid = arr[0]["id"].as_i64().unwrap();
    let fid = arr[0]["feed_id"].as_i64().unwrap();
    assert_eq!(eid, e_rust.id);
    assert_eq!(fid, f_articles.id);

    // SmartView 2: view=pictures + label_ids=[rust,news] + status=unread.
    let body2 = serde_json::json!({
        "name": "pictures-rust-news-unread",
        "view": "pictures",
        "filters": {
            "label_ids": [l_rust.id, l_news.id],
            "status": "unread"
        },
        "sort_by": "published_at",
        "sort_order": "asc",
        "pinned": false
    });
    let req = Request::post("/api/v1/smart-views")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(body2.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "create_smart_view 2 failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let sv2: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sv2_id = sv2["id"].as_i64().expect("sv2 id");

    let req = Request::get(format!(
        "/api/v1/smart-views/{}/entries?limit=10&offset=0&sort_by=published_at&order=asc",
        sv2_id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "sv2 entries failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        arr.len(),
        1,
        "sv2 should only see pictures entry with one of the labels"
    );
    let eid = arr[0]["id"].as_i64().unwrap();
    let fid = arr[0]["feed_id"].as_i64().unwrap();
    assert_eq!(eid, e_news.id);
    assert_eq!(fid, f_pictures.id);
}
