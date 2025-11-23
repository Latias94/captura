use axum::{body::Body, http::Request};
use captura_api::{AppState, build_router};
use captura_storage::entity::entry;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use tower::ServiceExt;

#[tokio::test]
async fn api_v1_feed_inherits_category_view_and_filters_entries() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "view_user").await;
    let auth = format!("Bearer {}", token);

    // 1) Create a category with view = pictures.
    let req = Request::post("/api/v1/categories")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(
            serde_json::json!({
                "name": "pics",
                "view": "pictures"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "create_category failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let category_id = v["id"].as_i64().expect("category id");

    // 2) Create feeds:
    //    - f_pics: in category "pics", without explicit view (should inherit pictures).
    //    - f_articles: explicit view = articles.
    //    - f_default: no category, no view (treated as articles).
    let create_feed = |body: serde_json::Value| {
        let auth = auth.clone();
        let app = app.clone();
        async move {
            let req = Request::post("/api/v1/feeds")
                .header("content-type", "application/json")
                .header(axum::http::header::AUTHORIZATION, auth.as_str())
                .body(Body::from(body.to_string()))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert!(
                resp.status().is_success(),
                "create_feed failed: {}",
                resp.status()
            );
            let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            v["id"].as_i64().expect("feed id")
        }
    };

    let f_pics_id = create_feed(serde_json::json!({
        "feed_url": "https://example.com/pics.xml",
        "title": "Pics",
        "type": "rss",
        "category_id": category_id
    }))
    .await;

    let f_articles_id = create_feed(serde_json::json!({
        "feed_url": "https://example.com/articles.xml",
        "title": "Articles",
        "type": "rss",
        "view": "articles"
    }))
    .await;

    let f_default_id = create_feed(serde_json::json!({
        "feed_url": "https://example.com/default.xml",
        "title": "Default",
        "type": "rss"
    }))
    .await;

    // 3) Verify feed in category inherited view = pictures.
    let req = Request::get(format!("/api/v1/feeds/{f_pics_id}"))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "get_feed failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let feed_obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(feed_obj["view"], serde_json::json!("pictures"));

    // 4) Insert one unread entry for each feed.
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let insert_entry = |fid: i64, guid: &str| entry::ActiveModel {
        feed_id: Set(fid),
        guid: Set(Some(guid.to_string())),
        url: Set(Some(format!("https://example.com/{}", guid))),
        title: Set(Some(format!("E {}", guid))),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    insert_entry(f_pics_id, "g_pics")
        .insert(st.db())
        .await
        .unwrap();
    insert_entry(f_articles_id, "g_articles")
        .insert(st.db())
        .await
        .unwrap();
    insert_entry(f_default_id, "g_default")
        .insert(st.db())
        .await
        .unwrap();

    // 5) Query by view = pictures → only entries from f_pics.
    let req = Request::get("/api/v1/entries?view=pictures")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries?view=pictures failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1, "expected 1 entry for view=pictures");
    assert_eq!(arr[0]["feed_id"].as_i64(), Some(f_pics_id));

    // 6) Query by view = articles → entries from f_articles + feeds with no explicit view (treated as articles).
    let req = Request::get("/api/v1/entries?view=articles")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries?view=articles failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    let mut fids: Vec<i64> = arr
        .iter()
        .filter_map(|e| e.get("feed_id").and_then(|x| x.as_i64()))
        .collect();
    fids.sort_unstable();
    fids.dedup();
    assert!(
        fids.contains(&f_articles_id),
        "articles view should include explicit articles feed"
    );
    assert!(
        fids.contains(&f_default_id),
        "articles view should include feeds without explicit view"
    );
    // picture feed should not be included.
    assert!(
        !fids.contains(&f_pics_id),
        "pictures feed should not be included in view=articles"
    );
}

#[tokio::test]
async fn api_v1_mark_all_read_scoped_by_view() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "view_user2").await;
    let auth = format!("Bearer {}", token);

    // Create two feeds with different views.
    let create_feed = |view: &str, url_suffix: &str| {
        let view = view.to_string();
        let url_suffix = url_suffix.to_string();
        let auth = auth.clone();
        let app = app.clone();
        async move {
            let body = serde_json::json!({
                "feed_url": format!("https://example.com/{}.xml", url_suffix),
                "title": url_suffix,
                "type": "rss",
                "view": view
            });
            let req = Request::post("/api/v1/feeds")
                .header("content-type", "application/json")
                .header(axum::http::header::AUTHORIZATION, auth.as_str())
                .body(Body::from(body.to_string()))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert!(resp.status().is_success(), "create_feed failed");
            let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            v["id"].as_i64().expect("feed id")
        }
    };

    let f_pics_id = create_feed("pictures", "pics_mark").await;
    let f_articles_id = create_feed("articles", "articles_mark").await;

    // Insert one unread entry for each feed.
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    for (fid, guid) in &[
        (f_pics_id, "g_pics_mark"),
        (f_articles_id, "g_articles_mark"),
    ] {
        let am = entry::ActiveModel {
            feed_id: Set(*fid),
            guid: Set(Some((*guid).to_string())),
            url: Set(Some(format!("https://example.com/{}", guid))),
            title: Set(Some(format!("E {}", guid))),
            is_read: Set(false),
            is_starred: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        am.insert(st.db()).await.unwrap();
    }

    // Mark all read for view=pictures.
    let req = Request::post("/api/v1/entries/mark-all-read")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(
            serde_json::json!({
                "feed_id": null,
                "category_id": null,
                "view": "pictures"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "mark-all-read by view failed: {}",
        resp.status()
    );

    // Verify DB: entries for pictures feed are read; articles feed remain unread.
    let pics_unread_count: i64 = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f_pics_id))
        .filter(entry::Column::IsRead.eq(false))
        .count(st.db())
        .await
        .unwrap() as i64;
    assert_eq!(
        pics_unread_count, 0,
        "expected all entries for pictures feed to be marked read"
    );

    let articles_unread_count: i64 = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f_articles_id))
        .filter(entry::Column::IsRead.eq(false))
        .count(st.db())
        .await
        .unwrap() as i64;
    assert_eq!(
        articles_unread_count, 1,
        "entries for other views should remain unread"
    );
}

