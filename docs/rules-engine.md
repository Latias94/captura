# Rules Engine Design

This document describes the design of Captura's content rules engine:

- how Hub routes and the underlying scraping model fit together,
- where routes and rules come from (code, DB, filesystem),
- how scraping, readability and JSON/XPath extraction are composed,
- and how the engine is evolving towards RSSHub-style route handlers.

For the conceptual scraping model (DSL v1), see `docs/rules-dsl.md`.

---

## 1. Goals and Scope

The rules engine is responsible for turning non-standard sources into normalized
feed entries, complementing standard RSS/Atom/JSON feeds.

Design goals:

- **Ergonomic** for route/rule authors:
  - Hub routes defined as Rust modules (metadata + handler),
  - reusable scraping building blocks (DSL v1 concepts) exposed as Rust types
    and helpers.
- **Expressive** enough to cover common scraping patterns (HTML list/detail,
  single-page articles, JSON APIs, XPath-based extraction) and complex
  RSSHub-style routes.
- **Extensible**: difficult sites can drop down to custom Rust handlers instead
  of forcing everything into a declarative DSL.
- **Testable and upgradable**:
  - built-in routes live in code (Git),
  - DB-backed rules support UI-driven and experimental rules,
  - filesystem rules can be synced and snapshot-tested.

Out of scope for the rules engine:

- job scheduling and rate limiting (handled by `scheduler`),
- standard feed parsing (handled by `fetcher`),
- persistence (handled by `service` + `storage`).

---

## 2. Route and Rule Sources

The engine currently supports two complementary layers:

- **Hub routes** – the primary abstraction for built-in/official routes;
- **DB/Filesystem rules** – legacy and UI-created DSL v1 rules.

### 2.1 Hub routes (`crates/hub`)

Hub routes are defined in code and represent RSSHub-style routes:

- Each route is described by:
  - `RouteMeta` – static metadata:
    - `hub_id` (e.g. `"github/trending"`),
    - `path`, `categories`, `example`,
    - `parameters` (name/description pairs),
    - `features` (require_config/anti_crawler/nsfw/etc.),
    - `radar` (source/target hints),
    - `name`, `maintainers`, `url`, `description`.
  - A `HubHandler` implementation:
    - async handler that takes a `HandlerCtx` and returns `HubResult`,
    - `HubResult::Data(HubData)` is analogous to RSSHub's `Data`.
- Built-in route metadata lives in `crates/hub`:
  - e.g. `crates/hub/src/hub/github/trending.rs` defines
    `META_GITHUB_TRENDING`, a `GithubTrendingHandler`, and a
    `RouteRegistration`.
  - Each site module (github/hn/lobsters/zhihu/reuters/medium/bilibili)
    exposes:
    - `ROUTES: &[&RouteMeta]`,
    - `ROUTE_REGISTRATIONS: [RouteRegistration; N]`.
  - `crates/hub/src/hub/registry.rs` exposes:
    - `builtin_route_metas()` for discovery and validation,
    - `builtin_routes()` → `&'static [RouteRegistration]` (meta + handler).
    The pipeline resolves handlers only through `builtin_routes()` and no
    longer hard-codes per-route wiring.
- Hub routes are introspectable and debuggable via the API:
  - `GET /api/v1/hub/routes` – list built-in Hub routes (`RouteMeta`),
  - `GET /api/v1/hub/routes/{namespace}/{name}` – get a single route meta,
  - `POST /api/v1/hub/preview` – execute a Hub route once (by
    `captura_hub://` URL) and return raw `HubData` for inspection.

Hub routes are intended to be the main entrypoint for new official/community
rules, similar to RSSHub's `lib/routes/*`.

### 2.2 DB-backed rules (`rule` table)

At runtime, a DB-backed rule is represented by a record in the `rule` table:

- `id` (primary key, numeric)
- `rule_id` (string, globally unique logical ID, e.g. `captura.route.github.trending`)
- `version` (optional string; used for template versioning, not DSL version)
- `namespace` (optional, usually prefix of `rule_id`, e.g. `captura.route`)
- `description` (optional)
- `yaml` (string, Rules DSL v1 document)
- `examples_json` (optional JSON, derived from `spec.examples`)
- `verified_at`, `maintainer`, timestamps, etc.

