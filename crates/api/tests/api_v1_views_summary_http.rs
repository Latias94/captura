use axum::{body::Body, http::Request};
use captura_api::{AppState, build_router};
use captura_storage::entity::{entry, feed};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

#[tokio::test]
async fn api_v1_views_summary_counts_feeds_and_unreads() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "summary_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Feed A: view=articles, 2 unread entries
    let f_articles = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("A".into())),
        site_url: Set(Some("https://a".into())),
        feed_url: Set("https://a/rss".into()),
        view: Set(Some("articles".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    for i in 0..2 {
        let am = entry::ActiveModel {
            feed_id: Set(f_articles.id),
            guid: Set(Some(format!("ga{}", i))),
            url: Set(Some(format!("https://a/{}", i))),
            title: Set(Some(format!("ta{}", i))),
            is_read: Set(false),
            is_starred: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let _ = am.insert(&db).await.unwrap();
    }

    // Feed B: view=pictures, 1 unread entry + 1 read entry
    let f_pics = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("P".into())),
        site_url: Set(Some("https://p".into())),
        feed_url: Set("https://p/rss".into()),
        view: Set(Some("pictures".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let e_unread = entry::ActiveModel {
        feed_id: Set(f_pics.id),
        guid: Set(Some("gp0".into())),
        url: Set(Some("https://p/0".into())),
        title: Set(Some("tp0".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let _ = e_unread.insert(&db).await.unwrap();

    let e_read = entry::ActiveModel {
        feed_id: Set(f_pics.id),
        guid: Set(Some("gp1".into())),
        url: Set(Some("https://p/1".into())),
        title: Set(Some("tp1".into())),
        is_read: Set(true),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let _ = e_read.insert(&db).await.unwrap();

    // Call /api/v1/views/summary.
    let req = Request::get("/api/v1/views/summary")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "views/summary failed: {}",
        resp.status()
    );

    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();

    // Find summaries for articles and pictures.
    let mut articles = None;
    let mut pictures = None;
    for item in &arr {
        match item["view"].as_str() {
            Some("articles") => articles = Some(item.clone()),
            Some("pictures") => pictures = Some(item.clone()),
            _ => {}
        }
    }

    let articles = articles.expect("articles summary");
    assert_eq!(articles["feed_count"], 1);
    assert_eq!(articles["unread_count"], 2);

    let pictures = pictures.expect("pictures summary");
    assert_eq!(pictures["feed_count"], 1);
    assert_eq!(pictures["unread_count"], 1);
}
