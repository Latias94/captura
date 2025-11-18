# Captura API Quickstart for Client Developers

This document is a short, opinionated guide for building first‑party
clients (Web/TUI/mobile) on top of Captura. It focuses on the native
`/api/v1/*` surface and the unified timeline model.

For the full reference, see:

- `docs/api.md` – endpoint catalogue and payload details.
- `docs/timeline.md` – unified timeline semantics (`EntryView`, SmartViews,
  Timelines, `TimelineQuery`).

---

## 1. Authentication & Base URLs

- Base paths:
  - Native REST: `/api/v1/*`
  - Miniflux compatible: `/v1/*`
  - Fever: `/fever`
  - Google Reader: `/reader/api/0/*`
- Recommended for new clients: **use `/api/v1/*`** only.
- Auth flow:
  1. If no user exists, create one:
     - `POST /api/v1/users`
       - Body: `{ "username": "...", "password": "..." }`
       - Resp: `{ "id": 1 }`
  2. Log in:
     - `POST /api/v1/auth/login`
       - Body: `{ "username": "...", "password": "..." }`
       - Resp includes a bearer token.
  3. Send the token on every request:
     - Header: `Authorization: Bearer <token>`

You can also obtain tokens via Miniflux-compatible `/v1/api-keys`, but
native clients should prefer `/api/v1/auth/login` or Web UI–generated tokens.

---

## 2. First sync: load structure & preferences

On first launch (after login), a client typically needs:

1. **Feeds & categories**
   - `GET /api/v1/feeds`
     - Includes `view` for each feed (`articles|pictures|...`).
   - `GET /api/v1/categories`
     - Includes `view` for each category (default view preference).
   - `GET /api/v1/feeds/counters`
   - `GET /api/v1/categories/counters`

2. **Labels**
   - `GET /api/v1/labels`
   - Use for entry tagging/filtering and SmartView filters.

3. **Built‑in views & timelines**
   - `GET /api/v1/views`
     - List of supported `EntryView` values with labels and descriptions.
   - `GET /api/v1/smart-views`
     - User‑defined named timelines (SmartViews).
   - `GET /api/v1/timelines`
     - Unified timeline directory:
       - `kind="view"` – built‑in views (`Articles`, `Pictures`, …).
       - `kind="smart_view"` – user SmartViews.
     - Clients can treat this as the source of truth for sidebar navigation.

4. **Optional: full export for backup/migration**
   - `GET /api/v1/export/full`
   - Contains categories, feeds, SmartViews, labels and user prefs.

---

## 3. Unified timeline usage

The unified timeline model is Folo‑inspired:

- `/api/v1/entries` – **global timeline**, view‑aware.
- `/api/v1/smart-views/{id}/entries` – **named timelines**.
- `/api/v1/timelines` – directory of all timelines.

See `docs/timeline.md` for full semantics. Below is a practical guide.

### 3.1 Global timeline: `GET /api/v1/entries`

Use this for the main reading experience.

Typical query parameters:

- `view`:
  - `articles` (default), `pictures`, `videos`, `audios`, `social`, `notifications`, `all`.
  - `view=all` – no view filtering.
- `status`:
  - `read`, `unread`, `starred`.
- `feed_id`, `category_id`:
  - Restrict to a single feed or category.
- `search` (or `q`):
  - Full‑text search; supports:
    - `title:"foo bar"`, `author:alice`, `url:example.com`
    - tags via `#tagname`
    - remaining text as general search terms.
- Sorting:
  - `sort_by=published_at|created_at|relevance`
  - `order=asc|desc`
  - When searching on Postgres and `sort_by` is not provided, Captura
    defaults to `relevance desc, published_at desc, created_at desc`.
- Paging:
  - `limit` (default 100), `offset` (default 0).

Example: unread articles timeline

```http
GET /api/v1/entries?view=articles&status=unread&limit=50&offset=0
Authorization: Bearer <token>
```

Example: search in all views

```http
GET /api/v1/entries?view=all&search=rust%20#security&sort_by=relevance
Authorization: Bearer <token>
```

### 3.2 Named timelines: SmartViews

SmartViews are stored timeline definitions:

- `GET /api/v1/smart-views`
  - Returns `SmartViewDto[]` (id/name/view/filters/sort/pinned).
- `POST /api/v1/smart-views`
  - Create a new SmartView (e.g. “Unread Pictures from Work feeds”):

    ```jsonc
    {
      "name": "Unread Pics",
      "view": "pictures",
      "filters": {
        "feed_ids": [1, 2],
        "label_ids": [5],
        "status": "unread"
      },
      "sort_by": "published_at",
      "sort_order": "desc",
      "pinned": true
    }
    ```

