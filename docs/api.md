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
    - `GET /api/v1/feeds` – list feeds for the current user（包含每个 feed 的 `view` 字段和错误统计，用于视图过滤与错误提示；`view` 总是一个有效的 `EntryView` 值，默认 `articles`）。
    - `GET /api/v1/feeds/{id}` – get a single feed（包含 `view` 和 `error_count/last_error_message`，永不为 null）。
    - `GET /api/v1/feeds/counters` – read/unread counters per feed（返回 `FeedCountersDto`，包含 `reads/unreads` 两个 `feed_id -> count` 的映射）。
    - `POST /api/v1/feeds/{id}/refresh` – synchronous refresh of a single feed.
    - `POST /api/v1/feeds/{id}/enqueue-refresh` – enqueue refresh job.
    - `POST /api/v1/feeds/bulk-view` – bulk update the preferred view for multiple feeds（用于“批量移动到某视图”的场景，Body: `{ "feed_ids": [1,2,3], "view": "articles|pictures|videos|audios|social|notifications" }`，返回 `{ "updated": 3 }`）。
  - Categories:
    - `GET /api/v1/categories` – list categories（包含每个 category 的 `view` 字段，作为该组的默认视图偏好）。
    - `GET /api/v1/categories/counters` – unread counters per category (including `null` = uncategorized).
  - Entries & views / timelines:
    - `GET /api/v1/entries` – list entries with filters (`feed_id`, `category_id`, `status`, `view`, `limit`, `offset`)。
      - 视图语义：
        - `view` 是基于订阅属性 `feed.view`（必要时结合 `category.view`）的过滤器；
        - 当 `view=all` 时不做视图过滤；
        - 当 `view=articles` 时匹配默认文章视图订阅（`feed.view = 'articles'`），即传统 RSS 时间线；
        - 其他视图（pictures/videos/...）则按 `feed.view = 'pictures'` 等精确匹配。
      - 建议客户端把 `/api/v1/entries` 视为**统一时间线**接口，配合 `view` 与 `status` 构建类似 Folo 的文章流/图片流/社交流等视图。
    - `GET /api/v1/entries/{id}` – get a single entry.
    - `GET /api/v1/entries/{id}/content?update_content=bool` – fetch and optionally persist full content (readability).
    - `POST /api/v1/entries/{id}/read` – mark entry read/unread (`{ "value": true|false }`).
    - `POST /api/v1/entries/{id}/star` – mark entry starred/unstarred (`{ "value": true|false }`).
    - `POST /api/v1/entries/mark-all-read` – mark entries as read by `feed_id`、`category_id` 或 `view`（三者至少提供其一；`view` 仅作为“按视图全局标记已读”的辅助过滤器）。
    - `GET /api/v1/views` – 列出当前内置的视图类型，用于客户端显示视图切换选项（如“文章视图 / 图片视图 / 视频视图 / 社交流”等）。
    - `GET /api/v1/smart-views` / `POST /api/v1/smart-views` – 列出/创建智能视图（基于 `view + filters` 的“命名时间线”，类似 Folo 的自定义时间线）。
    - `GET/PUT/DELETE /api/v1/smart-views/{id}` – 读取/更新/删除单个智能视图（命名时间线）。
    - `GET /api/v1/smart-views/{id}/entries` – 按智能视图定义的视图 + 过滤条件返回条目列表，相当于带预设参数调用 `/api/v1/entries`。
    - `GET /api/v1/timelines` – 列出所有“时间线”：包括内置视图（kind=`view`）和用户 SmartView（kind=`smart_view`），客户端可以直接用它构建类似 Folo 的侧边栏时间线目录，而无需手动合并 `/views` 与 `/smart-views`。
- Compatibility layers:
  - `/v1/*` is a Miniflux-compatible API surface, intended for Miniflux clients and other third-party readers.
  - `/fever` and `/reader/api/0/*` are intended for Fever / Google Reader compatible clients.
  - First-party clients should prefer `/api/v1/*` for most operations and only use compatibility endpoints when strictly necessary.

