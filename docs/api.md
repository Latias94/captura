# Captura API v1

Base path: `/api/v1`

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

## Fever (read-only)

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

## Notes
- All timestamps are RFC3339.
- Errors: JSON body with fields `{ code, message }`, with appropriate HTTP status codes.
  - Codes (initial set):
    - `bad_request` (400): invalid parameters or missing fields
    - `unauthorized` (401): invalid or missing token
    - `forbidden` (403): action not allowed
    - `not_found` (404): resource not found
    - `internal_error` (500): unexpected server error
  - Example:
    - Status: 404 Not Found
    - Body: `{ "code": "not_found", "message": "feed not found" }`
