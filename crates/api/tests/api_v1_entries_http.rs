use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::{entry, feed};
use captura_types::EntryView;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set};
use tower::ServiceExt;

/// HTTP-level coverage for `/api/v1/entries` search + id cursors.
///
/// This mirrors the service-layer test in `captura_service::query` but
/// exercises the full Axum stack (AuthUser, query mapping, router).
#[tokio::test]
async fn api_v1_entries_search_and_id_cursors_http() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) =
        captura_testkit::seed_user_and_token(&db, "entries_search_http").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a single feed owned by this user.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("search feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/search.xml".into()),
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

    // Insert three entries with different titles.
    let titles = ["hello world", "rust timeline", "another hello"];
    for t in titles.iter() {
        let am = entry::ActiveModel {
            id: Default::default(),
            feed_id: Set(f.id),
            guid: Set(Some(format!("guid-{}", t))),
            url: Set(Some(format!("https://example.com/{}", t.replace(' ', "_")))),
            title: Set(Some(t.to_string())),
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
        };
        am.insert(&db).await.unwrap();
    }

    // Fetch all entries to know their ids (ordered by id).
    let all_ids: Vec<i64> = entry::Entity::find()
        .order_by_asc(entry::Column::Id)
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(all_ids.len(), 3);

    // 1) Search for "hello" via `q=hello` should match titles containing "hello" (2 entries).
    let req = Request::get(format!(
        "/api/v1/entries?feed_id={}&q=hello&view=articles&sort_by=id&order=asc",
        f.id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries search failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 2, "expected 2 entries for q=hello");
    let titles_found: Vec<String> = arr
        .iter()
        .map(|e| e["title"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(titles_found.iter().all(|t| t.contains("hello")));

    // 2) The alias `search=hello` should behave the same as `q=hello`.
    let req_alias = Request::get(format!(
        "/api/v1/entries?feed_id={}&search=hello&view=articles&sort_by=id&order=asc",
        f.id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req_alias).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries search (alias) failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr_alias: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    let alias_ids: Vec<i64> = arr_alias
        .iter()
        .filter_map(|e| e.get("id").and_then(|v| v.as_i64()))
        .collect();
    let ids: Vec<i64> = arr
        .iter()
        .filter_map(|e| e.get("id").and_then(|v| v.as_i64()))
        .collect();
    assert_eq!(alias_ids.len(), ids.len());
    assert!(alias_ids.iter().all(|id| ids.contains(id)));

    // 3) before_id cursor should exclude entries with id >= before_id.
    let req_before = Request::get(format!(
        "/api/v1/entries?feed_id={}&q=hello&view=articles&sort_by=id&order=asc&before_id={}",
        f.id, all_ids[2]
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req_before).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries before_id failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr_before: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(arr_before
        .iter()
        .all(|e| e["id"].as_i64().unwrap_or(i64::MAX) < all_ids[2]));

    // 4) after_id cursor should exclude entries with id <= after_id.
    let req_after = Request::get(format!(
        "/api/v1/entries?feed_id={}&q=hello&view=articles&sort_by=id&order=asc&after_id={}",
        f.id, all_ids[0]
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req_after).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries after_id failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr_after: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(arr_after
        .iter()
        .all(|e| e["id"].as_i64().unwrap_or(i64::MIN) > all_ids[0]));
}

