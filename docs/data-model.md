# Data Model

This document summarizes the current schema intended to avoid frequent breaking changes while keeping room for growth. It is influenced by Miniflux, FreshRSS and RSSHub use-cases.

## Entities

- user
  - id (PK), username (unique), password_hash, fever_key_md5 (nullable), created_at
- category
  - id (PK), user_id (FK user), name, view (NOT NULL, preferred view for this category; same key space as feed.view), created_at
- rule
  - id (PK), rule_id (unique), kind, version, namespace, description, spec_json (DSL v1 schema in JSON), handler_target, examples_json, verified_at, maintainer, created_at, updated_at
- feed
  - id (PK), user_id (FK user), category_id (FK category, nullable)
  - type: rss | atom | json | rule | hub
  - title, site_url, feed_url (for rule-type: source URL)
  - favicon_id (FK favicon, nullable), rule_id (FK rule, nullable)
  - view (NOT NULL, preferred view for this feed: articles | pictures | videos | audios | social | notifications; stored as snake_case string)
    - 语义：视图是**订阅属性**，不是纯查询参数；它决定该订阅在时间线中的默认呈现方式（例如文章流、图片流、视频流），并参与 `/api/v1/entries` 与 “mark-all-read” 等操作。
    - 继承规则：当创建订阅时未显式指定 `view` 且 `category_id` 指向的分类设置了 `view`，订阅会默认继承该分类视图；否则使用默认文章视图（`articles`）。
    - 查询侧可以通过 `?view=` 临时过滤：例如 `view=pictures` 仅返回首选视图为图片的订阅条目；`view=all` 则表示不按视图过滤。注意：逻辑视图 `all` **不会**写入 `feed.view`，仅存在于查询和 SmartView 语义中。
  - fetch options: user_agent, headers_json, cookies, proxy_url, fetch_via_proxy, disable_http2, allow_invalid_certs, request_timeout_ms
  - scheduling & state: checked_at, next_run_at, etag, last_modified, last_status, error_count, disabled
  - rewriting & filtering: scraper_rules, rewrite_rules, blocklist_rules, keeplist_rules, url_rewrite_rules, block_filter_entry_rules, keep_filter_entry_rules
  - created_at, updated_at
  - index: unique(user_id, feed_url)
- entry
  - id (PK), feed_id (FK feed)
  - guid (unique per feed), url, title, summary, content_html, author, published_at, hash
  - flags: is_read, is_starred
  - extras_json
  - created_at, updated_at
  - index: unique(feed_id, guid), index(feed_id, published_at)
- enclosure
  - id (PK), entry_id (FK entry), url, mime, length, kind
- label
  - id (PK), user_id (FK user), name, color, created_at
  - index: unique(user_id, name)
- entry_label
  - id (PK), entry_id (FK entry), label_id (FK label)
  - index: unique(entry_id, label_id)
- smart_view
  - id (PK), user_id (FK user)
  - name (display name for this smart view)
  - view (EntryView key: all | articles | pictures | videos | audios | social | notifications)
  - filters_json (JSON-encoded filters: feed_ids/category_ids/label_ids/search/status)
  - sort_by (optional: published_at | created_at)
  - sort_order (optional: asc | desc)
  - pinned (bool, whether highlighted in UI)
  - created_at, updated_at
- job
  - id (PK), user_id (FK user), feed_id (FK), rule_id (FK)
  - job_type: feed_refresh | favicon | integration
  - status: pending | running | done | failed
  - priority, run_at, attempts, last_error, created_at, updated_at
  - index: (status, run_at)

## Notes

- Feed owns user_id to keep implementation simple (Miniflux approach). Shared feeds can be considered later with a separate mapping.
- headers_json and extras_json are JSON-typed in Postgres, TEXT-backed JSON in SQLite; the API normalizes I/O.
- Filtering/rewriting fields are TEXT for portability; future versions can encode them with a richer format if needed.
- request_timeout_ms is per-feed, while global client defaults will be configured in service settings.
- Rule keeps the parsed DSL v1 spec in `spec_json`（由 YAML/JSON 解析而来）以及 `examples_json`，用于校验工具链和快照测试；YAML 仅作为输入格式存在于 API 层。

## Future-proofing

- Add optional tables for integration/webhook, favicon cache, read-states history, and error logs without breaking existing schema.
- Introduce a `host_policy` table if per-host quotas are required; current design can compute quotas in scheduler using aggregated queries.
- Consider a `setting` KV store per user for UI preferences; it is orthogonal to content storage.
- favicon
  - id (PK), feed_id (FK feed), url, mime, data (binary), created_at, updated_at
  - index: (feed_id)
