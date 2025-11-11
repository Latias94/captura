# Captura Architecture

This document describes the initial architecture and workspace layout for Captura.

## Goals

- Self-hosted RSS service combining: feed aggregation (Miniflux/FreshRSS-like),
  advanced scraping (RSSHub-like), and flexible outputs/APIs.
- Extensible rule directory powered by a simple DSL for community contributions.
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
- `crates/api`: Axum-based HTTP service (REST, compatibility APIs in future). All endpoints are versioned under `/api/v1`.
- `crates/fetcher`: standard feed fetching (HTTP + ETag/IMS) and parsing.
- `crates/crawler`: spider-based adapter for dynamic pages and anti-bot bypass.
- `crates/rules`: DSL schema, validator/linter; rule executor to be added.
- `crates/scheduler`: job scheduler and background workers.
- `crates/pipeline`: Orchestrates `fetcher`/`crawler`/`rules` into normalized entries.
- `rules/`: rule directory (YAML) for community.
- `docs/`: documentation.

## High-Level Architecture

1. Scheduler enqueues jobs for feeds and rule-based sources with host-level budgets.
2. Workers resolve source type:
   - Feed: `fetcher` uses reqwest + feed-rs, leveraging ETag/Last-Modified.
   - Rule: `rules` executes DSL → fetch HTML (basic HTTP) or `crawler` (spider) when needed.
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

## Rules DSL (MVP)

YAML schema (minimal):

```yaml
id: example.site
description: Example site timeline
examples:
  - https://example.com/news
fetch:
  user_agent: captura/0.1
  smart: false
list:
  url: https://example.com/news
  item: article.post
  link: a@href
  title: a.title
  published_at:
    selector: time@datetime
    format: "%Y-%m-%dT%H:%M:%S%z"
content:
  use: css
  selector: "div.article-content, section.content"
  fallback: readability
filters:
  include: [".*"]
  exclude: []
```

Validator ensures required fields and regexes are valid. A linter/test runner (to be added) will fetch sample URLs and snapshot results.

## Crawler (spider)

- Use spider for dynamic pages only. Respect robots, rate limits, and budgets.
- Headless/"smart" features are opt-in (rule-level or automatic when JS challenge is detected).
- Keep the adapter thin to allow replacement/fallback.

## API

- REST for feeds, entries, rules, OPML.
- Compatibility: Fever first, Google Reader next (read-only then extended).
- Health, metrics, and webhooks (later).

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
