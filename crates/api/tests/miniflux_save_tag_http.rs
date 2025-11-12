use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use captura_storage::entity::{entry, feed};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use tower::ServiceExt;

#[tokio::test]
async fn miniflux_save_and_tags_flow() {
    let db = captura_testkit::setup_db().await;
    let (uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    // seed feed + entry
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

    let app = miniflux_service_with_state(db.clone());

    // save entry
    let req = Request::post(format!("/entries/{}/save", e.id))
        .header("X-Auth-Token", token.clone())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    // verify extras_json has saved=true
    let saved = captura_storage::entity::entry::Entity::find()
        .filter(captura_storage::entity::entry::Column::Id.eq(e.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let ok = saved
        .extras_json
        .as_ref()
        .and_then(|j| j.as_object())
        .and_then(|m| m.get("saved"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(ok);

    // add tags
    let req = Request::post(format!("/entries/{}/tags", e.id))
        .header("X-Auth-Token", &token)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"tags":["x","y"]}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());

    // get entry -> tags contains x,y
    let req = Request::get(format!("/entries/{}", e.id))
        .header("X-Auth-Token", &token)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let tags = v
        .get("tags")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let names: Vec<String> = tags
        .into_iter()
        .filter_map(|t| t.as_str().map(|s| s.to_string()))
        .collect();
    assert!(names.contains(&"x".to_string()) && names.contains(&"y".to_string()));
}
