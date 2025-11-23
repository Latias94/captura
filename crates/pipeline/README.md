# captura-pipeline

`captura-pipeline` is the orchestration crate that turns feeds, rules
and Hub routes into normalized entries ready for storage.

It wires together:

- `captura-fetcher` for standard RSS/Atom/JSON feeds.
- `captura-crawler` for smart/anti-bot HTML fetching when requested.
- `captura-extract` for full-content extraction and rules DSL v1.
- `captura-hub` for RSSHub-style Hub routes.

Core concepts
-------------

- `NormalizedEntry` – common entry model shared via `captura-common`.
- `RefreshMeta` – HTTP/cache metadata captured during refresh
  (status, ETag, Last-Modified).
- `FeedConfigDto` – lightweight feed configuration (URL, headers,
  cookies, proxy, timeouts, rewrite/filter rules) that is independent
  of database models.

Refreshing standard feeds
-------------------------

For regular RSS/Atom/JSON feeds:

- `refresh_feed` – refresh a `feed::Model` and return entries.
- `refresh_feed_with_meta` – same as above but also returns `RefreshMeta`.
- `refresh_standard_feed_with_meta_dto` – refresh using a
  `FeedConfigDto` instead of a database model. This is useful for tools
  or clients that do not depend on `captura-storage`.

The standard refresh path:

1. Builds an HTTP client via `captura-fetcher` with the right headers,
   ETag / If-Modified-Since and proxy settings.
2. Parses the feed with `feed-rs`.
3. Normalizes entries into `NormalizedEntry`.
4. Applies URL/content rewrite rules and entry filters.

Rules DSL v1
------------

For v1 rules, the crate exposes:

- `refresh_rule_v1(feed, spec)` – execute a `RuleSpecV1` against a
  `feed::Model`, using its HTTP configuration (headers, cookies, proxy,
  timeouts) and rule-level fetch settings.

Internally this function:

- Executes list/detail, single_page, JSON or XPath sources.
- Applies v1 filters and optional full-content fetching logic.
- Applies description templates and standard rewrite/filter rules.

Hub routes
----------

Hub routes (RSSHub-style) are executed via:

- `execute_hub_route(hub_id, params)` – call a built-in Hub handler and
  return `HubData`.
- `refresh_hub_feed` (internal) – convert `captura_hub://` URLs in
  `feed::Model` into Hub executions and then into `NormalizedEntry`
  values.

The public API exposes only `execute_hub_route`; the higher-level
service layer decides how to wire it into feeds.

Content helpers
---------------

This crate also exposes a few helpers for working with HTML and entry
content:

- `sanitize_html` – clean potentially unsafe HTML fragments.
- `apply_rewrite_rules` – apply Miniflux-style rewrite rules to strings.
- `apply_entry_filters_with_cfg` – apply include/exclude filters using
  a `ContentTransformConfig`.
- `clean_url` – normalize URLs (strip tracking parameters, etc.).

Example: refreshing a feed DTO
------------------------------

```rust
use captura_pipeline::{refresh_standard_feed_with_meta_dto, FeedConfigDto};

async fn demo() -> captura_common::Result<()> {
    let cfg = FeedConfigDto {
        url: "https://example.com/feed.xml".to_string(),
        ..Default::default()
    };
    let (entries, meta) = refresh_standard_feed_with_meta_dto(&cfg).await?;
    println!("Entries: {}", entries.len());
    if let Some(m) = meta {
        println!("HTTP status: {:?}", m.last_status);
    }
    Ok(())
}
```

In a full Captura deployment, `captura-service` and `captura-scheduler`
are the main callers of this crate; most user-facing code talks to the
API instead of driving the pipeline directly.
