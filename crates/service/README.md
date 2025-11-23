# captura-service

`captura-service` is the application service layer for Captura.
It encapsulates feed refresh, persistence, search, webhooks and
integration logic so that HTTP layers (axum handlers, compat APIs)
can stay thin and focus on transport concerns.

Responsibilities
----------------

- Refresh feeds and persist new entries, including enclosures and
  integration jobs.
- Provide entry-level operations (full-content extraction, saved flag,
  tags).
- Expose read-side queries and counters for feeds, categories and
  timelines.
- Handle webhooks and built-in integrations on the producer side
  (scheduler is the consumer).
- Manage favicons for feeds.

Core APIs
---------

Feed refresh and persistence (lib.rs):

- `refresh_and_persist_by_id(db, feed_id)`  
  Load a feed by id, refresh it using `captura-pipeline`, persist new
  entries and update feed metadata. Returns the number of inserted
  entries.

- `refresh_and_persist(db, &feed_model)`  
  Same as above but takes a `feed::Model` directly. Used internally by
  the scheduler and some API paths.

Both functions:

- Avoid inserting duplicate entries based on GUID.
- Persist enclosures.
- Update feed metadata (checked_at, error counters, ETag, Last-Modified).
- Enqueue integration jobs and fire webhooks for new entries.

Entry operations (entries.rs):

- `get_entry_content(db, entry, update_content)`  
  Fetch full content for an entry using the pipeline extractor and the
  entry’s parent feed configuration. Optionally persists the new
  `content_html` and title when `update_content = true`. Returns an
  `EntryContentDto` for API layers to serialize.

- `set_entry_saved(db, entry, value)`  
  Set or clear the “saved” flag on an entry by updating its `extras`
  JSON and timestamps.

- `add_tags_to_entry(db, user_id, entry, tags)`  
  Normalize tag names, create missing labels for the user and link them
  to the entry.

- `remove_tags_from_entry(db, user_id, entry, tags)`  
  Remove specific labels from the entry for a given user.

Queries and counters (query.rs, search.rs):

- `feed_counters_for_user(db, user_id)`  
  Compute read/unread counters per feed.

- `category_unread_counters_for_user(db, user_id)`  
  Compute unread counters per category (including “uncategorized”).

- Timeline/query helpers  
  Types like `EntryQueryFilter` and helpers such as
  `view_filter_condition` build SeaORM conditions that both native
  `/api/v1` and Miniflux-compatible `/v1` endpoints can share.

Search-related helpers live alongside queries so that both APIs reuse
the same search semantics and sorting behaviour.

Webhooks and integrations
-------------------------

Webhooks (webhook.rs + integration.rs):

- `emit_new_entries(db, user_id, feed, entry_ids)`  
  Build a Miniflux-compatible payload for `new_entries` and send it to
  all enabled webhooks for the user.

- `emit_save_entry(db, user_id, entry)`  
  Build and send a `save_entry` payload when the user saves an entry.

Built-in integrations (integration.rs):

- `emit_new_entries(db, user_id, feed, entry_ids)`  
  Build integration payloads for new entries and delegate them to the
  scheduler via `IntegrationEvent::NewEntries`.

- `emit_save_entry(db, user_id, entry)`  
  Same for `save_entry` events.

Delivery to external services is performed asynchronously by
`captura-scheduler` based on the `job` table; this crate focuses on
serializing events and enqueuing jobs.

Favicons
--------

Favicon-related logic lives in `favicon.rs`:

- `refresh_for_feed_id(db, feed_id)`  
  Fetch and store a favicon for the given feed, updating the
  `favicon_id` on success.

This is used by both API endpoints and scheduler jobs.

How this crate fits in
----------------------

- The axum API crate (`crates/api`) calls into `captura-service`
  functions to implement its handlers.
- The scheduler crate (`crates/scheduler`) uses `refresh_and_persist*`
  and integration helpers to process background jobs.
- By keeping business logic here, other frontends or tools can reuse
  the same behaviour without duplicating SQL or pipeline wiring.
