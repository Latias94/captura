# Unified Timeline Model (Folo-inspired)

This document describes the unified timeline query model used by Captura,
inspired by Folo's “one global timeline + views + named timelines” design.

The goal is to make all entry listing surfaces (global `/entries`, SmartViews,
future timeline endpoints, and compatibility layers) reuse the same underlying
query semantics while remaining view-aware.

## Concepts

### EntryView (视图枚举)

- Logical view of entries (`captura_types::EntryView`):
  - `all` – do not apply any view-based filtering.
  - `articles` – traditional article-centric timeline (default).
  - `pictures` – image-heavy timeline (comics/galleries).
  - `videos` – video-centric timeline.
  - `audios` – audio/podcast timeline.
  - `social` – short-form/social feeds.
  - `notifications` – alert/status feeds.
- EntryView is a **configuration attribute**, not a query API:
  - it is stored on entities (categories/feeds/smart views);
  - read-side queries (timeline/entries) only *use* it for filtering.

### Entity-level view attributes

- **Category view** (`category.view`):
  - Non-null string column storing a concrete EntryView (e.g. `"articles"`).
  - Semantics: *default view* for feeds in this category.
  - HTTP surface:
    - `GET /api/v1/categories` → `CategoryDto { id, name, view }`.
    - `POST /api/v1/categories` → accepts optional `view`; defaults to `articles`.
    - `PUT /api/v1/categories/{id}` → may update `view` (except `all`, which is rejected).

- **Feed view** (`feed.view`):
  - Nullable string column storing a concrete EntryView when set.
  - Semantics: preferred view for this subscription.
  - When both category and feed views exist, the effective view is:
    - `EntryView::effective(feed.view, category.view)`:
      - feed view wins when valid;
      - otherwise fallback to category view;
      - otherwise fallback to `articles`.
  - HTTP surface:
    - `GET /api/v1/feeds` and `GET /api/v1/feeds/{id}` return `FeedDto.view`
      as a concrete `EntryView` on the wire.
    - `POST /api/v1/feeds` accepts optional `view`; when omitted, the server
      uses the category's view (or `articles`).
    - `PATCH /api/v1/feeds/{id}` accepts optional `view` to update
      this attribute (rejects `"all"`).
    - `POST /api/v1/feeds/bulk-view`:
      - body: `{ "feed_ids": [1,2,3], "view": "articles|pictures|..." }`;
      - semantics: bulk update `feed.view` for the given feed ids.

- **SmartView view** (`smart_view.view`):
  - String field storing the logical view of the SmartView itself.
  - Semantics: tells clients which EntryView this named timeline belongs to
    (e.g. a pictures-only unread SmartView).
  - HTTP surface:
    - `GET /api/v1/smart-views` and `GET /api/v1/smart-views/{id}` expose
      the `view` field as `EntryView` on the wire.
    - A SmartView is essentially: `view + filters + sort prefs + pinned`.

### Timeline descriptor

- **Timeline** – a logical timeline descriptor used by `/api/v1/timelines`:
  - built-in views (kind=`view`) and SmartViews (kind=`smart_view`) merged
    into a single list for sidebars and navigation.
  - HTTP type: `captura_types::TimelineDto`:
    - `kind`: `"view"` or `"smart_view"`;
    - `id`: `null` for built-in views, SmartView id otherwise;
    - `view`: `EntryView` (articles/pictures/...);
    - `name`: human-friendly label;
    - `description?`: optional description;
    - `pinned`: whether highlighted (for SmartViews).

From a client perspective:

- `/api/v1/entries` is the canonical **global timeline** endpoint;
- `/api/v1/smart-views/{id}/entries` are **named timelines**;
- `/api/v1/timelines` exposes these timelines as a single directory.

## HTTP Surfaces

### `GET /api/v1/entries`

Global, view-aware timeline endpoint.

- Query parameters:
  - `feed_id?` – restrict to a single feed.
  - `category_id?` – restrict to a single category.
  - `status?=read|unread|starred` – entry status filter.
  - `view?=all|articles|pictures|videos|audios|social|notifications` – view
    filter (see semantics below).
  - `q?` / `search?` – search query (same syntax; `search` preferred).
  - `sort_by?=published_at|created_at|relevance` – optional sort key.
  - `order?=asc|desc` – optional sort order (default `desc`).
  - `limit?` – max items (default 100).
  - `offset?` – offset (default 0).
- View semantics:
  - `view=all` – no view filtering.
  - `view=articles` – match feeds where `feed.view IS NULL OR feed.view='articles'`.
  - other views – exact match on `feed.view = '<view>'` (e.g. pictures/videos).
- Search semantics:
  - Query syntax supports:
    - `title:"foo bar"` / `author:alice` / `url:example.com`
    - tags: `#security`, `#rust`
    - remaining text as general query.
  - PostgreSQL:
    - general query uses `entry.tsv` + `websearch_to_tsquery('simple', ...)`；
    - field queries use `to_tsvector` on specific columns；
    - tags use an `EXISTS` subquery against `label` via `entry_label`.
  - Non-Postgres:
    - general query falls back to `LIKE` over `title/summary/content_html`;
    - tags use a `LIKE`-based `EXISTS` subquery.