The **DSL version** is inside the YAML (`version: 1`) and validated by
`captura_rules::v1::parse_rule_v1` (re-exporting the schema from
`captura-extract::v1`). The `rule.version` column is for template metadata and
does not control parsing.

Rule CRUD and rule templates are exposed via the API in `crates/api/src/rules.rs`:

- `POST /api/v1/rules` – create rule (accepts YAML, validates as DSL v1),
- `GET /api/v1/rules` – list rules,
- `GET /api/v1/rules/{id}` – get a rule,
- `PUT /api/v1/rules/{id}` – update rule YAML,
- `DELETE /api/v1/rules/{id}` – delete rule (if no feeds reference it),
- `GET /api/v1/rules/templates` – list templates by namespace,
- `POST /api/v1/feeds/from-template` – create a feed from a rule template.

### 2.3 Filesystem rules (`rules/` directory)

Repository-managed rules under `rules/` are treated as legacy/example DSL v1
rules:

- `rules/contrib/*.yaml` – historical/community examples stored in Git.
- `rules/local/*.yaml` – local-only rules (not tracked in Git).

Each file is a single DSL v1 YAML document; the `id` inside the YAML is used as
`rule_id` in the DB when imported.

### 2.4 Syncing filesystem rules to DB

The sync logic lives in `crates/service/src/rules_sync.rs`:

- `sync_rules_from_fs(db, root: &Path) -> RulesSyncReport`:
  - recursively scans `root` (typically `rules/`) for `*.yaml` files,
  - parses each file with `parse_rule_v1`,
  - upserts into `rule` table keyed by `rule_id`,
    - missing → `INSERT`,
    - existing → `UPDATE` `namespace/description/yaml/examples_json/updated_at`.
  - collects counters: `scanned_files/created/updated/failed`.

This is exposed via the API:

- `POST /api/v1/rules/sync-from-fs`
  - requires Bearer token authentication,
  - uses `rules_sync::sync_rules_from_fs(&db, Path::new("rules"))`,
  - returns `SyncRulesResp` with the report.

In the current version the sync strategy is intentionally simple (blind upsert).
Later, it can be extended with:

- rule origin (`contrib/local/user_ui`),
- content hash (`source_hash`),
- `user_modified` flag,

to implement “update contrib rules automatically, do not overwrite user-edited
rules” semantics.

---

## 3. Execution Model

### 3.1 Feed types and pipeline entry points

Feeds are typed in `captura_storage::entity::feed` as:

- `FeedType::Rss | Atom | Json` – standard feeds,
- `FeedType::Rule` – rule-based sources.

Pipeline entry points (in `crates/pipeline/src/lib.rs`):

- `refresh_feed_with_meta(feed) -> (Vec<NormalizedEntry>, Option<RefreshMeta>)`
  - for standard feeds: delegates to `captura-fetcher` (HTTP + ETag/Last-Modified
    + feed-rs) and applies URL/content rewrite + DB-level filters.
  - for rule feeds: currently returns `Ok((vec![], None))` and is bypassed in
    favor of the service-layer rule path.

- Service-layer orchestration is in `crates/service/src/lib.rs`:
  - `refresh_and_persist(db, &feed::Model)`:
    - if `feed.type` is `Rule`:
      - loads `rule.yaml` from DB,
      - calls `captura_pipeline::refresh_rule_with_yaml(&feed, &yaml)`,
      - persists resulting `NormalizedEntry`s + enclosures + feed metadata.
    - otherwise delegates to `refresh_feed_with_meta`.

### 3.2 Rule and route backends

Rule-type feeds can be backed by:

