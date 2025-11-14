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

/// 验证：`source.type = json` + `from_html` 能从 HTML 中的 JSON 片段构造条目并持久化。
#[tokio::test]
async fn rule_feed_json_from_html_persists_entries() {
    // 1) 启动本地 HTTP server，返回嵌有 JSON 的 HTML
    async fn html_json_handler() -> &'static str {
        r#"<html><body>
            <script id="data" type="application/json">
                {"items":[{"title":"FromHtml Title","url":"https://example.com/from_html"}]}
            </script>
        </body></html>"#
    }

    let app = Router::new().route("/html_json", get(html_json_handler));

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
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_json").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // DSL v1 JSON 规则：从 /html_json 抽取 script#data 文本作为 JSON
    let yaml = format!(
        r#"id: "test.rule.json_from_html"
version: 1
description: "json from html pipeline e2e"
source:
  type: json
  from_html:
    request:
      url: "{base}/html_json"
    selector: "script#data"
    multiple: false
  root: "items"
  mapping:
    title: "title"
    url: "url"
"#,
        base = base
    );

    let rule_am = rule::ActiveModel {
        rule_id: Set("test.rule.json_from_html".to_string()),
        version: Set(None),
        namespace: Set(Some("test".to_string())),
        description: Set(Some("json from html rule pipeline e2e".to_string())),
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
        title: Set(Some("RuleFeedJsonFromHtml".into())),
        site_url: Set(Some(base.clone())),
        feed_url: Set("rule://test-json-from-html".into()),
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
        .expect("refresh json-from-html rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) 验证数据库中 entry 的标题和 URL 对应预期 JSON 数据
    let entries = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f.id))
        .all(&db)
        .await
        .expect("query entries");
    assert!(!entries.is_empty(), "no entries persisted for json-from-html feed");
    let has_expected = entries.iter().any(|e| {
        e.title
            .as_deref()
            .map(|t| t == "FromHtml Title")
            .unwrap_or(false)
            && e.url
                .as_deref()
                .map(|u| u == "https://example.com/from_html")
                .unwrap_or(false)
    });
    assert!(
        has_expected,
        "no entry matched expected title/url from JSON fragment"
    );
}

/// 验证：DSL v1 filters.fetch_full_content_when + transform.content_merge
/// 能够触发全文抓取并替换原有 content_html。
#[tokio::test]
async fn rule_feed_full_content_when_persists_extracted_body() {
    use axum::routing::get;

    // 1) 本地 HTTP server：/list 返回摘要列表，/article_full 返回完整正文。
    async fn list_handler() -> &'static str {
        r#"<html><body>
            <ul>
              <li>
                <a class="item" href="/article_full">Read full article</a>
                <p class="summary">Short summary</p>
              </li>
            </ul>
        </body></html>"#
    }
    async fn article_handler() -> &'static str {
        r#"<html><body>
            <article><p>FULL_CONTENT_MARKER</p></article>
        </body></html>"#
    }

    let app = Router::new()
        .route("/list_full", get(list_handler))
        .route("/article_full", get(article_handler));

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
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_full").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // 规则：list_detail + 空 CSS 内容选择器；遇到标题包含 "Read full" 条目时，
    // 触发全文抓取，并用 content_merge.mode = replace 覆盖 content_html。
    let yaml = format!(
        r#"id: "test.rule.full_content"
version: 1
description: "full content when pipeline e2e"
source:
  type: list_detail
  list:
    request:
      url: "{base}/list_full"
    item: "li"
    link: "a.item@href"
    title: "a.item"
    summary: "p.summary"
  content:
    mode: "css"
    selector: "div.does-not-exist"
filters:
  fetch_full_content_when:
    - field: title
      regex: ".*Read full.*"
transform:
  content_merge:
    mode: replace
"#,
        base = base
    );

    let rule_am = rule::ActiveModel {
        rule_id: Set("test.rule.full_content".to_string()),
        version: Set(None),
        namespace: Set(Some("test".to_string())),
        description: Set(Some("full content rule pipeline e2e".to_string())),
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
        title: Set(Some("RuleFeedFullContent".into())),
        site_url: Set(Some(base.clone())),
        feed_url: Set("rule://test-full-content".into()),
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

    // 3) 刷新并持久化
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh full-content rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) 验证 content_html 中包含 FULL_CONTENT_MARKER，说明确实使用全文替换。
    let entries = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f.id))
        .all(&db)
        .await
        .expect("query entries");
    assert!(
        !entries.is_empty(),
        "no entries persisted for full-content rule feed"
    );
    let has_full_content = entries.iter().any(|e| {
        e.content_html
            .as_deref()
            .map(|c| c.contains("FULL_CONTENT_MARKER"))
            .unwrap_or(false)
    });
    assert!(
        has_full_content,
        "no entry content contained FULL_CONTENT_MARKER from full-content fetch"
    );
}

/// 验证：`source.type = xpath` 能通过轻量级 XPath→CSS 转换执行抓取并持久化。
#[tokio::test]
async fn rule_feed_xpath_persists_entries() {
    use axum::routing::get;

    // 1) 本地 HTTP server：/xpath_list 返回包含复杂结构的 HTML。
    async fn xpath_list_handler() -> &'static str {
        r#"<html><body>
            <ul id="items">
              <li>
                <h2><a href="/xpath_article1">XPath Title</a></h2>
                <div class="entry-content"><p>XPath Body</p></div>
              </li>
            </ul>
        </body></html>"#
    }

    let app = Router::new().route("/xpath_list", get(xpath_list_handler));

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
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_xpath").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // XPath 规则：使用 //ul/li 作为 item，.//h2/text() 作为标题，
    // .//a/@href 作为链接，.//div[@class='entry-content'] 作为正文 HTML。
    let yaml = format!(
        r#"id: "test.rule.xpath"
version: 1
description: "xpath rule pipeline e2e"
source:
  type: xpath
  request:
    url: "{base}/xpath_list"
  xpath:
    item: "//ul/li"
    title: ".//h2/text()"
    url: ".//a/@href"
    content_html: ".//div[@class='entry-content']"
"#,
        base = base
    );

    let rule_am = rule::ActiveModel {
        rule_id: Set("test.rule.xpath".to_string()),
        version: Set(None),
        namespace: Set(Some("test".to_string())),
        description: Set(Some("xpath rule pipeline e2e".to_string())),
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
        title: Set(Some("RuleFeedXPath".into())),
        site_url: Set(Some(base.clone())),
        feed_url: Set("rule://test-xpath".into()),
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

    // 3) 刷新并持久化
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh xpath rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) 验证标题、URL 与正文 HTML。
    let entries = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f.id))
        .all(&db)
        .await
        .expect("query entries");
    assert!(
        !entries.is_empty(),
        "no entries persisted for xpath rule feed"
    );
    let has_expected = entries.iter().any(|e| {
        e.title
            .as_deref()
            .map(|t| t == "XPath Title")
            .unwrap_or(false)
            && e.url
                .as_deref()
                .map(|u| u.ends_with("/xpath_article1"))
                .unwrap_or(false)
            && e.content_html
                .as_deref()
                .map(|c| c.contains("XPath Body"))
                .unwrap_or(false)
    });
    assert!(
        has_expected,
        "no entry matched expected title/url/content for xpath rule"
    );
}
