# Captura API v1

Base paths

- 原生 REST：`/api/v1/*`
- Miniflux 兼容：`/v1/*`（错误返回 `{ "error_message": "..." }`）
- Fever 兼容：`/fever`（单端点 GET/POST）
- Google Reader 兼容：`/reader/api/0/*`

路由挂载与兼容层说明

- 兼容端点（Miniflux/Fever/Reader）不再重复挂载在 `/api/v1` 下，避免路径分叉与行为不一致。
- 建议客户端使用：
  - 原生 REST：固定访问 `/api/v1/*`
  - Miniflux 客户端：固定访问 `/v1/*`
  - Fever 客户端：固定访问 `/fever`
  - Google Reader 客户端：固定访问 `/reader/api/0/*`

## First-party clients (TUI/CLI/GUI)

For first-party clients maintained together with Captura (such as `captura-tui`), the recommended usage is:

- Auth:
  - Obtain a token either via Web UI (API key), `POST /api/v1/auth/login`, or Miniflux-compatible `/v1/api-keys`.
  - Use the header `Authorization: Bearer <token>` for all authenticated requests.
  - Both `/api/v1` and `/v1` accept bearer tokens; prefer bearer over `X-Auth-Token` or `Basic` for new clients.
- Preferred `/api/v1` endpoints:
  - Feeds:
    - `GET /api/v1/feeds` – list feeds for the current user.
    - `GET /api/v1/feeds/{id}` – get a single feed.
    - `GET /api/v1/feeds/counters` – read/unread counters per feed.
    - `POST /api/v1/feeds/{id}/refresh` – synchronous refresh of a single feed.
    - `POST /api/v1/feeds/{id}/enqueue-refresh` – enqueue refresh job.
  - Categories:
    - `GET /api/v1/categories` – list categories.
    - `GET /api/v1/categories/counters` – unread counters per category (including `null` = uncategorized).
  - Entries:
    - `GET /api/v1/entries` – list entries with filters (`feed_id`, `category_id`, `status`, `limit`, `offset`).
    - `GET /api/v1/entries/{id}` – get a single entry.
    - `GET /api/v1/entries/{id}/content?update_content=bool` – fetch and optionally persist full content (readability).
    - `POST /api/v1/entries/{id}/read` – mark entry read/unread (`{ "value": true|false }`).
    - `POST /api/v1/entries/{id}/star` – mark entry starred/unstarred (`{ "value": true|false }`).
    - `POST /api/v1/entries/mark-all-read` – mark entries as read by `feed_id` or `category_id`.
- Compatibility layers:
  - `/v1/*` is a Miniflux-compatible API surface, intended for Miniflux clients and other third-party readers.
  - `/fever` and `/reader/api/0/*` are intended for Fever / Google Reader compatible clients.
  - First-party clients should prefer `/api/v1/*` for most operations and only use compatibility endpoints when strictly necessary.

## Auth

- `POST /users`
  - Create first user (allowed only when no user exists).
  - Body: `{ "username": "...", "password": "..." }`
  - Resp: `{ "id": 1 }`

- `POST /auth/login`
  - Body: `{ "username": "...", "password": "...", "name": "optional" }`
  - Resp: `{ "token": "..." }`
  - Use header: `Authorization: Bearer <token>` for subsequent requests.

- `POST /users/:id/fever-key`
  - Auth required (must be the same user id)
  - Body: `{ "api_password": "..." }`
  - Effect: stores `md5("username:api_password")` (lowercase hex) into the user's Fever key for Fever-compatible clients.
  - Resp: `ok`

## Health

- `GET /healthz`
  - Resp: `ok`

## Feeds

- `POST /feeds` (create)
  - Auth required
  - Body (subset): `{ "type": "rss|atom|json|rule", "feed_url": "...", "category_id": 1, "user_agent": "...", "headers_json": {...}, "cookies": "...", "proxy_url": "...", "fetch_via_proxy": false, "disable_http2": false, "allow_invalid_certs": false, "request_timeout_ms": 15000 }`
  - Resp: `{ "id": 1 }`
- `GET /feeds` (list)
  - Query: `category_id?`
- `GET /feeds/:id` (get)
- `PATCH /feeds/:id` (update)
  - Body (any subset): `{ "title": "...", "category_id": 1, "disabled": false, "user_agent": "...", "headers_json": {...}, "cookies": "...", "proxy_url": "...", "fetch_via_proxy": false, "disable_http2": false, "allow_invalid_certs": false, "request_timeout_ms": 15000 }`
- `DELETE /feeds/:id` (delete)
- `POST /feeds/:id/refresh`
  - Resp: `{ "inserted": 3 }`
- `POST /feeds/:id/favicon/refresh`
  - Auth required
  - Effect: tries to fetch `site_url` + `/favicon.ico`, stores to `favicon` table and updates `feed.favicon_id`
  - Resp: `{ "favicon_id": 123, "updated": true }`

## Favicons