## View & Timeline model (概念速览)

这一小节总结 Captura 目前已经存在的与视图相关的几个核心概念，帮助客户端快速对齐心智模型。更详细的说明可以参考 `docs/timeline.md` 和 `docs/data-model.md`。

- **EntryView（视图枚举）**
  - 值：`all | articles | pictures | videos | audios | social | notifications`。
  - 定位：**配置属性**，用于描述“这类内容属于哪种时间线视图”，而不是一个独立接口。
  - 出现位置：
    - `CategoryDto.view`（`GET /api/v1/categories`）。
    - `FeedDto.view`（`GET /api/v1/feeds` / `GET /api/v1/feeds/{id}`）。
    - SmartView 的 `view` 字段（`/api/v1/smart-views`）。
    - `ViewDto.key`、`TimelineDto.view`（`/api/v1/views`、`/api/v1/timelines`）。
  - 配置入口：
    - 分类：`POST/PUT /api/v1/categories` 中的 `view`（禁止 `"all"`）。
    - 订阅：`POST /api/v1/feeds`、`PATCH /api/v1/feeds/{id}`、`POST /api/v1/feeds/bulk-view`。
    - SmartView：`POST/PUT /api/v1/smart-views`。

- **Category.view（分类视图属性）**
  - 表字段：`category.view`（非空字符串）。
  - 语义：该分类下订阅的默认视图，用于在创建订阅或求“有效视图”时作为 fallback。
  - HTTP：
    - `GET /api/v1/categories` 返回 `CategoryDto { id, name, view }`。
    - `POST /api/v1/categories` / `PUT /api/v1/categories/{id}` 用于创建/更新 `view`。

- **Feed.view（订阅视图属性）**
  - 表字段：`feed.view`（可空字符串）。
  - 语义：该订阅的首选视图；当自身缺省时，通过 `EntryView::effective(feed.view, category.view)` 与分类视图一起决定“有效视图”。
  - HTTP：
    - `GET /api/v1/feeds` / `GET /api/v1/feeds/{id}` 以 `FeedDto.view` 暴露一个具体 `EntryView` 值（永不为 null）。
    - `POST /api/v1/feeds` / `PATCH /api/v1/feeds/{id}` 允许设置/更新视图（拒绝 `"all"`）。
    - `POST /api/v1/feeds/bulk-view` 用于“批量移动到某视图”。

- **SmartView.view（命名时间线视图）**
  - 表字段：`smart_view.view`。
  - 语义：该 SmartView 所属的视图（例如“未读图片”属于 `pictures` 视图），用于 UI 分组与 `/api/v1/timelines`。
  - HTTP：
    - `GET /api/v1/smart-views` / `GET /api/v1/smart-views/{id}` 暴露 `view` 字段。

- **TimelineDto（时间线描述）**
  - 由 `/api/v1/timelines` 返回，用于构建侧边栏/时间线目录。
  - 结构：`{ kind: "view"|"smart_view", id: null|number, view: EntryView, name, description?, pinned }`。
  - 用法：
    - kind=`view` → 客户端通常调用 `/api/v1/entries?view=<view>&status=unread`；
    - kind=`smart_view` → 客户端调用 `/api/v1/smart-views/{id}/entries`。

- **TimelineQuery（统一时间线查询模型）**
  - 服务层结构：`captura_service::query::TimelineQuery`。
  - 由 `/api/v1/entries` 和 `/api/v1/smart-views/{id}/entries` 的请求参数映射而来。
  - 关键字段：
    - `view: Option<EntryView>` – 按视图过滤（基于 feed.view/category.view）。
    - `feed_ids[] / category_ids[] / label_ids[]` – 子集过滤。
    - `status: Option<Read|Unread|Starred>` – 条目状态。
    - `search: Option<String>` – 搜索。
    - `sort_by/sort_order/limit/offset/before_id/after_id` – 排序与分页。
  - 语义：
    - `/api/v1/entries` / `/api/v1/smart-views/{id}/entries` 都只是不同的“入口”，最终都会构造一个 `TimelineQuery` 调用 `list_entries_for_user`。
    - `view` 在这里仅作为过滤条件，不会修改任何实体的视图属性（feed/category/smart_view）。

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

