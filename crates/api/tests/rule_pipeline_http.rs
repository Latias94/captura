use axum::{routing::get, Router};
use captura_hub::v1::parse_rule_v1;
use captura_service::refresh_and_persist;
use captura_storage::entity::{entry, feed, rule};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

/// Use a local HTTP server to verify that a `rule`-type feed with YAML rules
/// can go through the full service pipeline and persist entries.
#[tokio::test]
async fn rule_feed_pipeline_persists_entries() {
    // 1) Start a local HTTP server that serves list + article HTML.
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

    // 2) Initialize in-memory database, user, rule and rule-type feed.
    let db = captura_testkit::setup_db().await;
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Simple DSL v1 YAML rule: fetch links from `/list` via `a.item`, then use CSS for content.
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

    let spec = parse_rule_v1(&yaml).expect("parse rule yaml");
    let spec_json =
        serde_json::to_value(&spec).expect("encode rule spec_json in rule_feed_pipeline");
    let examples_json = serde_json::to_value(&spec.examples)
        .expect("encode rule examples_json in rule_feed_pipeline");
    let namespace = spec.id.rsplit_once('.').map(|(ns, _)| ns.to_string());

    let rule_am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        kind: Set("dsl".to_string()),
        version: Set(None),
        namespace: Set(namespace),
        description: Set(spec.description.clone()),
        spec_json: Set(Some(spec_json)),
        handler_target: Set(None),
        examples_json: Set(Some(examples_json)),
        verified_at: Set(Some(now)),
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

    // 3) Refresh the rule feed via the service layer and persist entries.
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) Verify that entries from the rule exist and content contains the expected marker.
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

/// Verify `source.type = json` + `from_html` can build entries from HTML-embedded
/// JSON fragments and persist them.
#[tokio::test]
async fn rule_feed_json_from_html_persists_entries() {
    // 1) Start a local HTTP server that returns HTML embedding JSON.
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

    // 2) Initialize in-memory database, user, rule and rule-type feed.
    let db = captura_testkit::setup_db().await;
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_json").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // DSL v1 JSON rule: extract `script#data` text from `/html_json` as JSON.
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

    let spec = parse_rule_v1(&yaml).expect("parse json-from-html rule yaml");
    let spec_json =
        serde_json::to_value(&spec).expect("encode rule spec_json in json_from_html test");
    let examples_json = serde_json::to_value(&spec.examples)
        .expect("encode rule examples_json in json_from_html test");
    let namespace = spec.id.rsplit_once('.').map(|(ns, _)| ns.to_string());

    let rule_am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        kind: Set("dsl".to_string()),
        version: Set(None),
        namespace: Set(namespace),
        description: Set(spec.description.clone()),
        spec_json: Set(Some(spec_json)),
        handler_target: Set(None),
        examples_json: Set(Some(examples_json)),
        verified_at: Set(Some(now)),
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

    // 3) Refresh the rule feed via the service layer and persist entries.
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh json-from-html rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) Verify that entry title and URL match the expected JSON data.
    let entries = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f.id))
        .all(&db)
        .await
        .expect("query entries");
    assert!(
        !entries.is_empty(),
        "no entries persisted for json-from-html feed"
    );
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

/// Verify that `fetch.proxies` (rule-level proxy config) overrides feed-level
/// proxy settings for JSON rules, so rules can steer traffic through dedicated
/// proxy pools independent of per-feed defaults.
#[tokio::test]
async fn rule_feed_json_uses_rule_level_proxy_over_feed_proxy() {
    use axum::http::StatusCode;
    use axum::routing::any;

    // 1) Start a simple HTTP server acting as a dummy proxy. It ignores the
    // upstream URL and always returns a JSON payload compatible with the rule
    // mapping below.
    async fn proxy_handler() -> (StatusCode, &'static str) {
        (
            StatusCode::OK,
            r#"{"items":[{"title":"ViaProxy","url":"https://example.com/via_proxy"}]}"#,
        )
    }

    let app = Router::new().fallback(any(proxy_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind proxy listener");
    let addr = listener.local_addr().expect("proxy local_addr");
    let proxy_base = format!("http://{}", addr);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve proxy");
    });

    // 2) Init in-memory DB + user + rule + rule-type feed with a bogus feed-level
    // proxy, and a valid rule-level proxy pointing to our dummy proxy server.
    let db = captura_testkit::setup_db().await;
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_json_proxy").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // JSON rule: target URL is an unreachable host; only the proxy can make it work.
    let yaml = format!(
        r#"id: "test.rule.json_proxy"
version: 1
description: "json proxy override test"
fetch:
  proxies:
    - "{proxy}"
source:
  type: json
  request:
    url: "http://unreachable.example.local/json"
  root: "items"
  mapping:
    title: "title"
    url: "url"
"#,
        proxy = proxy_base
    );

    let spec = parse_rule_v1(&yaml).expect("parse json proxy rule yaml");
    let spec_json = serde_json::to_value(&spec).expect("encode rule spec_json in json_proxy test");
    let examples_json =
        serde_json::to_value(&spec.examples).expect("encode rule examples_json in json_proxy test");
    let namespace = spec.id.rsplit_once('.').map(|(ns, _)| ns.to_string());

    let rule_am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        kind: Set("dsl".to_string()),
        version: Set(None),
        namespace: Set(namespace),
        description: Set(spec.description.clone()),
        spec_json: Set(Some(spec_json)),
        handler_target: Set(None),
        examples_json: Set(Some(examples_json)),
        verified_at: Set(Some(now)),
        maintainer: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let r = rule_am.insert(&db).await.expect("insert rule");

    // Feed-level proxy is intentionally bogus; if rule-level proxies were not
    // respected, refresh would fail due to proxy connection errors.
    let feed_am = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rule),
        title: Set(Some("RuleFeedJsonProxy".into())),
        site_url: Set(Some("http://unreachable.example.local".into())),
        feed_url: Set("rule://test-json-proxy".into()),
        rule_id: Set(Some(r.id)),
        rule_params_json: Set(None),
        user_agent: Set(None),
        username: Set(None),
        password: Set(None),
        headers_json: Set(None),
        cookies: Set(None),
        proxy_url: Set(Some("http://127.0.0.1:1".into())),
        fetch_via_proxy: Set(true),
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

    // 3) Refresh the rule feed; this should succeed only if the rule-level proxy
    // configuration is applied (i.e. the engine uses `fetch.proxies[0]`).
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh json-proxy rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted via proxy, got {}",
        inserted
    );

    // 4) Verify that entry title and URL match the JSON returned by the proxy.
    let entries = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(f.id))
        .all(&db)
        .await
        .expect("query entries");
    assert!(
        !entries.is_empty(),
        "no entries persisted for json-proxy rule feed"
    );
    let has_expected = entries.iter().any(|e| {
        e.title.as_deref().map(|t| t == "ViaProxy").unwrap_or(false)
            && e.url
                .as_deref()
                .map(|u| u == "https://example.com/via_proxy")
                .unwrap_or(false)
    });
    assert!(
        has_expected,
        "no entry matched expected title/url from proxy JSON"
    );
}

