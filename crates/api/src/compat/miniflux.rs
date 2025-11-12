use crate::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

mod apikeys;
mod categories;
mod discover;
mod enclosures;
mod entries;
mod error;
mod export_import;
mod feeds;
mod icons;
mod integrations;
mod tags;
mod types;
mod users;
mod version;

// MfError/MfResult moved to error.rs

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(users::me))
        .route("/users", get(users::list).post(users::create))
        .route(
            "/users/{id}",
            get(users::get).put(users::update).delete(users::delete),
        )
        .route(
            "/categories",
            get(categories::list).post(categories::create),
        )
        .route("/categories/counters", get(categories::counters))
        .route(
            "/categories/{id}/mark-all-as-read",
            put(categories::mark_all_read),
        )
        .route("/categories/{id}/feeds", get(categories::feeds))
        .route("/categories/{id}/refresh", put(categories::refresh))
        .route(
            "/categories/{id}",
            put(categories::update).delete(categories::delete),
        )
        .route("/feeds/counters", get(feeds::counters))
        .route("/version", get(version::version))
        .route("/feeds", get(feeds::list).post(feeds::create))
        .route("/feeds/refresh", put(feeds::refresh_all))
        .route(
            "/feeds/{id}",
            get(feeds::get).put(feeds::update).delete(feeds::delete),
        )
        .route("/feeds/{id}/mark-all-read", post(feeds::mark_all_read))
        .route("/feeds/{id}/refresh", post(feeds::refresh_one))
        .route("/feeds/{id}/icon", get(icons::icon_by_feed))
        .route("/entries", get(entries::list).put(entries::update_bulk))
        .route("/entries/{id}", get(entries::get).put(entries::update))
        .route("/entries/{id}/star", put(entries::toggle_star))
        .route("/entries/{id}/bookmark", put(entries::toggle_star))
        .route("/entries/{id}/save", post(entries::save))
        .route(
            "/entries/{id}/tags",
            post(entries::add_tags).delete(entries::remove_tags),
        )
        .route("/entries/{id}/fetch-content", get(entries::fetch_content))
        .route("/feeds/{id}/entries", get(entries::feed_entries))
        .route("/categories/{id}/entries", get(categories::entries))
        .route("/flush-history", put(entries::flush_history))
        .route("/users/{id}/mark-all-as-read", put(users::mark_all_read))
        .route("/api-keys", get(apikeys::list).post(apikeys::create))
        .route("/api-keys/{id}", delete(apikeys::delete))
        .route("/icons/{id}", get(icons::icon_by_id))
        .route(
            "/enclosures/{id}",
            get(enclosures::get).put(enclosures::update),
        )
        .route("/export", get(export_import::export))
        .route("/import", post(export_import::import))
        .route("/discover", post(discover::discover))
        .route("/integrations/status", get(integrations::status))
        .route("/tags", get(tags::list).post(tags::create))
        .route(
            "/tags/{name}",
            get(tags::get).delete(tags::delete).put(tags::rename),
        )
}

// user/admin handlers moved to users.rs

// user create/update DTO moved to users.rs

// MfCategoryDto moved to types.rs

// category handlers moved to categories.rs

// MfFeedDto moved to types.rs

// map_feed moved to types.rs

// 单个订阅源刷新已迁移至 feeds::refresh_one

// feed refresh_all moved to feeds.rs

// entries list moved to entries.rs

// entries get/list/update moved to entries.rs

// category counters moved to categories.rs

// entries bulk/update/star moved to entries.rs

// moved implementations: flush_history -> entries.rs; enclosures -> enclosures.rs; user mark-all-read -> users.rs

// strip_html moved to util.rs

// category mark-all-read moved to categories.rs

// export/import moved to export_import.rs

// discover moved to discover.rs

// integrations status moved to integrations.rs

// integrations moved to integrations.rs

// tag DTO moved to tags.rs

// tags moved to tags.rs

// tag create req moved to tags.rs

// tags moved to tags.rs

// tags moved to tags.rs

// tag rename req moved to tags.rs

// tags moved to tags.rs

// -----------------------------
// HTTP 级最小集成测试（sqlite::memory + migration）
// -----------------------------
#[cfg(any())]
mod it {
    use super::*;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::util::ServiceExt;

    async fn setup_db() -> sea_orm::DatabaseConnection {
        captura_testkit::setup_db().await
    }

    async fn seed_user_and_token(db: &sea_orm::DatabaseConnection) -> String {
        let (_uid, token) = captura_testkit::seed_user_and_token(db, "u").await;
        token
    }

    async fn json_body(resp: axum::response::Response) -> serde_json::Value {
        let status = resp.status();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        if status == StatusCode::NO_CONTENT {
            return serde_json::Value::Null;
        }
        serde_json::from_slice(&body).unwrap_or_else(|_| serde_json::Value::Null)
    }