## User & Preferences

- `GET /me`
  - Auth required (Bearer)
  - Resp (subset):
    ```jsonc
    {
      "id": 1,
      "username": "alice",
      "is_admin": true,
      "theme": "system_serif",
      "language": "en_US",
      "entries_per_page": 100,
      "entry_sorting_direction": "desc",
      "stylesheet": "",
      "custom_js": "",
      "external_font_hosts": "",
      "keyboard_shortcuts": true,
      "show_reading_time": true,
      "open_external_links_in_new_tab": false,
      "mark_read_on_view": false
    }
    ```
  - Notes:
    - `theme` values mirror Miniflux themes (`system_serif|light_serif|dark_serif`) for compatibility.
    - Preferences are stored in the `user_pref` table as JSON values; missing keys fall back to sensible defaults.

- `PUT /me/prefs`
  - Auth required (Bearer)
  - Body (all fields optional; only provided keys are updated):
    ```jsonc
    {
      "theme": "system_serif|light_serif|dark_serif",
      "language": "en_US",
      "entries_per_page": 50,
      "entry_sorting_direction": "asc|desc",
      "stylesheet": "...",
      "custom_js": "...",
      "external_font_hosts": "fonts.example.com",
      "keyboard_shortcuts": true,
      "show_reading_time": true,
      "open_external_links_in_new_tab": true,
      "mark_read_on_view": false
    }
    ```
  - Effect:
    - For each provided field, upserts a corresponding `user_pref` record (`key` = field name, `value_json` = JSON value).
    - Existing preferences are overwritten; missing fields are left unchanged.
  - Typical usage:
    - WebUI/TUI can call this endpoint to persist settings such as entries per page, sort direction, theme, and reading-time display, while still using cookies for immediate UI response when needed.

## Health

- `GET /healthz`
  - Resp: `ok`

## Feeds

- `POST /feeds` (create)
  - Auth required
  - Body (subset): `{ "type": "rss|atom|json|rule", "feed_url": "...", "category_id": 1, "view?": "articles|pictures|videos|audios|social|notifications", "user_agent": "...", "headers_json": {...}, "cookies": "...", "proxy_url": "...", "fetch_via_proxy": false, "disable_http2": false, "allow_invalid_certs": false, "request_timeout_ms": 15000 }`
  - Resp: `{ "id": 1 }`
  - Note:
    - 当 `view` 为空且 `category_id` 指向的分类有视图偏好时，新建订阅会默认继承该分类的 `view`；
    - 若分类也未设置视图，则默认视为文章视图（`articles`）。
    - 视图是订阅属性：后续 `/api/v1/entries?view=...` 会基于 `feed.view` 进行过滤，而不仅仅是一个前端查询参数。
- `GET /feeds` (list)
  - Query: `category_id?`
  - Resp: `[{ "id": 1, "title": "Example", "feed_url": "...", "site_url": "...", "disabled": false, "category_id": 1, "view": "articles|pictures|videos|audios|social|notifications", "favicon_id?": 123, "error_count": 0, "last_error_message": null }]`
- `GET /feeds/:id` (get)
  - Resp: 单个订阅对象（结构同上）
- `PATCH /feeds/:id` (update)
  - Body (any subset): `{ "title": "...", "category_id": 1, "view?": "articles|pictures|videos|audios|social|notifications", "disabled": false, "user_agent": "...", "headers_json": {...}, "cookies": "...", "proxy_url": "...", "fetch_via_proxy": false, "disable_http2": false, "allow_invalid_certs": false, "request_timeout_ms": 15000 }`
- `DELETE /feeds/:id` (delete)
- `POST /feeds/:id/refresh`
  - Resp: `{ "inserted": 3 }`
