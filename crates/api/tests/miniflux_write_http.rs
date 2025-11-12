use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use captura_storage::entity::{entry, feed};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

#[tokio::test]
async fn miniflux_toggle_star_flow() {
    let db = captura_testkit::setup_db().await;
    let (uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let f = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("mf".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/feed".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let e = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g".into())),
        url: Set(Some("https://example.com/1".into())),
        title: Set(Some("A".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let app = miniflux_service_with_state(db);

    // toggle star -> true
    let req = Request::put(format!("/entries/{}/star", e.id))
        .header("X-Auth-Token", token.clone())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());

    // get entry -> starred is true
    let req = Request::get(format!("/entries/{}", e.id))
        .header("X-Auth-Token", token)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v.get("starred").and_then(|x| x.as_bool()), Some(true));
}
