# Captura

Captura is a self-hosted, modern RSS hub written in Rust (axum + SeaORM).
It blends the strengths of Miniflux, FreshRSS, RSSHub and Folo into a single
service: a cozy, batteries-included RSS backend with powerful scraping and a
clean API surface for your own clients.

> Early-stage project – APIs and internals may still change between versions.

## Who is Captura for?

- **Self-hosters** who want a modern, privacy‑respecting RSS backend they can
  run on their own hardware instead of relying on hosted services.
- **Developers** who enjoy building their own clients (Web, mobile, TUI) on top
  of a clean HTTP API and a unified timeline model. If you prefer reading in
  the terminal, you can pair Captura with the Feednest TUI reader:
  <https://github.com/Latias94/feednest>.
- **Heavy RSS users** who subscribe to a mix of “clean” feeds and difficult
  sites, and need a smarter hub that can combine standard feeds, custom rules
  and RSSHub-style routes in one place.
- **Tinkerers and rule authors** who like to capture content from almost any
  site using a combination of Rust handlers, a scraping DSL and optional
  crawler support.

## Features

### Feed reader

- Supports standard feed formats: RSS, Atom and JSON feeds.
- OPML import/export and full export/import for backup and migration.
- Per-user categories, labels and SmartViews.
- Folo-inspired unified timeline: `/api/v1/entries` is the canonical stream,
  everything else (views, SmartViews, timelines) builds on top of it.
- Per-feed and per-category views (`articles`, `pictures`, `videos`, `audios`,
  `social`, `notifications`) that shape how items show up in the timeline.

### Hub routes & rules (RSSHub-inspired)

- Built-in RSSHub-style routes implemented in Rust (`crates/hub`), one route
  per file (e.g. `hn/front`, `github/trending`, `bilibili/*`, `zhihu/hotlist`).
- Flexible Hub metadata (path, params, features, radar) surfaced via
  `/api/v1/hub/routes` and previewable via `/api/v1/hub/preview`.
- First-class scraping DSL (rules v1) for advanced extraction from HTML/JSON,
  backed by `crates/extract` and `crates/pipeline`.
- Local CLI (`captura-cli`) and Web UI helpers for trying rules and Hub routes
  without writing any application code.

### Web UI

- Minimal SSR Web UI (Askama + axum) in `crates/webui`, designed to stay fast
  and lightweight.
- Keyboard-friendly layout and a unified timeline view inspired by Folo.
- SmartViews, labels and feed management supported out of the box.
- Strict Content Security Policy and media proxy for safer reading.

### Privacy, security & anti-bot posture

- HTML sanitization and URL cleaning before storing or rendering content.
- Media proxy to avoid mixed content and reduce tracking.
- ETag / Last-Modified support and conservative scheduling to be polite to
  origin servers.
- Optional crawler path powered by `spider` for tougher sites, used only when
  rules or Hub routes explicitly ask for it.
- Configurable HTTP behaviour via environment variables:
  `CAPTURA_HTTP_USER_AGENT`, `CAPTURA_HTTP_TIMEOUT_MS`, `CAPTURA_HTTP_PROXY`.

### Integrations & compatibility

- Native REST API under `/api/v1/*` with a unified timeline model.
- Miniflux-compatible API under `/v1/*`, plus Fever and Google Reader
  compatibility layers for existing mobile apps.
- Webhooks for `new_entries` and `save_entry` events (HMAC-signed).
- Built-in integrations (Wallabag, Telegram, Ntfy, Slack, Pocket, Instapaper,
  Pushover, Matrix, and more) driven by a durable job queue.

## Authentication

- Local username/password with Argon2 password hashing.
- API tokens (Miniflux-style `X-Auth-Token` and `Authorization: Bearer`).
- Optional Basic auth for compatibility with existing Miniflux clients.
- Optional reverse-proxy authentication (trusted header + auto user creation).
- Optional OpenID Connect / OAuth2 (generic, with room for multiple providers).

## Technical details

Captura is a Cargo workspace composed of small, focused crates:

- `crates/common` – shared error type, identifiers and normalized entry model.
- `crates/storage` – SeaORM database connection and entities.
- `crates/migration` – database migrations.
- `crates/net` – shared HTTP client and HTML helpers.
- `crates/fetcher` – standard RSS/Atom/JSON feed fetching and parsing.
- `crates/crawler` – optional `spider`-based crawler for dynamic/anti-bot sites.
- `crates/extract` – content extraction and rules DSL v1 (YAML/JSON-based).
- `crates/hub` – Hub route metadata and handlers (RSSHub-style routes).
- `crates/pipeline` – orchestrates fetcher/crawler/rules into normalized entries.
- `crates/service` – feed refresh, persistence, search and integration logic.
- `crates/scheduler` – background job queue and backoff-aware workers.
- `crates/api` – axum-based HTTP service (native API + compat layers).
- `crates/webui` – SSR Web UI mounted by the API service.
- `crates/cli` – developer CLI for trying rules and Hub routes.
- `crates/testkit` – test utilities (ephemeral DB, seed helpers).

