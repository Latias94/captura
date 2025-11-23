use axum::{body::Body, http::Request};
use captura_api::{AppState, build_router};
use captura_storage::entity::{category, feed, smart_view};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

#[tokio::test]
async fn api_v1_export_full_contains_views_and_smart_views() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "export_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Create a category with a non-default view.
    let cat = category::ActiveModel {
        user_id: Set(user_id),
        name: Set("Pictures".to_string()),
        view: Set(Some("pictures".to_string())),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Create a feed in that category.
    let f = feed::ActiveModel {
        user_id: Set(user_id),
        category_id: Set(Some(cat.id)),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("PicsFeed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/pics.xml".into()),
        view: Set(Some("pictures".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Create a smart view.
    let _sv = smart_view::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("Unread Pics".into()),
        view: Set("pictures".into()),
        filters_json: Set(Some(serde_json::json!({
            "feed_ids": [f.id],
            "status": "unread"
        }))),
        sort_by: Set(Some("published_at".into())),
        sort_order: Set(Some("desc".into())),
        pinned: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Call /api/v1/export/full.
    let req = Request::get("/api/v1/export/full")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "export_full failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // Basic shape assertions.
    assert_eq!(v["version"], serde_json::json!("1"));
    assert!(v.get("exported_at").is_some(), "exported_at missing");

    // Category view should be exported as pictures.
    let cats = v["categories"].as_array().expect("categories array");
    assert!(
        cats.iter()
            .any(|c| c["id"] == cat.id && c["view"] == serde_json::json!("pictures")),
        "category view should be pictures in export"
    );

    // Feed with pictures view should appear with view='pictures'.
    let feeds = v["feeds"].as_array().expect("feeds array");
    assert!(
        feeds
            .iter()
            .any(|fobj| fobj["id"] == f.id && fobj["view"] == "pictures"),
        "feed view should be pictures in export"
    );

    // Smart view should be present with view='pictures'.
    let svs = v["smart_views"].as_array().expect("smart_views array");
    assert!(
        svs.iter().any(|s| s["view"] == "pictures"),
        "smart view with pictures view should be exported"
    );
}