#[tokio::test]
async fn api_v1_smart_views_basic_flow() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "smart_user").await;
    let auth = format!("Bearer {}", token);

    // Create a feed via /api/v1/feeds.
    let req = Request::post("/api/v1/feeds")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(
            serde_json::json!({
                "feed_url": "https://example.com/smart.xml",
                "title": "Smart",
                "type": "rss"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "create_feed failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = v["id"].as_i64().expect("feed id");

    // Insert a single unread entry for this feed.
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let e = entry::ActiveModel {
        feed_id: Set(feed_id),
        guid: Set(Some("g_smart".into())),
        url: Set(Some("https://example.com/smart/1".into())),
        title: Set(Some("Smart Entry".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(st.db())
    .await
    .unwrap();

    // Create a smart view for unread entries of this feed.
    let req = Request::post("/api/v1/smart-views")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(
            serde_json::json!({
                "name": "Unread Smart",
                "view": "all",
                "filters": {
                    "feed_ids": [feed_id],
                    "status": "unread"
                },
                "pinned": true
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "create smart view failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let sv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let smart_id = sv["id"].as_i64().expect("smart_view id");

    // List smart views should include the created one.
    let req = Request::get("/api/v1/smart-views")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "list smart views failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(
        arr.iter()
            .any(|x| x.get("id").and_then(|v| v.as_i64()) == Some(smart_id)),
        "created smart view should appear in list"
    );

    // Fetch entries via smart view.
    let req = Request::get(format!("/api/v1/smart-views/{smart_id}/entries?limit=10"))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    if !status.is_success() {
        eprintln!(
            "smart view entries failed: {} body={}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }
    assert!(status.is_success(), "smart view entries failed: {}", status);
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1, "expected exactly 1 entry from smart view");
    let eid = entries[0]["id"].as_i64().expect("entry id");
    assert_eq!(eid, e.id, "smart view should return the seeded entry");
}