/// Verify DSL v1 `filters.fetch_full_content_when` + `transform.content_merge`
/// can trigger full-content fetching and replace the original `content_html`.
#[tokio::test]
async fn rule_feed_full_content_when_persists_extracted_body() {
    use axum::routing::get;

    // 1) Local HTTP server: `/list` returns a summary list and `/article_full` returns full content.
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

    // 2) Initialize in-memory database, user, rule and rule-type feed.
    let db = captura_testkit::setup_db().await;
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_full").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Rule: `list_detail` + empty CSS content selector; when title contains "Read full",
    // trigger full-content fetch and use `content_merge.mode = replace` to override `content_html`.
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

    let spec = parse_rule_v1(&yaml).expect("parse full_content rule yaml");
    let spec_json =
        serde_json::to_value(&spec).expect("encode rule spec_json in full_content test");
    let examples_json = serde_json::to_value(&spec.examples)
        .expect("encode rule examples_json in full_content test");
    let namespace = spec.id.rsplit_once('.').map(|(ns, _)| ns.to_string());

    let rule_am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        kind: Set("dsl".to_string()),
        version: Set(None),
        namespace: Set(namespace),
        description: Set(spec.description.clone()),
        spec_json: Set(Some(spec_json)),
        handler_target: Set(None),
        examples_json: Set(Some(examples_json)),
        verified_at: Set(Some(now)),
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

    // 3) Refresh and persist.
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh full-content rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) Ensure `content_html` contains `FULL_CONTENT_MARKER`,
    // confirming full-content replacement took place.
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

/// Verify `source.type = xpath` can crawl and persist entries via a light XPath→CSS conversion.
#[tokio::test]
async fn rule_feed_xpath_persists_entries() {
    use axum::routing::get;

    // 1) Local HTTP server: `/xpath_list` returns HTML with a complex structure.
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

    // 2) Initialize in-memory database, user, rule and rule-type feed.
    let db = captura_testkit::setup_db().await;
    let (uid, _token) = captura_testkit::seed_user_and_token(&db, "rule_user_xpath").await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // XPath rule: use `//ul/li` as item, `.//h2/text()` as title,
    // `.//a/@href` as link, `.//div[@class='entry-content']` as content HTML.
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

    let spec = parse_rule_v1(&yaml).expect("parse xpath rule yaml");
    let spec_json = serde_json::to_value(&spec).expect("encode rule spec_json in xpath test");
    let examples_json =
        serde_json::to_value(&spec.examples).expect("encode rule examples_json in xpath test");
    let namespace = spec.id.rsplit_once('.').map(|(ns, _)| ns.to_string());

    let rule_am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        kind: Set("dsl".to_string()),
        version: Set(None),
        namespace: Set(namespace),
        description: Set(spec.description.clone()),
        spec_json: Set(Some(spec_json)),
        handler_target: Set(None),
        examples_json: Set(Some(examples_json)),
        verified_at: Set(Some(now)),
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

    // 3) Refresh and persist entries.
    let inserted = refresh_and_persist(&db, &f)
        .await
        .expect("refresh xpath rule feed");
    assert!(
        inserted >= 1,
        "expected at least one entry inserted, got {}",
        inserted
    );

    // 4) Verify title, URL and content HTML.
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
