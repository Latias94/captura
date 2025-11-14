use axum::{routing::get, Router};
use captura_service::refresh_and_persist;
use captura_storage::entity::{entry, feed, rule};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use tracing_subscriber::EnvFilter;

/// 使用本地 HTTP 服务器验证：rule-type feed + YAML 规则
/// 能通过 service 层完整走 pipeline，最终将 entry 落库。
#[tokio::test]
async fn rule_feed_pipeline_persists_entries() {
    // 1) 启动本地 HTTP server，提供 list + article HTML
    async fn list_handler() -> &'static str {
        r#"<html><body>
            <a class="item" href="/article1">Item 1</a>
        </body></html>"#
    }
    async fn article_handler() -> &'static str {
        r#"<html><body>
            <div class="article"><p>Hello Rule Pipeline</p></div>
        </body></html>"#
    }
    let app = Router::new()
        .route("/list", get(list_handler))
        .route("/article1", get(article_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local_addr");
    let base = format!("http://{}", addr);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    // 2) 初始化内存数据库 + 用户 + rule + rule-type feed
    let db = captura_testkit::setup_db().await;
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // 简单 YAML 规则（DSL v1）：从 /list 抓取 a.item 链接，并用 CSS 选择正文
    let yaml = format!(
        r#"id: "test.rule"
version: 1
description: "rule pipeline e2e"
source:
  type: list_detail
  list:
    request:
      url: "{base}/list"
    item: "a.item"
  content:
    mode: "css"
    selector: "div.article"
"#,
        base = base
    );

    let rule_am = rule::ActiveModel {
        rule_id: Set("test.rule".to_string()),
        version: Set(None),
        namespace: Set(Some("test".to_string())),
        description: Set(Some("rule pipeline e2e".to_string())),
        yaml: Set(yaml),
        examples_json: Set(None),
        verified_at: Set(None),
        maintainer: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let r = rule_am.insert(&db).await.expect("insert rule");

    let feed_am = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rule),
        title: Set(Some("RuleFeed".into())),
        site_url: Set(Some(base.clone())),
        feed_url: Set("rule://test".into()),
        rule_id: Set(Some(r.id)),
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
        ..Default::default()
    };
    let f = feed_am.insert(&db).await.expect("insert feed");

    // 3) 通过 service 层刷新 rule feed 并持久化
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) 验证数据库中确实存在来自规则的 entry，且正文包含预期内容
    let entries = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f.id))
        .all(&db)
        .await
        .expect("query entries");
    assert!(!entries.is_empty(), "no entries persisted for rule feed");
    let has_expected = entries.iter().any(|e| {
        e.content_html
            .as_deref()
            .map(|c| c.contains("Hello Rule Pipeline"))
            .unwrap_or(false)
    });
    assert!(
        has_expected,
        "no entry content contained expected marker text"
    );
}
