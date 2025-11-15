use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use tower::ServiceExt;

/// Verify `/v1/me` exposes user preference fields consistent with those written.
#[tokio::test]
async fn miniflux_me_exposes_core_prefs() {
    let db = captura_testkit::setup_db().await;
    let (uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    // Promote the test user to admin so that `/v1/users/{id}` can update prefs.
    {
        use captura_storage::entity::user;
        use captura_storage::entity::user::Entity as User;
        use sea_orm::{ActiveModelTrait, EntityTrait, Set};
        let u = User::find_by_id(uid).one(&db).await.unwrap().unwrap();
        let mut am: user::ActiveModel = u.into();
        am.role = Set(captura_storage::entity::user::UserRole::Admin);
        let _ = am.update(&db).await.unwrap();
    }
    let app = miniflux_service_with_state(db.clone());

    // 1) Call `/v1/users/{id}` to update several preferences.
    let prefs = serde_json::json!({
        "theme": "dark_serif",
        "language": "zh_CN",
        "timezone": "Asia/Shanghai",
        "entry_sorting_direction": "asc",
        "entries_per_page": 42,
        "keyboard_shortcuts": false,
        "show_reading_time": true,
        "entry_swipe": false,
        "stylesheet": "body{background:#000;}",
        "custom_js": "window.__mf_custom_js = true;",
        "external_font_hosts": "fonts.gstatic.com fonts.googleapis.com",
        "always_open_external_links": true,
        "open_external_links_in_new_tab": true,
        "mark_read_on_view": true
    });
    let req = Request::put(format!("/users/{}", uid))
        .header("X-Auth-Token", token.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(prefs.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "update prefs failed: {}",
        resp.status()
    );

    // 2) Call `/v1/me` and assert the returned fields.
    let req = Request::get("/me")
        .header("X-Auth-Token", token.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(v.get("theme").and_then(|x| x.as_str()), Some("dark_serif"));
    assert_eq!(v.get("language").and_then(|x| x.as_str()), Some("zh_CN"));
    assert_eq!(
        v.get("timezone").and_then(|x| x.as_str()),
        Some("Asia/Shanghai")
    );
    assert_eq!(
        v.get("entry_sorting_direction").and_then(|x| x.as_str()),
        Some("asc")
    );
    assert_eq!(v.get("entries_per_page").and_then(|x| x.as_i64()), Some(42));
    assert_eq!(
        v.get("keyboard_shortcuts").and_then(|x| x.as_bool()),
        Some(false)
    );
    assert_eq!(
        v.get("show_reading_time").and_then(|x| x.as_bool()),
        Some(true)
    );
    assert_eq!(v.get("entry_swipe").and_then(|x| x.as_bool()), Some(false));
    assert_eq!(
        v.get("stylesheet").and_then(|x| x.as_str()),
        Some("body{background:#000;}")
    );
    assert_eq!(
        v.get("custom_js").and_then(|x| x.as_str()),
        Some("window.__mf_custom_js = true;")
    );
    assert_eq!(
        v.get("external_font_hosts").and_then(|x| x.as_str()),
        Some("fonts.gstatic.com fonts.googleapis.com")
    );
    assert_eq!(
        v.get("always_open_external_links")
            .and_then(|x| x.as_bool()),
        Some(true)
    );
    assert_eq!(
        v.get("open_external_links_in_new_tab")
            .and_then(|x| x.as_bool()),
        Some(true)
    );
    assert_eq!(
        v.get("mark_read_on_view").and_then(|x| x.as_bool()),
        Some(true)
    );
}