- **Hub routes** (for built-in/official routes):
  - For selected built-in routes, the engine already resolves a Hub route and
    executes its Rust handler instead of a pure DSL rule. The current flow is:
    - DB feeds refer to a v1 rule template (`rule_id`, e.g.
      `captura.route.github.trending`);
    - `captura_pipeline::refresh_rule_v1(feed, spec)` first calls
      `handlers::execute_rust_handler_if_any(feed, spec)`;
    - that in turn calls
      `hub_bridge::execute_builtin_hub_for_rule(feed, spec)` which:
      - maps `spec.id` (`captura.route.github.trending`) to a Hub id
        (`github/trending`),
      - merges rule params + `feed.rule_params_json` into a params map,
      - builds a `captura_hub::types::HandlerCtx` with `hub_id` and params,
      - calls the corresponding `HubHandler::handle(&mut ctx)` to obtain
        `HubResult::Data(HubData)`,
      - maps `HubData` into `Vec<NormalizedEntry>` for persistence.
  - As of now, the following built-in routes use this Hub handler path:
    - `github/trending`,
    - `hn/front`,
    - `lobsters/front`,
    - `zhihu/hotlist`,
    - `reuters/top`,
    - `medium/tag`,
    - several `bilibili/*` routes (hot-search, popular, link-news, ranking,
      user/video, bangumi/season, bangumi/media).
  - The goal is for most built-in/official routes to eventually live in this
    layer, with the DSL used as a reusable scraping model underneath.
  - Implementation-wise we distinguish two kinds of Hub routes:
    - **DSL** routes: handlers are thin adapters that build a `RuleSpecV1`
      (often from a built-in template in `crates/hub`) and delegate to the
      DSL executor in `captura-extract`.
    - **HANDLER** routes: handlers implement scraping logic directly in Rust;
      they may still call DSL helpers internally, but from the engine’s
      perspective the route is treated as “handler-backed”.
    Exactly one of these applies to a given route; there is no separate
    “hybrid” type at the data-model level.

- **DSL v1 executor (current default for DB rules)**:
  - `refresh_rule_with_yaml(feed, yaml)`:
    - parses the YAML using `captura_rules::v1::parse_rule_v1`,
    - passes the resulting `RuleSpecV1` into `refresh_rule_v1(feed, &spec)`.
  - `refresh_rule_v1(feed, &spec)`:
    - may first try a Rust handler if one is registered for the given `spec.id`,
      returning handler-produced entries when present;
    - otherwise dispatches on `spec.source.type`:
      - `list_detail` → `execute_list_detail_v1`,
      - `single_page` → `execute_single_page_v1`,
      - `json` → the stateless JSON executor in `captura-extract`
        (`execute_json_v1_stateless`),
      - `xpath` → `execute_xpath_v1` (subset implementation).
    - applies DSL-level filters:
      - `filters.entry_include / entry_exclude` via `apply_rule_filters_v1`,
      - `filters.fetch_full_content_when` via `apply_full_content_when_v1`:
        - triggers extra per-entry fetch and extraction when conditions match,
        - merges full content according to `transform.content_merge.mode`.
    - applies feed-level entry filters (`keep_filter_entry_rules` etc.).

Over time, built-in routes will move to the Hub route layer, while DSL v1 will
remain as a reusable scraping model and for user-defined/legacy rules.

---

## 4. Rules DSL v1 Execution

The normative schema and examples are described in `docs/rules-dsl.md`. This
section focuses on how the engine actually executes a v1 rule.

### 4.1 Parameter resolution

Before executing a rule, parameter defaults and feed-level overrides are merged:

- Rule-level defaults: `spec.params.defaults` (JSON object),
- Feed-level overrides: `feed.rule_params_json` (JSON object).

`merge_rule_params_v1` produces a single JSON map where feed params override
rule defaults. Interpolation is string-based:

- `:name` and `{name}` placeholders inside URLs and other strings are replaced
  with the corresponding parameter value (see `render_with_params`).

### 4.2 Common fetch strategy

Most v1 executors use a shared helper to fetch HTML:

- `FetchCfg` in `crates/pipeline/src/lib.rs` captures:
  - `user_agent`, `headers`, `smart`, `timeout_ms`, `respect_robots`,
    `delay_ms`, `limit`, `proxy_url`.
- `fetch_html_strategy`:
  - decides whether to use `captura-crawler::fetch_html` (spider/smart) or a
    plain `reqwest` HTTP client,
  - respects per-feed proxy and invalid-cert settings,
  - applies timeouts and simple fallbacks.

This helper is reused by:

- `execute_list_detail_v1`,
- `execute_single_page_v1`,
- `execute_xpath_v1`,
- Readability-based helpers and some Hub utilities.

JSON rules executed via `captura-extract::execute_json_v1_stateless` use a
separate, HTTP-only helper because they are designed to be DB-agnostic.

### 4.3 `type: list_detail`

Common pattern for news/blog listings:

1. Use `source.list.request` + merged params to build the list URL.
2. Fetch list HTML via `fetch_html_strategy`.
3. Parse DOM with `scraper::Html` and `Selector`:
   - `list.item` – CSS selector for each item block,
   - `list.link` – shorthand `css@attr` within the item,
   - `list.title` – shorthand for title text within the item,
   - `list.summary` – shorthand for summary text.
