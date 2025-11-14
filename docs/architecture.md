# Captura Architecture

This document describes the initial architecture and workspace layout for Captura.

## Goals

- Self-hosted RSS service combining: feed aggregation (Miniflux/FreshRSS-like),
  advanced scraping (RSSHub-like), and flexible outputs/APIs.
- Extensible route layer powered by Rust-defined Hub routes (RSSHub-style) and
  a reusable scraping model (DSL concepts) for community contributions.
- Modular components, easy to evolve and to scale.

## Design Principles

- Separation of concerns: API, storage, fetchers, crawler, rule engine, scheduler.
- Prefer standard feeds first (RSS/Atom/JSON). Use rules + crawler as enhancement.
- Keep headless/"smart" crawling optional and budgeted.
- Posture for security and privacy (sanitization, URL cleaning, media proxy).

## Workspace Layout

- `crates/common`: shared error/result, small utilities.
- `crates/storage`: SeaORM integration, DB connectors, entities (to be added).
- `crates/migration`: SeaORM migration crate for schema evolution.
- `crates/api`: Axum-based HTTP service。
  - 原生 REST：挂载在 `/api/v1/*`
  - 兼容层：默认开箱即用，根路径挂载
    - Miniflux：`/v1/*`
    - Fever：`/fever`
    - Google Reader：`/reader/api/0/*`
- `crates/fetcher`: standard feed fetching (HTTP + ETag/IMS) and parsing.
- `crates/crawler`: spider-based adapter for dynamic pages and anti-bot bypass.
- `crates/rules`: Rules DSL v1 schema and validator/linter (conceptual model +
  legacy YAML support).
- `crates/scheduler`: job scheduler and background workers.
- `crates/pipeline`: Orchestrates `fetcher`/`crawler`/`rules` into normalized entries.
- `crates/hub`: Hub route definitions and metadata (RSSHub-style routes).
- `rules/`: legacy/example DSL rule files (YAML); new official routes live in
  `crates/hub`.
- `docs/`: documentation.

## High-Level Architecture

1. Scheduler enqueues jobs for feeds and route-based sources with host-level budgets.
2. Workers resolve source type:
   - Feed: `fetcher` uses reqwest + feed-rs, leveraging ETag/Last-Modified.
   - Hub route / rule: the rules engine executes a Hub route handler or a DSL
     v1 executor, which in turn fetches HTML/JSON (basic HTTP) or uses
     `crawler` (spider) when needed.
3. Pipeline applies transformations: URL cleaning, rewrite, sanitization, content extraction.
4. Storage persists feeds, entries, rules, jobs via SeaORM. API serves clients.

## Storage (SeaORM v2)

Initial tables (to be refined):
- `user`: local auth/identity.
- `category`: per-user grouping of feeds.
- `feed`: type (rss/atom/json/rule), config (UA, proxy, cookies, HTTP2 disable, allow invalid certs, headers, timeouts), schedule/state (ETag/Last-Modified, error counters), rewrite/filter fields.
- `entry`: feed_id, guid/url, title, summary, content_html, author, published_at, read/starred, extras.
- `enclosure`: entry_id, url, mime, length, kind.
- `label`: per-user labels; `entry_label`: M-N mapping.
- `rule`: logical rule record (id, version, namespace, YAML, examples, verified_at, maintainer).
- `job`: type, status, priority, run_at, attempts, last_error.

SeaORM provides entity-first or migration workflows. During early development we can use entity-first, then solidify migrations for releases.

## Hub Routes and Rules Model

Captura 的内容抓取层正在向“Hub routes + 规则模型”收敛，以更好对齐
RSSHub 的路由处理方式，同时保留一个可复用的抓取 DSL 概念。

- 顶层路由层：使用 Rust 定义的 Hub routes（见 `crates/hub`），每条路由包含：
  - 静态元信息 `RouteMeta`：`hub_id/path/categories/example/parameters/features/radar/name/maintainers/url/description`。
  - 一个实现 `HubHandler` 的异步 handler，用于执行抓取逻辑，返回 `HubData`
   （类似 RSSHub 的 `Data`）。
- 抓取模型层：Rules DSL v1 概念（见 `docs/rules-dsl.md`），描述：
  - HTML list/detail、single-page、JSON API、XPath 抽取、filters/transform 等。
  - 这些概念在代码中通过 Rust 结构和 helper 函数体现，既可以在 Hub handler 内复用，也可以用作 legacy DSL 路径的执行器。

### Rule / Route 类型

当前处于过渡阶段，规则/路由有两种来源：

- **Hub routes（推荐）**：
  - 源码中定义在 `crates/hub`，通过 `RouteMeta + HubHandler` 组合注册（后续使用宏简化）。
  - 官方/社区规则建议以 Hub route 形式贡献，类似 RSSHub 的
    `lib/routes/*/*.ts`。
  - Hub handler 通过 `HubData` 暴露完整 feed 级与条目级信息。

- **DB 规则（legacy / UI 自定义）**：
  - 逻辑记录存于 DB 的 `rule` 表，用于：
    - 早期的 DSL v1 YAML 规则，
    - Web UI/API 创建的用户自定义规则。
  - 核心字段：
    - `rule.rule_id`：逻辑 ID，例如 `captura.route.github.trending`。
    - `rule.version`：模板版本（与 DSL 版本无关）。
    - `rule.yaml`：当前仍是 DSL v1 文本（YAML），由
      `captura_rules::v1::RuleSpecV1` 解释。
    - 运行时元数据（namespace、maintainer、examples 等）。
  - 这些规则通过通用 DSL v1 执行器在 pipeline 中运行，主要用于兼容和用户实验场景。

Hub routes 和 DB 规则将逐步融合：官方/社区贡献使用 Hub route 模式，DB 规则则作为用户配置入口，并在必要时映射到相同的 Hub route/抓取模型之上。

## Crawler (spider)

- Use spider for dynamic pages only. Respect robots, rate limits, and budgets.
- Headless/"smart" features are opt-in (rule-level or automatic when JS challenge is detected).
- Keep the adapter thin to allow replacement/fallback.

## API

- 原生 REST：feeds、entries、rules、OPML、favicons、jobs 等，路径前缀 `/api/v1/*`
- 兼容层（默认启用）：
  - Miniflux：阅读流、分类、订阅源、OPML、Discover、API Keys 等（`/v1/*`）
  - Fever：单端点读写子集（`/fever`）
  - Google Reader：读写子集（`/reader/api/0/*`）
- 健康检查与后续：health、metrics、webhooks（规划）

## Scheduling & Workers

- Tokio-based scheduler with host-level quotas, backoff, retries.
- Separate queues for feed vs. rule jobs to protect resources.
- Optional external worker processes/containers controlled via DB + feature flags.

## Security & Privacy (initial)

- HTML sanitization, media proxy (later), URL trackers stripping, strict headers on responses.
- Minimal surface area for headless by default.

## Milestones

1. Core: entities/migrations, basic fetcher, DSL parser, API skeleton, scheduler stub.
2. Rules: linter, snapshot runner, community submission flow, selective smart crawling.
3. Compatibility & UX: Fever API, media proxy, exports/bridges, metrics.

## Open Points

- Rule plugin runtime (WASM/Lua) — keep a reserved config field; defer implementation.
- RSSHub bridge (fallback when no local rule) — behind a feature flag.
