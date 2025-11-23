# captura-scheduler

`captura-scheduler` is the background job runner for Captura.
It handles feed refreshes, favicon refreshes and integration delivery
using a persistent job table and conservative concurrency limits.

Responsibilities
----------------

- Enqueue jobs for due feeds based on `feed.next_run_at`.
- Execute pending jobs with host- and user-level concurrency limits.
- Apply backoff policies on failures to avoid hammering failing feeds
  or integrations.
- Deliver integration events (new entries, save entry) via the
  `captura-service` integration layer.

Core APIs
---------

- `run_once(db, max)`  
  Fetch up to `max` pending jobs from the database, schedule them
  according to concurrency rules and execute them. Returns the number
  of processed jobs. Intended to be called periodically by the API
  service or an external scheduler.

- `enqueue_due_feeds(db, max)`  
  Scan for feeds whose `next_run_at` is due, and enqueue
  `FeedRefresh` jobs for them (unless a refresh is already pending or
  running). Returns the number of enqueued jobs.

- `enqueue_integration_event(db, user_id, feed_id, payload)`  
  Insert an `Integration` job carrying an `IntegrationEvent` payload.
  Used by `captura-service` when new entries are created or entries are
  saved.

Job types
---------

The scheduler operates on three main job types stored in the `job`
table:

- `FeedRefresh` – refresh a feed and persist new entries via
  `captura-service::refresh_and_persist_by_id`.
- `Favicon` – refresh a feed’s favicon via
  `captura-service::favicon::refresh_for_feed_id`.
- `Integration` – deliver integration events (`NewEntries` or
  `SaveEntry`) via `captura-service::integration::*`.

Concurrency and throttling
--------------------------

`run_once` enforces several limits:

- Global worker concurrency: derived from `std::thread::available_parallelism`
  and overrideable via `SCHEDULER_WORKER_CONCURRENCY`.
- Per-host concurrency for `FeedRefresh` jobs: controlled by
  `SCHEDULER_PER_HOST_CONCURRENCY` (default `2`), based on the feed
  URL host.
- Optional per-user concurrency: `SCHEDULER_PER_USER_CONCURRENCY`
  limits how many jobs a single user can run in one batch.
- SQLite-specific cap: when using a SQLite `DATABASE_URL`, worker
  concurrency is capped by `SCHEDULER_SQLITE_MAX_CONCURRENCY` to reduce
  lock contention.

Backoff behaviour
-----------------

On job failure:

- For `FeedRefresh` jobs, `update_feed_on_failure` increments
  `feed.error_count`, sets `feed.last_error_message` and computes a new
  `next_run_at` using an exponential backoff based on
  `SCHEDULER_BACKOFF_BASE_SECS` and `SCHEDULER_BACKOFF_MAX_SECS`.

- For `Integration` jobs, a similar backoff is applied to the job’s
  `run_at` so that delivery is retried after an increasing delay.

Successful jobs are marked as `Done` with `attempts` incremented; failed
jobs are marked as `Failed` with `last_error` populated.

Configuration
-------------

Environment variables used by the scheduler include:

- `SCHEDULER_WORKER_CONCURRENCY` – maximum number of workers per
  `run_once` batch (default: number of CPU cores, at least 1).
- `SCHEDULER_SQLITE_MAX_CONCURRENCY` – cap for worker concurrency when
  using SQLite (default: 2).
- `SCHEDULER_PER_HOST_CONCURRENCY` – maximum number of concurrent
  refreshes per host (default: 2).
- `SCHEDULER_PER_USER_CONCURRENCY` – optional per-user concurrency
  cap (0 or unset disables this limit).
- `SCHEDULER_BACKOFF_BASE_SECS` – base seconds for backoff (feeds:
  default 300; integration jobs: default 60).
- `SCHEDULER_BACKOFF_MAX_SECS` – maximum backoff delay in seconds
  (feeds: default 3600; integration jobs: default 1800).

In addition, the API service typically controls how often `run_once`
and `enqueue_due_feeds` are called via its own environment variables
(for example, `SCHEDULER_ENQUEUE_INTERVAL_SECS` and
`SCHEDULER_RUNONCE_INTERVAL_SECS`).

How this crate fits in
----------------------

- The API binary (`crates/api`) usually spawns background tasks that
  call `enqueue_due_feeds` and `run_once` on a timer.
- `captura-service` is responsible for business logic (feed refresh,
  persistence, integration emission); the scheduler only orchestrates
  when those actions run.
- This separation makes it possible to replace the scheduler with an
  external worker or a different job runner in the future while
  keeping service-level behaviour unchanged.