    #[tokio::test]
    async fn me_and_tags_flow() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        // feed + entry
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
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
            title: Set(Some("hello".into())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });

        // GET /v1/me
        let resp = app
            .clone()
            .oneshot(
                Request::get("/me")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // POST /v1/entries/:id/tags
        let resp = app
            .clone()
            .oneshot(
                Request::post(format!("/entries/{}/tags", e.id))
                    .header("X-Auth-Token", token.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        json!({"tags":["t1","t2"]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // GET /v1/tags
        let resp = app
            .clone()
            .oneshot(
                Request::get("/tags")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.is_array());

        // PUT /v1/tags/:name
        let resp = app
            .clone()
            .oneshot(
                Request::put("/tags/t1")
                    .header("X-Auth-Token", token.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(json!({"title":"t3"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // DELETE /v1/tags/:name
        let resp = app
            .oneshot(
                Request::delete("/tags/t2")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn entries_filters_basic() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let e1 = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g1".into())),
            title: Set(Some("hello".into())),
            is_read: Set(false),
            is_starred: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _e2 = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g2".into())),
            title: Set(Some("world".into())),
            is_read: Set(true),
            is_starred: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });
        // unread only
        let resp = app
            .clone()
            .oneshot(
                Request::get("/entries?status=unread&limit=10")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let j = json_body(resp).await;
        if status != StatusCode::OK {
            panic!("/v1/entries returned {}: {}", status, j);
        }
        assert_eq!(j["total"].as_i64().unwrap_or(-1), 1);
        assert_eq!(j["entries"][0]["id"].as_i64().unwrap_or(-1), e1.id);
    }

    #[tokio::test]
    async fn icon_binary_and_bookmark_alias() {
        use captura_storage::entity::favicon as fv;
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        // feed + entry + favicon
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
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
            title: Set(Some("x".into())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let fav = fv::ActiveModel {
            feed_id: Set(Some(f.id)),
            url: Set(Some("https://example.com/favicon.ico".into())),
            mime: Set(Some("image/x-icon".into())),
            data: Set(Some(vec![7, 8, 9])),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let mut fm: feed::ActiveModel = f.into();
        fm.favicon_id = Set(Some(fav.id));
        let f = fm.update(&db).await.unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });

        // GET /v1/feeds/:id/icon binary
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/feeds/{}/icon", f.id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("image/"));
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.to_vec(), vec![7, 8, 9]);

        // PUT /v1/entries/:id/bookmark alias
        let resp = app
            .clone()
            .oneshot(
                Request::put(format!("/entries/{}/bookmark", e.id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // GET /v1/entries/:id and check starred
        let resp = app
            .oneshot(
                Request::get(format!("/entries/{}", e.id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert_eq!(j["id"].as_i64().unwrap_or(-1), e.id);
        assert!(j["starred"].as_bool().unwrap_or(false));
    }

    #[tokio::test]
    async fn feeds_counters_basic() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        // two feeds
        let f1 = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("f1".into())),
            site_url: Set(Some("https://a".into())),
            feed_url: Set("https://a/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let f2 = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("f2".into())),
            site_url: Set(Some("https://b".into())),
            feed_url: Set("https://b/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        // entries: f1 -> 1 read + 1 unread; f2 -> 1 unread
        let _ = entry::ActiveModel {
            feed_id: Set(f1.id),
            guid: Set(Some("g1".into())),
            title: Set(Some("e1".into())),
            is_read: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _ = entry::ActiveModel {
            feed_id: Set(f1.id),
            guid: Set(Some("g2".into())),
            title: Set(Some("e2".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _ = entry::ActiveModel {
            feed_id: Set(f2.id),
            guid: Set(Some("g3".into())),
            title: Set(Some("e3".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });
        let resp = app
            .oneshot(
                Request::get("/feeds/counters")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        // reads[f1]=1; unreads[f1]=1; unreads[f2]=1
        assert_eq!(j["reads"][f1.id.to_string()].as_i64().unwrap_or(-1), 1);
        assert_eq!(j["unreads"][f1.id.to_string()].as_i64().unwrap_or(-1), 1);
        assert_eq!(j["unreads"][f2.id.to_string()].as_i64().unwrap_or(-1), 1);
    }

    #[tokio::test]
    async fn opml_export_import_roundtrip() {
        // source db
        let db1 = setup_db().await;
        let token1 = seed_user_and_token(&db1).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let c = category::ActiveModel {
            user_id: Set(1),
            name: Set("cat".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&db1)
        .await
        .unwrap();
        let _ = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(Some(c.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("title".into())),
            site_url: Set(Some("https://site".into())),
            feed_url: Set("https://site/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db1)
        .await
        .unwrap();
        let app1 = router().with_state(crate::AppState { db: db1.clone() });
        let resp = app1
            .clone()
            .oneshot(
                Request::get("/export")
                    .header("X-Auth-Token", token1.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let xml = String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(xml.starts_with("<?xml"));

        // target db
        let db2 = setup_db().await;
        let token2 = seed_user_and_token(&db2).await;
        let app2 = router().with_state(crate::AppState { db: db2.clone() });
        // import XML
        let resp = app2
            .clone()
            .oneshot(
                Request::post("/import")
                    .header("X-Auth-Token", token2.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/xml")
                    .body(axum::body::Body::from(xml.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // list feeds
        let resp = app2
            .oneshot(
                Request::get("/feeds")
                    .header("X-Auth-Token", token2.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.is_array());
        assert_eq!(j.as_array().unwrap().len(), 1);
        assert_eq!(j[0]["feed_url"].as_str().unwrap_or(""), "https://site/feed");
    }

    #[tokio::test]
    async fn discover_local_html() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        // start local server serving HTML with rel=alternate rss
        let app_site = axum::Router::new()
            .route(
                "/",
                axum::routing::get(|| async {
                    axum::http::Response::builder()
                        .header(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                        .body(
                            "<html><head><link rel=\"alternate\" type=\"application/rss+xml\" title=\"Site RSS\" href=\"/feed.xml\"></head><body>ok</body></html>"
                                .to_string(),
                        )
                        .unwrap()
                }),
            )
            .route(
                "/feed.xml",
                axum::routing::get(|| async {
                    axum::http::Response::builder()
                        .header(axum::http::header::CONTENT_TYPE, "application/rss+xml")
                        .body("<?xml version=\"1.0\"?><rss></rss>".to_string())
                        .unwrap()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app_site).await;
        });

        // call /v1/discover
        let app = router().with_state(crate::AppState { db: db.clone() });
        let url = format!("http://{}:{}", addr.ip(), addr.port());
        let body = serde_json::json!({"url": url});
        let resp = app
            .oneshot(
                Request::post("/discover")
                    .header("X-Auth-Token", token.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let list = json_body(resp).await;
        assert!(list.is_array());
        assert!(!list.as_array().unwrap().is_empty());
        // ensure discovered feed URL is absolute to /feed.xml
        let first_url = list[0]["url"].as_str().unwrap_or("");
        assert!(first_url.ends_with("/feed.xml"));
    }

    #[tokio::test]
    async fn fever_flow_basic() {
        use captura_storage::entity::user;
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        // set fever key for user 1
        if let Some(u) = User::find_by_id(1).one(&db).await.unwrap() {
            let mut am: user_entity::ActiveModel = u.into();
            am.fever_key_md5 = Set(Some(format!("{:x}", md5::Md5::digest(b"u:fever"))));
            let _ = am.update(&db).await.unwrap();
        }
        // seed feed and entry
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let c = category::ActiveModel {
            user_id: Set(1),
            name: Set("news".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(Some(c.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example".into())),
            feed_url: Set("https://example/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            title: Set(Some("hi".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        // build app with fever route
        let app = Router::new()
            .merge(super::router())
            .route(
                "/fever",
                axum::routing::get(crate::compat::fever::endpoint)
                    .post(crate::compat::fever::endpoint),
            )
            .with_state(crate::AppState { db: db.clone() });

        let key = format!("{:x}", md5::Md5::digest(b"u:fever"));
        // groups & feeds
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/fever?api_key={}&groups=1&feeds=1", key))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.get("groups").is_some());
        assert!(j.get("feeds").is_some());

        // items & unread ids
        let resp = app
            .oneshot(
                Request::get(format!(
                    "/fever?api_key={}&items=1&since_id=0&limit=50&unread_item_ids=1",
                    key
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.get("items").is_some());
        assert!(j.get("unread_item_ids").is_some());
    }

    #[tokio::test]
    async fn reader_unread_count_basic() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            title: Set(Some("hello".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        // app with reader routes
        let app = Router::new()
            .route(
                "/reader/api/0/unread-count",
                axum::routing::get(crate::compat::reader::unread_count),
            )
            .with_state(crate::AppState { db: db.clone() });
        let resp = app
            .oneshot(
                Request::get("/reader/api/0/unread-count")
                    .header(
                        axum::http::header::AUTHORIZATION,
                        format!("Bearer {}", token),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.get("unreadcounts").is_some());
    }

    #[tokio::test]
    async fn admin_users_flow_and_non_admin_forbidden() {
        let db = setup_db().await;
        // seed admin user and token
        let token = seed_user_and_token(&db).await;
        if let Some(u) = User::find_by_id(1).one(&db).await.unwrap() {
            let mut am: user_entity::ActiveModel = u.into();
            am.role = Set(user_entity::UserRole::Admin);
            let _ = am.update(&db).await.unwrap();
        }
        let app = Router::new()
            .merge(super::router())
            .with_state(crate::AppState { db: db.clone() });

        // list users (admin)
        let resp = app
            .clone()
            .oneshot(
                Request::get("/users")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // create user bob
        let body = json!({"username":"bob","password":"p","is_admin":false});
        let resp = app
            .clone()
            .oneshot(
                Request::post("/users")
                    .header("X-Auth-Token", token.as_str())
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let created = json_body(resp).await;
        let bob_id = created.get("id").and_then(|v| v.as_i64()).unwrap();

        // non-admin user u2
        let (_uid2, token2) = captura_testkit::seed_user_and_token(&db, "u2").await;
        let resp = app
            .clone()
            .oneshot(
                Request::get("/users")
                    .header("X-Auth-Token", token2.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // delete bob by admin -> 204
        let resp = app
            .clone()
            .oneshot(
                Request::delete(format!("/users/{}", bob_id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }
}

// tags get moved to tags.rs