- `POST /feeds/bulk-view`
  - Auth required
  - Body: `{ "feed_ids": [1, 2, 3], "view": "articles|pictures|videos|audios|social|notifications" }`
  - Effect: 批量更新这些订阅的 `feed.view`，返回 `{ "updated": <number-of-rows> }`，用于类似 Folo 的“批量移动到某视图”操作。
- `POST /feeds/:id/favicon/refresh`
  - Auth required
  - Effect: tries to fetch `site_url` + `/favicon.ico`, stores to `favicon` table and updates `feed.favicon_id`
  - Resp: `{ "favicon_id": 123, "updated": true }`

## Export (JSON)

- `GET /api/v1/export/full`
  - Auth required
  - Resp: a JSON document capturing the current user's subscriptions, categories and smart views in a Captura-native, view-aware format:
    ```jsonc
    {
      "version": "1",
      "exported_at": "2025-01-01T12:34:56Z",
      "categories": [
        { "id": 1, "name": "Tech", "view": "articles" }
      ],
      "feeds": [
        {
          "id": 1,
          "title": "Example",
          "site_url": "https://example.com",
          "feed_url": "https://example.com/feed.xml",
          "category_id": 1,
          "view": "articles",
          "type": "rss",
          "fetch": {
            "user_agent": "...",
            "headers_json": { "User-Agent": "..." },
            "cookies": "...",
            "proxy_url": null,
            "fetch_via_proxy": false,
            "disable_http2": false,
            "allow_invalid_certs": false,
            "request_timeout_ms": 15000
          },
          "filters": {
            "scraper_rules": null,
            "rewrite_rules": null,
            "blocklist_rules": null,
            "keeplist_rules": null,
            "url_rewrite_rules": null,
            "block_filter_entry_rules": null,
            "keep_filter_entry_rules": null
          }
        }
      ],
      "smart_views": [
        {
          "id": 10,
          "name": "Unread Pictures",
          "view": "pictures",
          "filters": {
            "feed_ids": [1],
            "category_ids": null,
            "label_ids": null,
            "search": null,
            "status": "unread"
          },
          "sort_by": "published_at",
          "sort_order": "desc",
          "pinned": true
        }
      ],
      "labels": [
        {
          "id": 5,
          "name": "Work",
          "color": "#ff8800"
        }
      ],
      "user_prefs": [
        {
          "key": "general.default_view",
          "value": "articles"
        },
        {
          "key": "reading.unread_only",
          "value": true
        }
      ]
    }
    ```
  - 设计目标：
    - 对 Captura 原生客户端友好（无需解析 OPML，就能拿到视图信息、抓取配置以及基础标签/偏好配置）；
    - 只导出与用户订阅结构、视图和 SmartView 定义相关的必要字段，不包含条目内容；
    - 在 `version = "1"` 前提下允许按需扩展字段：老客户端可以忽略新增字段，新客户端可以利用 `labels` / `user_prefs` 做更完整的迁移。