- `GET /api/v1/smart-views/{id}/entries`
  - Returns entries according to the SmartView definition:
    - `view` → timeline view.
    - `filters.feed_ids/category_ids/label_ids/search/status`.
    - `sort_by/sort_order` from SmartView, overridable via query.

Clients can:

- Use `/api/v1/timelines` to list all timelines (views + SmartViews).
- For `kind="smart_view"` entries, call `/api/v1/smart-views/{id}/entries`
  to fetch items.

### 3.3 Entry details and content

- `GET /api/v1/entries/{id}`
  - Quick metadata: `url/title/summary/content_html/...`.
- `GET /api/v1/entries/{id}/content?update_content=true|false`
  - Fetch full content using Captura’s extractor.
  - When `update_content=true`, the extracted content is persisted back
    to the entry.

---

## 4. Mark as read/starred

### 4.1 Single entry updates

- `POST /api/v1/entries/{id}/read`
  - Body: `{ "value": true }` or `{ "value": false }`.
- `POST /api/v1/entries/{id}/star`
  - Same shape.

Clients typically:

- Toggle read/starred in the UI.
- Fire these small POSTs; optimistic UI updates are recommended.

### 4.2 Mark‑all‑read

- `POST /api/v1/entries/mark-all-read`
  - Body must provide at least one of:
    - `feed_id`
    - `category_id`
    - `view` (`all|articles|pictures|videos|audios|social|notifications`)

Examples:

- Mark a single feed as read:

  ```json
  { "feed_id": 42 }
  ```

- Mark all “Articles” view entries as read:

  ```json
  { "view": "articles" }
  ```

This operation is implemented on top of a view‑aware filter in the service
layer, so semantics are consistent with the listing endpoints.

---

## 5. Managing subscriptions and structure

### 5.1 Feeds

- `GET /api/v1/feeds`
  - Includes id/title/feed_url/site_url/category_id/view/disabled.
- `POST /api/v1/feeds`
  - Body: `{ "feed_url": "...", "category_id?": 1, "view?": "articles|..." }`.
  - The server infers feed type (rss/atom/json/hub/rule) and schedules
    refresh jobs.
- `GET /api/v1/feeds/{id}` / `PATCH /api/v1/feeds/{id}` / `DELETE /api/v1/feeds/{id}`.
- `POST /api/v1/feeds/{id}/refresh`
  - Synchronous refresh of a single feed.
- `POST /api/v1/feeds/{id}/enqueue-refresh`
  - Enqueue a refresh job for background workers.
- `GET /api/v1/feeds/counters`
  - Read/unread counters per feed.
- `POST /api/v1/feeds/bulk-view`
  - Bulk change preferred view for multiple feeds.

### 5.2 Categories

- `GET /api/v1/categories`
  - Includes `view` preference per category.
- `POST /api/v1/categories`
  - Create; body `{ "name": "...", "view?": "articles|..." }`.
- `GET/PUT/DELETE /api/v1/categories/{id}`
- `GET /api/v1/categories/counters`
  - Unread counters per category.

### 5.3 Labels

- `GET /api/v1/labels`
- `POST /api/v1/labels`
  - `{ "name": "...", "color?": "#rrggbb" }`.
- `PUT /api/v1/labels/{id}`
- `DELETE /api/v1/labels/{id}`

Labels can be used to build SmartViews (filters.label_ids) and for future
label‑based mark‑all‑read operations.

---

## 6. Export, import and compatibility

### 6.1 Native full export/import

- `GET /api/v1/export/full`
  - Captura‑native, view‑aware export:
    - categories (with view)
    - feeds (type, fetch config, filters, view)
    - smart_views
    - labels
    - user_prefs
- `POST /api/v1/import/full`
  - Re‑imports a previously exported payload for the current user.

### 6.2 Miniflux / Fever / Reader

Captura exposes compatibility layers for existing clients:

- Miniflux: `/v1/*`
  - `GET /v1/entries`, `/v1/feeds`, `/v1/export`, etc.
- Fever: `/fever`
- Google Reader: `/reader/api/0/*`

New clients should only rely on `/api/v1/*`. Compatibility endpoints are
intended for third‑party readers and migration scenarios.

---

## 7. Where to look next

- For HTTP/JSON shapes and additional endpoints (OPML, webhooks, rules,
  integrations, jobs, auth variants), read `docs/api.md`.
- For precise timeline semantics and how they map to the service layer,
  read `docs/timeline.md`.
- For Hub routes and rule‑based subscriptions (RSSHub‑style routes),
  read `docs/hub.md`.

If you stick to the flows described in this quickstart, you will already
cover the majority of a modern reader UI: discovery of timelines, global
timeline reading, SmartViews, read/starred state, and basic subscription
management.\

