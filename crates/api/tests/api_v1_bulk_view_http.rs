use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::feed;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tower::ServiceExt;

#[tokio::test]
async fn api_v1_feeds_bulk_view_updates_multiple_feeds() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "bulk_view_user").await;
    let auth = format!("Bearer {}", token);

    // Create three feeds with different initial views.
    let create = |view: &str, suffix: &str| {
        let view = view.to_string();
        let suffix = suffix.to_string();
        let auth = auth.clone();
        let app = app.clone();
        async move {
            let body = serde_json::json!({
                "feed_url": format!("https://example.com/{}.xml", suffix),
                "title": suffix,
                "type": "rss",
                "view": view
            });
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

    let f_articles_id = create("articles", "bulk_articles").await;
    let f_pictures_id = create("pictures", "bulk_pictures").await;
    let f_videos_id = create("videos", "bulk_videos").await;

    // Bulk move two feeds to audios view.
    let req = Request::post("/api/v1/feeds/bulk-view")
        .header("content-type", "application/json")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::from(
            serde_json::json!({
                "feed_ids": [f_articles_id, f_pictures_id],
                "view": "audios"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "bulk-view failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v["updated"].as_u64(),
        Some(2),
        "expected 2 feeds to be updated"
    );

    // Verify DB: the two targeted feeds have view='audios', the third one keeps its original view.
    let f_articles = feed::Entity::find()
        .filter(feed::Column::Id.eq(f_articles_id))
        .one(st.db())
        .await
        .unwrap()
        .expect("articles feed");
    assert_eq!(f_articles.view.as_deref(), Some("audios"));

    let f_pictures = feed::Entity::find()
        .filter(feed::Column::Id.eq(f_pictures_id))
        .one(st.db())
        .await
        .unwrap()
        .expect("pictures feed");
    assert_eq!(f_pictures.view.as_deref(), Some("audios"));

    let f_videos = feed::Entity::find()
        .filter(feed::Column::Id.eq(f_videos_id))
        .one(st.db())
        .await
        .unwrap()
        .expect("videos feed");
    assert_eq!(f_videos.view.as_deref(), Some("videos"));
}