- `POST /api/v1/import/full`
  - Auth required
  - Body: 同 `/api/v1/export/full` 的 JSON 结构（通常直接使用导出的内容）；当前仅支持 `version = "1"`。
  - 语义（按当前用户作用域）：
    - 分类（`categories`）：
      - 按名称匹配已有分类：若存在则更新其 `view`，否则创建新分类；
      - Payload 中的 `id` 仅用于在本次导入过程中重映射 `feeds.category_id` 和 `smart_views.filters.category_ids`。
    - 订阅（`feeds`）：
      - 按 `(user_id, feed_url)` 匹配已有订阅：若存在则更新 `category_id`、`view`、抓取配置（`fetch`）和过滤规则（`filters`），不会改动历史条目；
      - 若不存在则创建新订阅；`view = "all"` 会被自动降级为 `articles`。
    - 智能视图（`smart_views`）：
      - 始终为当前用户创建新的 smart view，不尝试去重；
      - `filters` 中出现的 `feed_ids` / `category_ids` / `label_ids` 会使用本次导入构建的映射表重写为新实例的 ID，无法映射的 ID 会被忽略；
      - `view = "all"` 同样会被降级为 `articles`。
    - 标签（`labels`）：
      - 按名称匹配已有标签：若存在则更新其 `color`，否则创建新标签；
      - Payload 中的 `id` 仅用于在本次导入过程中重映射 `smart_views.filters.label_ids`。
    - 用户偏好（`user_prefs`）：
      - 视为以 `key` 为主键的 KV 存储：若某个 `key` 已存在则覆盖其 `value_json`，否则创建新记录；
      - 导入仅作用于当前用户，不影响服务器全局配置。
  - 该端点不会导入任何条目内容，仅导入结构与视图配置；与 `/v1/export` OPML 兼容导出互不影响。

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
  - Query:
    - 过滤：`feed_id?`, `category_id?`, `status?=read|unread|starred`, `view?=all|articles|pictures|videos|audios|social|notifications`, `q?` / `search?`（等价，推荐使用 `search`）
    - 排序：`sort_by?=published_at|created_at|relevance|id`, `order?=asc|desc`
    - 分页：`limit?`, `offset?`
    - 可选游标：`before_id?`, `after_id?`（对 id 做 `<` / `>` 过滤，配合 `sort_by=id` 可实现基于 id 的 prev/next 导航）
  - Resp: `[{ id, feed_id, url, title, summary, content_html, author, published_at, is_read, is_starred, tags? }]`
  - 统一时间线：该端点已经在服务层映射为一个 `TimelineQuery`，其语义详见 `docs/timeline.md`；其中标签搜索使用 `#tag` 语法，对应于服务层的 label 存储（`label` / `entry_label`），与 SmartView 中显式的 `filters.label_ids` 一致：当存在标签条件时，条目需要至少包含其中一个标签（标签之间为 OR 关系）。
- `POST /entries/bulk-status`
  - Auth required
  - Body: `{ "entry_ids": [1, 2, 3], "status": "read|unread" }`
  - 语义：仅对当前用户拥有的条目生效（通过 feed.user_id 约束），批量将这些条目的 `is_read` 设置为对应布尔值，常用于 WebUI 的“批量标记已读/未读”操作。
- `POST /entries/:id/read` Body: `{ "value": true }`
- `POST /entries/:id/star` Body: `{ "value": true }`
- `POST /entries/:id/save`
  - Auth required
  - Body: `{ "value": true|false }`
  - Semantics:
    - `value = true`：将该条目标记为“已保存”，在 `extras_json` 中写入 `{ "saved": true, "saved_at": "<rfc3339>" }`，并触发保存条目的 webhook / 集成事件（行为与 `/v1/entries/:id/save` 保持一致）。
    - `value = false`：当前实现为清空该条目的 `extras_json`，不触发 webhook / 集成事件。
- `POST /entries/:id/tags`
  - Auth required
  - Body: `{ "tags": ["x", "y"] }`
  - Semantics:
    - 对当前用户：
      - 去空格、去重、过滤空字符串；
      - 为不存在的标签名创建 label（`/api/v1/labels` 的简化变体）；
      - 为每个标签创建 `entry_label` 记录（若不存在），实现“给条目打标签”。
- `DELETE /entries/:id/tags`
  - Auth required
  - Body: `{ "tags": ["x", "y"] }`
  - Semantics:
    - 从当前用户的标签集中找到这些标签名对应的 label id，并从 `entry_label` 中删除与该条目关联的关系。
- `POST /entries/mark-all-read` Body: `{ "feed_id?": 1, "category_id?": 2, "view?": "all|articles|pictures|videos|audios|social|notifications" }`（至少提供 `feed_id`、`category_id`、`view` 之一）

## Views

