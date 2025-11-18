use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::{entry, entry_label, feed, label};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Ensure `/api/v1/entries/{id}` returns tags for the current user.
#[tokio::test]
async fn api_v1_entry_includes_tags_for_user() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "entry_tags_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a feed owned by this user.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("tags feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/tags.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Seed an entry.
    let e = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("g-tags".into())),
        url: Set(Some("https://example.com/1".into())),
        title: Set(Some("hello tags".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Seed two labels and attach to entry via entry_label.
    let l1 = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("x".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let l2 = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("y".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e.id),
        label_id: Set(l1.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e.id),
        label_id: Set(l2.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Call /api/v1/entries/{id} and ensure tags are present.
    let req = Request::get(format!("/api/v1/entries/{}", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entry GET failed: {}",
        resp.status()
    );
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

/// Ensure `/api/v1/entries/{id}/tags` add/remove flow works end-to-end.
#[tokio::test]
async fn api_v1_add_and_remove_tags_flow() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "entry_tags_flow_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a feed and entry owned by this user.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("tags flow feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/tags-flow.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let e = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("g-tags-flow".into())),
        url: Set(Some("https://example.com/flow".into())),
        title: Set(Some("tags flow entry".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Add tags via API (with duplicates/whitespace to exercise normalization).
    let req = Request::post(format!("/api/v1/entries/{}/tags", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"tags":["x","y","x","  "]}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "add tags failed: {}",
        resp.status()
    );

    // Verify tags x,y are present on entry.
    let req = Request::get(format!("/api/v1/entries/{}", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
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

    // Remove tag "x" via API.
    let req = Request::delete(format!("/api/v1/entries/{}/tags", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({"tags":["x"]}).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "remove tags failed: {}",
        resp.status()
    );

    // Verify only "y" remains.
    let req = Request::get(format!("/api/v1/entries/{}", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
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
    assert!(!names.contains(&"x".to_string()) && names.contains(&"y".to_string()));
}

/// Ensure `/api/v1/entries` supports `include_tags=true` for list endpoints.
#[tokio::test]
async fn api_v1_entries_list_can_include_tags() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) =
        captura_testkit::seed_user_and_token(&db, "entries_include_tags_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a feed and two tagged entries.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("list tags feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/list-tags.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let e1 = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("g-list-1".into())),
        url: Set(Some("https://example.com/lt/1".into())),
        title: Set(Some("entry one".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let e2 = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("g-list-2".into())),
        url: Set(Some("https://example.com/lt/2".into())),
        title: Set(Some("entry two".into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Two labels and attach: e1 -> "a"; e2 -> "b".
    let l_a = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("a".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let l_b = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("b".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e1.id),
        label_id: Set(l_a.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e2.id),
        label_id: Set(l_b.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Call /api/v1/entries without include_tags: tags should be null/absent.
    let req = Request::get(format!(
        "/api/v1/entries?feed_id={}&limit=10&offset=0&sort_by=id&order=asc",
        f.id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&bytes);
        panic!("entries list failed: {} body: {}", status, body_str);
    }
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 2);
    assert!(arr.iter().all(|e| e.get("tags").is_none()));

    // Call /api/v1/entries with include_tags=true: tags should be populated.
    let req = Request::get(format!(
        "/api/v1/entries?feed_id={}&limit=10&offset=0&sort_by=id&order=asc&include_tags=true",
        f.id
    ))
    .header(axum::http::header::AUTHORIZATION, auth.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "entries list (include_tags) failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 2);
    let tags1 = arr[0]
        .get("tags")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let tags2 = arr[1]
        .get("tags")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let t1: Vec<String> = tags1
        .into_iter()
        .filter_map(|t| t.as_str().map(|s| s.to_string()))
        .collect();
    let t2: Vec<String> = tags2
        .into_iter()
        .filter_map(|t| t.as_str().map(|s| s.to_string()))
        .collect();
    assert!(t1.contains(&"a".to_string()) || t2.contains(&"a".to_string()));
    assert!(t1.contains(&"b".to_string()) || t2.contains(&"b".to_string()));
}