- Sorting:
  - default (no search): `published_at desc, created_at desc`;
  - `sort_by=created_at`: `created_at` asc/desc;
  - Postgres + search:
    - when `sort_by` is not explicitly set, default to:
      - `relevance desc, published_at desc, created_at desc`;
    - when `sort_by=relevance`, same as above with explicit control;
    - other `sort_by` values override relevance-based ordering.

Clients should treat `/api/v1/entries` as the **backbone timeline** and
use `view` + `status` + filters to implement UI tabs similar to Folo's
timeline categories.

### `GET /api/v1/smart-views/{id}/entries`

Named timeline endpoint based on stored SmartView definition.

- SmartView fields (see `captura_types::SmartViewDto`):
  - `view` – `EntryView` for this timeline;
  - `filters` (`SmartViewFiltersDto`):
    - `feed_ids?`: subset of feeds;
    - `category_ids?`: subset of categories;
    - `label_ids?`: subset of labels;
    - `search?`: search query string;
    - `status?`: `"read"|"unread"|"starred"`;
  - `sort_by?`: `"published_at"|"created_at"`;
  - `sort_order?`: `"asc"|"desc"`;
  - `pinned`: whether highlighted in UI.
- Query parameters:
  - `limit?`, `offset?` – same as `/api/v1/entries`;
  - `sort_by?`, `sort_order?` – override stored SmartView sort preferences
    when provided.
- Semantics:
  - The server combines:
    - SmartView's `view` + `filters` + stored sort preferences;
    - query overrides (`sort_by/sort_order/limit/offset`);
  - into a unified timeline query and returns entries using the same
    semantics as `/api/v1/entries`.

  - Label filter behavior:
    - when `filters.label_ids` is non-empty, entries must have at least one
      of the specified labels (logical OR across labels);
    - the `view` still applies first: only feeds whose effective view
      matches the SmartView `view` are considered, and label filtering
      is evaluated within that subset;
    - this matches the behaviour of `search` tag filters (`#tag`) at the
      service layer, but uses explicit label ids instead of names.

In other words, a SmartView is a **saved timeline query** and this endpoint
is equivalent to calling `/api/v1/entries` with pre-filled parameters.

### `GET /api/v1/timelines`

Metadata-only endpoint listing all timelines for the current user.

- Response: `TimelineDto[]` (see `captura_types::TimelineDto`), each item:
  - `kind`: `"view"` or `"smart_view"`;
  - `id`: `null` for built-in views, SmartView id otherwise;
  - `view`: `EntryView` used by this timeline;
  - `name`: human-friendly label;
  - `description?`: optional description;
  - `pinned`: whether highlighted (for SmartViews).
- Usage pattern:
  - For built-in view timelines:
    - call `/api/v1/entries?view=<view>&status=unread` for unread timeline;
  - For SmartView timelines:
    - call `/api/v1/smart-views/{id}/entries`.

This mirrors Folo's “timeline directory” concept: one list of timelines for
sidebars, while actual items come from the timeline entry endpoints.

## Internal TimelineQuery (service layer)

The `captura-service` crate defines a unified `TimelineQuery` struct used by
all timeline-style queries (`crates/service/src/query.rs:260`):

- Fields:
  - `view: Option<EntryView>` – view filter (same semantics as above).
  - `feed_ids: Vec<i64>` – restrict to these feeds when non-empty.
  - `category_ids: Vec<i64>` – restrict to these categories when non-empty.
  - `label_ids: Vec<i64>` – require entries to have at least one of these labels.
  - `status: Option<TimelineStatus>` – `Read | Unread | Starred`.
  - `search: Option<String>` – search query string (same syntax as HTTP).
  - `sort_by: Option<String>` – `published_at | created_at | relevance | id`.
  - `sort_order: Option<String>` – `asc | desc`.
  - `limit: u64` – number of items to return (server-side clamped).
  - `offset: u64` – offset for simple paging.
  - `before_id: Option<i64>` – optional id-based upper bound (`entry.id < before_id`).
  - `after_id: Option<i64>` – optional id-based lower bound (`entry.id > after_id`).

The main read function is:

- `list_entries_for_user(db, user_id, &TimelineQuery) -> Vec<entry::Model>`
  - joins `entry` with `feed` and scopes to `feed.user_id = user_id`;
  - applies view/feed/category/label/status filters;
  - applies search and sorting as described above;
  - returns a list of entries, which API layers map to `EntryDto`.

Current HTTP mapping:

- `/api/v1/entries`:
  - builds a `TimelineQuery` from query parameters and delegates to
    `list_entries_for_user`.
- `/api/v1/smart-views/{id}/entries`:
  - loads `smart_view` row, builds a `TimelineQuery` from its view/filters
    plus request overrides, then calls `list_entries_for_user`.

Future timeline endpoints (for example `/api/v1/timelines/{kind}/{id}/entries`)
should follow the same pattern: resolve timeline metadata → build a
`TimelineQuery` → call `list_entries_for_user`.