4. For each item, compute the detail URL (`absolutize(list_url, href)`).
5. Extract full content according to `source.content`:
   - `mode = readability`:
     - use the dom_smoothie-based readability engine from `captura-extract`
       (invoked via the pipeline’s `readability_like_strategy_async` helper),
     - fall back to simple heuristics and finally full page HTML, logging any
       failures.
   - `mode = css | json_fragment`:
     - use `fetch_and_select_strategy` to fetch and select `content.selector`
       from the detail page.
6. Build `NormalizedEntry`:
   - `guid`/`url`: the detail URL,
   - `title`, `summary`, `content_html`: from the steps above.

Historically, list-detail rules were the main way to express RSSHub-style HTML
routes such as GitHub Trending and Hacker News front page. Today:

- user-defined and many contrib rules still prefer `list_detail` for HTML
  listings;
- several built-in routes (e.g. GitHub Trending, HN front, Lobsters front) are
  implemented as Hub handlers but have equivalent DSL templates for reference
  and fallback.

### 4.4 `type: single_page`

For sources where a single URL corresponds to a single entry:

1. Build URL from `source.request.url` + params.
2. Fetch HTML via `fetch_html_strategy`.
3. Extract page `<title>` for entry title.
4. Extract content via `source.content`:
   - `mode = readability` → dom_smoothie,
   - `mode = css` → `fetch_and_select_strategy` with `content.selector`.
5. Build a single `NormalizedEntry` with URL as both `guid` and `url`.

### 4.5 `type: json`

For JSON APIs and JSON embedded in HTML:

1. Determine JSON root:
   - If `source.from_html` is present:
     - fetch base HTML using `from_html.request` or `source.request`,
     - select node(s) with `from_html.selector` (CSS),
     - read each node’s text content and parse as JSON,
     - if `multiple = false` → take first valid JSON document,
     - if `multiple = true` → aggregate into array.
   - Else:
     - fetch JSON directly using `source.request.url`,
     - parse as `serde_json::Value`.
2. Navigate to the array at `source.root` using dot-notation helper
   `json_get_path`.
3. For each item in the array, apply mapping:
   - `mapping.title`, `url`, `summary`, `content_html`, `author`:
     - dot-notation paths, extracted as strings,
   - `mapping.enclosure`:
     - `url/type/length` paths mapping to enclosure fields,
   - timestamp mapping can be extended later (schema already has
     `JsonTimestampMapping`).
4. Build `NormalizedEntry` for each item; apply DSL filters afterwards.

### 4.6 `type: xpath` (HTML/XML XPath subset)

For more structured HTML/XML sources, v1 provides an XPath-based schema:

- `source.request` – URL and fetch options (same as other types),
- `source.xpath`:
  - `item` – XPath selecting the item nodes,
  - `title` – XPath for title relative to item,
  - `url` – XPath for link,
  - `content_html` – XPath for content,
  - `published_at.expr/path/format` – reserved for future use.

Current implementation is a **pragmatic subset**:

- Uses a small converter `xpath_to_css_like` to map common XPath patterns to
  CSS-like expressions:
  - `//ul/li` → `ul li`,
  - `.//h2/text()` → `h2`,
  - `.//a/@href` → `a@href`,
  - `div[@class='entry-content']` → `div.entry-content`.
- Execution then reuses the same `scraper`-based CSS selection helpers:
  - `extract_text` for text,
  - `extract_attr` for attributes,
  - `extract_html` for inner HTML per matched node.

This is intentionally not a full XPath 1.0 engine like FreshRSS’s DOMXPath
integration; it focuses on the most common patterns and can be extended later.

---

## 5. Readability and Full-Content Fetch

### 5.1 dom_smoothie integration

`dom_smoothie` is used as the primary Readability implementation inside
`captura-extract`:

- `captura_extract::extract_from_html(html, url, scraper_rules)`:
  - wraps `dom_smoothie::Readability::new` with a default config,
  - returns an `ExtractResult` with `content_html` and optional `title`,
  - falls back to simple heuristics and finally full page HTML.

It is used in two main paths:

- Rule-level readability:
  - `content.mode = readability` in `list_detail` and `single_page` rules,
    via the pipeline helper `readability_like_strategy_async`.