- `GET /views`
  - Auth required
  - Resp: `[{ "key": "all|articles|pictures|videos|audios|social|notifications", "label": "Articles", "description": "..." }]`
  - 用途：供 WebUI/TUI 等客户端发现内置的视图类型，在 feed/category 设置和条目过滤 UI 中渲染视图列表。

- `GET /views/summary`
  - Auth required
  - Resp: `[{ "view": "articles|pictures|videos|audios|social|notifications", "feed_count": 12, "unread_count": 347 }]`
  - 用途：为侧边栏或设置页提供每个视图下的订阅数量与未读数量汇总，便于构建类似 Folo 的“视图分组 + 未读数”导航。

## Timelines (experimental)

- `GET /timelines`
  - Auth required
  - Resp: 统一的时间线列表，将内置视图和 SmartView 暴露为“时间线”：
    ```jsonc
    [
      {
        "kind": "view",
        "id": null,
        "view": "articles",
        "name": "Articles",
        "description": "Long-form text articles and blog posts",
        "pinned": false
      },
      {
        "kind": "smart_view",
        "id": 10,
        "view": "pictures",
        "name": "Unread Pics",
        "description": null,
        "pinned": true
      }
    ]
    ```
  - 设计目的：
    - 为 first-party 客户端提供一个统一的“时间线目录”，不必分别调用 `/views` 与 `/smart-views` 再手动合并；
    - 当前仅提供元信息（视图类型、名称、是否 pinned），条目列表仍通过 `/api/v1/entries` 或 `/api/v1/smart-views/{id}/entries` 获取；
    - 后续如有需要，可以在此基础上扩展 `/api/v1/timelines/{id}/entries` 的语义 alias。

## Smart views

- `GET /smart-views`
  - Auth required
  - Resp: `[{ "id": 1, "name": "...", "view": "articles", "filters": { "feed_ids": [...], "category_ids": [...], "label_ids": [...], "search": "...", "status": "unread" }, "sort_by": "published_at", "sort_order": "desc", "pinned": true }]`
- `POST /smart-views`
  - Auth required
  - Body: `{ "name": "...", "view": "articles|pictures|videos|audios|social|notifications", "filters": { "feed_ids?": [...], "category_ids?": [...], "label_ids?": [...], "search?": "...", "status?": "read|unread|starred" }, "sort_by?": "published_at|created_at", "sort_order?": "asc|desc", "pinned?": true }`
  - Resp: created smart view object.
- `GET /smart-views/{id}`
  - Auth required
  - Resp: 单个 smart view（同列表中的元素结构）。
- `PUT /smart-views/{id}`
  - Auth required
  - Body: 与 `POST /smart-views` 类似，但所有字段均为可选，用于局部更新。
- `DELETE /smart-views/{id}`
  - Auth required
  - Effect: 删除该智能视图定义，不影响任何条目状态。
- `GET /smart-views/{id}/entries`
  - Auth required
  - Query: `limit?`, `offset?`, `sort_by?=published_at|created_at`, `order?=asc|desc`
  - Resp: 与 `GET /entries` 相同的条目数组，过滤条件由该 smart view 的 `view + filters` 决定。

## Labels

- `GET /labels`
  - Auth required
  - Resp: `[{ "id": 1, "name": "Work", "color": "#ff8800" }, ...]`，返回当前用户下的全部标签，按名称升序。
- `POST /labels`
  - Auth required
  - Body: `{ "name": "Work", "color?": "#ff8800" }`
  - 语义：为当前用户创建一个新标签；同一用户下 `name` 必须唯一（忽略大小写对比由客户端自行决定，服务端按精确字符串匹配）。
- `PUT /labels/{id}`
  - Auth required
  - Body: `{ "name?": "NewName", "color?": "#00aa00" }`
  - 语义：仅允许更新当前用户拥有的标签；如果修改 `name`，同一用户下不得与其它标签重名。
- `DELETE /labels/{id}`
  - Auth required
  - Effect: 删除当前用户下的该标签记录；不影响条目本身，未来如有需要可在实现层面扩展级联删除 `entry_label` 关系。

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