- `GET /favicons/:id`
  - Auth required
  - Returns raw favicon bytes; content-type is set if stored

## Rules

- `GET /rules` `POST /rules` `GET /rules/:id` `PUT /rules/:id` `DELETE /rules/:id`
  - Manage rules (YAML DSL stored in DB), with basic validation on create/update.

- `POST /rules/try`
  - Auth required
  - Body: `{ "url": "https://example.com/page-or-list", "rule_id?": 1, "yaml?": "..." }` (either `rule_id` or `yaml` is required)
  - Effect: parses rule (from DB or inline YAML), overrides `list.url` with the provided `url`, executes extraction with strategy:
    - smart crawler when `fetch.smart=true` (respects DSL: `user_agent`, `respect_robots`, `delay_ms`, `limit`) unless a proxy is configured
    - plain HTTP otherwise（应用 DSL: `user_agent`, `timeout_ms`, `proxy_url`, `headers`）
  - Resp (preview up to 5):
    - `{ used_smart, list_url, item_count, entries: [{ title, url, content_len }], ua, timeout_ms, respect_robots, delay_ms, limit, proxy_applied, list_html_len, fallback_used, http_status, duration_ms }`

## Jobs

- `GET /jobs?status?=pending|running|done|failed&limit?&offset?`
  - Auth required
  - List user jobs.
- `POST /feeds/:id/enqueue-refresh`
  - Auth required
  - Enqueue a feed refresh job, returns `{ id }` (job id)
- `POST /jobs/run-once`
  - Auth required
  - Run up to 10 due jobs immediately (useful for development/testing)

## Fever 兼容（读多写少）

- Endpoint: `GET/POST /fever`
  - Query/body parameters (Fever spec compatible):
    - `api=1` (optional, probe)
    - `api_key` (required for auth): MD5 of `username:api_password` (lowercase hex)
    - `groups=1` → includes `{ groups, feeds_groups }`
    - `feeds=1` → includes `{ feeds }`
    - `favicons=1` → includes `{ favicons }` (each item: `{ id, data }`, `data`为base64字符串)
    - `items=1&since_id?=N[&limit?=N]` → includes `{ items, total_items }`（默认最多50，最大200）
    - `unread_item_ids=1` → includes `{ unread_item_ids }` (comma-separated string)
    - `saved_item_ids=1` → includes `{ saved_item_ids }` (comma-separated string)
  - Base response always includes: `{ api_version: 3, auth: 0|1, last_refreshed_on_time }`
  - When `auth=0` only the base fields are present. When `auth=1`, requested sections are included.

## Entries

- `GET /entries`
  - Query: `feed_id?`, `category_id?`, `status?=read|unread|starred`, `limit?`, `offset?`
  - Resp: `[{ id, feed_id, url, title, summary, content_html, author, published_at, is_read, is_starred }]`
- `POST /entries/:id/read` Body: `{ "value": true }`
- `POST /entries/:id/star` Body: `{ "value": true }`
- `POST /entries/mark-all-read` Body: `{ "feed_id?": 1, "category_id?": 2 }`

## Miniflux 兼容（概要）

- 路径：`/v1/*`
- 主要端点（子集）：
  - 用户/版本：`GET /v1/me`、`GET /v1/version`、`GET /v1/integrations/status`
  - 分类：`GET/POST /v1/categories`、`PUT/DELETE /v1/categories/:id`、`GET /v1/categories/counters`、`PUT /v1/categories/:id/mark-all-as-read`
  - 订阅源：`GET/POST /v1/feeds`、`GET/PUT/DELETE /v1/feeds/:id`、`POST /v1/feeds/:id/refresh`、`PUT /v1/feeds/refresh`、`GET /v1/feeds/counters`、`GET /v1/feeds/:id/icon`
  - 条目：`GET /v1/entries`、`PUT /v1/entries/:id`、`PUT /v1/entries/:id/star`、`POST /v1/entries/:id/save`、`GET /v1/entries/:id/fetch-content`、`POST/DELETE /v1/entries/:id/tags`
  - 标签：`GET/POST /v1/tags`、`GET/PUT/DELETE /v1/tags/:name`
  - OPML：`GET /v1/export`、`POST /v1/import`
  - 发现：`POST /v1/discover[?verify=true]`
  - API Keys：`GET/POST /v1/api-keys`、`DELETE /v1/api-keys/:id`
- 错误：400/500 返回 `{ "error_message": "..." }`，其余按状态码处理。

## Notes
- All timestamps are RFC3339.
- 原生 REST Errors: JSON body `{ code, message }`，包含 HTTP 状态码。
  - Codes (initial set):
    - `bad_request` (400): invalid parameters or missing fields
    - `unauthorized` (401): invalid or missing token
    - `forbidden` (403): action not allowed
    - `not_found` (404): resource not found
    - `internal_error` (500): unexpected server error
  - Example:
    - Status: 404 Not Found
    - Body: `{ "code": "not_found", "message": "feed not found" }`