- Feed-level full-content fetching (Miniflux-like):
  - `captura_pipeline::extractor::fetch_and_extract_entry`:
    - first apply `feed.scraper_rules` if present,
    - otherwise try dom_smoothie (log a warning on failure),
    - fall back to simple heuristics and full page HTML.

All dom_smoothie failures are logged with `tracing::warn`/`debug` and do not
abort the rule; the engine always has a fallback.

### 5.2 Conditional full-content fetch

Rules DSL v1 allows rules to request full content only for certain entries:

- `filters.fetch_full_content_when`:
  - list of `{ field: "title|summary|content_html", regex }`.
- `transform.content_merge.mode`:
  - `replace | prepend | append` (how to combine new full content with existing
    summary/content).

`apply_full_content_when_v1` implements this:

1. Compile conditions into matchers (field + regex).
2. For each entry:
   - check whether any condition matches,
   - if so, and entry has a URL, call
     `extractor::fetch_and_extract_entry(url, feed)` to fetch full content.
3. Merge `extract_result.content_html` into `entry.content_html` according to
   `content_merge.mode`, and optionally fill missing `title`.

This provides an engine-level analogue to FreshRSS’s “Full-Text” actions and
Miniflux’s “scraper_rules + full-content” feature, but expressed declaratively
in the rule.

---

## 6. Rust Handlers and External Backends

Rules DSL v1 covers a wide range of scraping scenarios, but some sites are
better handled with custom code (multi-step flows, GraphQL, complicated
post-processing, login/session handling). To support this, the engine now
includes Rust-based handlers and is designed to evolve further:

- **Hub-based Rust handlers (current state)**:
  - The `captura_hub::types::HubHandler` trait and `HandlerCtx` provide a
    typed interface for per-route logic, inspired by RSSHub’s `handler(ctx)`:
    - handlers receive a logical `hub_id` and a params map,
    - they typically use helpers in `crates/hub/src/hub/util.rs` (e.g.
      `get_html`, `for_each_element`, `extract_text`, `extract_attr`) for HTML
      fetching and parsing. Older handlers still use
      `captura-pipeline::hub_utils`, which reuses `fetch_html_strategy` and
      shared HTTP/crawler configuration; these are being migrated into the Hub
      crate.
  - `captura_pipeline::handlers::execute_rust_handler_if_any` and
    `hub_bridge::execute_builtin_hub_for_rule` connect v1 rule templates
    (`spec.id = "captura.route.*"`) to Hub handlers (`hub_id = "*/"*`).
  - This path currently powers the built-in routes listed in §3.2.

- **Future: more generic Rust rule backends**:
  - Beyond Hub routes, the rule spec can grow a `backend` block to map a v1
    rule to a named Rust handler (or handler module) without going through the
    Hub route registry.
  - A more generic `RustRuleHandler` trait could expose:
    - the feed model,
    - merged rule parameters,
    - a high-level HTTP client,
    - a cache interface,
    - time and logging utilities.

- **Future: external HTTP handlers**:
  - Delegating rule execution to an external service that speaks a simple JSON
    protocol remains a planned extension, enabling private scrapers and
    out-of-process integrations.

These backends coexist with the DSL v1 executor. Simple and moderate sites
remain DSL-only; complex sites can opt into a handler backend. Since Captura is
not yet bound by backward compatibility, the engine can continue to move logic
from DSL-only implementations to Hub/Rust handlers over time.

---

## 7. Summary

- The rules engine exposes **Hub routes** for built-in/official routes and
  uses **Rules DSL v1** (see `docs/rules-dsl.md`) as a reusable scraping model
  and for DB/filesystem-backed rules.
- Hub routes are defined as Rust modules in `crates/hub` (metadata +
  `HubHandler`), inspired by RSSHub's route design.
- DB rules (`rule` table) and filesystem rules (`rules/` directory) provide a
  path for UI-created and legacy DSL v1 rules; they execute via the v1 executor
  and share common helpers with Hub handlers.
- Readability is powered by `dom_smoothie`, with logged fallbacks to simpler
  heuristics and full page HTML.
- The engine already covers most Miniflux/FreshRSS-like scenarios and a good
  portion of RSSHub-style HTML routes. Ongoing work focuses on migrating
  built-in rules to Hub routes and expanding handler capabilities while keeping
  XPath support available and extensible.