For a more detailed walkthrough, refer to the architecture and design
notes in the project documentation.

Other technical highlights:

- Written in Rust, using axum for HTTP and SeaORM v2 for persistence.
- Works with PostgreSQL (recommended) or SQLite for quick setups.
- Uses `feed-rs` for feed parsing and `reqwest` for HTTP.
- Background jobs (feed refresh, favicons, integrations) are handled by an
  internal scheduler with host- and user-level concurrency limits.

## Quick start

### Docker (recommended for trying Captura)

You need Docker and Docker Compose v2:

```sh
docker compose build
docker compose up -d
```

- API: http://localhost:8080
- Health: http://localhost:8080/healthz

By default this uses PostgreSQL 18 (`postgres:18-alpine`) and runs database
migrations on startup. You can adjust environment variables and add a
reverse proxy (Traefik, Caddy, Nginx, etc.) as needed.

### Local development (Rust toolchain)

Requirements:

- Rust stable toolchain
- A database:
  - PostgreSQL (recommended), e.g. `postgres://captura:captura@localhost/captura`, or
  - SQLite for quick local experiments (`sqlite://captura.db?mode=rwc`).

Run the API service:

```sh
export DATABASE_URL=sqlite://captura.db?mode=rwc
cargo run -p captura-api
```

Then open http://localhost:8080 in your browser.

The first user can be created via:

- Environment variables (`CAPTURA_ADMIN_USERNAME` / `CAPTURA_ADMIN_PASSWORD`), or
- API: `POST /api/v1/users` followed by `POST /api/v1/auth/login`.

## Configuration (selected)

Some commonly used environment variables:

- `DATABASE_URL` – database connection string (PostgreSQL or SQLite).
- `RUST_LOG` – logging level (e.g. `info`, `debug`).
- HTTP client behaviour:
  - `CAPTURA_HTTP_USER_AGENT`
  - `CAPTURA_HTTP_TIMEOUT_MS`
  - `CAPTURA_HTTP_PROXY`
- Scheduler:
  - `SCHEDULER_ENABLED`
  - `SCHEDULER_ENQUEUE_INTERVAL_SECS`
  - `SCHEDULER_RUNONCE_INTERVAL_SECS`
  - `SCHEDULER_WORKER_CONCURRENCY`, `SCHEDULER_PER_HOST_CONCURRENCY`, etc.
- Auth / security:
  - `CAPTURA_AUTH_PROXY_HEADER`, `CAPTURA_AUTH_PROXY_USER_CREATION`
  - `CAPTURA_OIDC_*` (OIDC client/issuer/redirect/state)
  - `CAPTURA_DISABLE_LOCAL_AUTH`
  - `CAPTURA_SECURITY_HEADERS`, `CAPTURA_REFERRER_POLICY`, `CAPTURA_CSP`

Most options are optional and have reasonable defaults; a minimal setup
only needs `DATABASE_URL`.

## Developer tooling

Some handy commands while developing:

- Run API service: `cargo run -p captura-api`
- Run scheduler logic in tests: `cargo test -p captura-scheduler`
- Try a Hub route from the CLI:

  ```sh
  cargo run -p captura-cli -- hub-try \
    --hub hn/front \
    --limit 5
  ```

- Try a v1 rule against a URL:

  ```sh
  cargo run -p captura-cli -- rule-try \
    --yaml rules/example.yaml \
    --url https://example.com/news \
    --limit 5
  ```

For details on the CLI, see `crates/cli/README.md`.

## Status

Captura is currently early-stage and under active development:

- APIs, database schema and Hub/rules internals may still change.
- Backwards-compatibility is not guaranteed between releases yet.
- Some features documented here are experimental or incomplete.

If you are evaluating Captura for production, please treat it as alpha
software and expect breaking changes between versions.

## License

Captura is distributed under the terms of the MIT license or the Apache
License 2.0, at your option.

See `Cargo.toml` for the workspace-level license declaration.

## Credits

Captura stands on the shoulders of many excellent projects and ideas:

- Miniflux – minimal, opinionated feed reader and API design.
- FreshRSS – self-hosted feed reader and operational experience.
- RSSHub – route model and community-driven scraping patterns.
- Folo – unified timeline and modern reading experience.

All product and project names above are trademarks or registered trademarks
of their respective owners. Captura is an independent project that merely
draws inspiration from their design and user experience.
