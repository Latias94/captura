# Integrations

Captura supports two ways to connect with external services:

- Webhooks (Miniflux-compatible): push events to your HTTP endpoint with HMAC-SHA256 signatures.
- Built-in integrations: user-configurable connectors (e.g., Wallabag, Telegram) driven by a reliable background queue.

## Webhooks

- Manage endpoints via `/api/v1/webhooks` (GET/POST/GET/:id/DELETE/:id).
- Events:
  - `new_entries`: emitted after a feed refresh when new entries are discovered.
  - `save_entry`: emitted when an entry is saved via `/v1/entries/:id/save`.
- Security: `X-Miniflux-Signature` (HMAC-SHA256 with your secret) and `X-Miniflux-Event-Type`.
- No retries by design (same as Miniflux). Ensure idempotency on receiver side.

## Built-in Integrations

- Manage connectors via `/api/v1/integrations` (GET/POST/GET/:id/PUT/:id/DELETE/:id).
- Each integration belongs to a user and stores a JSON configuration (`config_json`) and an `enabled` flag.
- Feed-level gating: update a feed with `integrations_json` (object) to enable/disable per feed, e.g.:

```json
{"telegram": {"enabled": true}, "wallabag": {"enabled": false}}
```

- Reliability: all integration events are enqueued as background jobs.
  - Queue: `job_type=integration`, with exponential backoff on failures.
  - Inspect status: `GET /api/v1/integrations/jobs?status=pending|running|done|failed`.

### Wallabag (example)

- Create integration:

```http
POST /api/v1/integrations
{"kind":"wallabag","enabled":true,"config_json":{"base_url":"https://wallabag.example","access_token":"TOKEN"}}
```

- Triggered on `save_entry`. The link is sent to Wallabag API.

### Telegram (example)

- Create integration:

```http
POST /api/v1/integrations
{"kind":"telegram","enabled":true,"config_json":{"bot_token":"123:ABC","chat_id":"-100123"}}
```

- Triggered on `new_entries`. Sends title + link to the specified chat.

### Ntfy (example)

- Create integration:

```http
POST /api/v1/integrations
{"kind":"ntfy","enabled":true,"config_json":{"base_url":"https://ntfy.sh","topic":"mytopic","token":"OPTIONAL"}}
```

- Events: `new_entries` and `save_entry`. Publishes notifications to the topic.

### Slack (Incoming Webhook)

- Create integration (use a Slack Incoming Webhook URL):

```http
POST /api/v1/integrations
{"kind":"slack","enabled":true,"config_json":{"incoming_webhook_url":"https://hooks.slack.com/services/XXX/YYY/ZZZ"}}
```

- Events: `new_entries` and `save_entry`. Sends a simple text message.

### Pocket

- Simple config with existing credentials (you can obtain `access_token` via Pocket's own flow):

```http
POST /api/v1/integrations
{"kind":"pocket","enabled":true,"config_json":{"consumer_key":"CK","access_token":"AT"}}
```

- Event: `save_entry` adds the link to Pocket.

### Instapaper

- Simple config using account credentials:

```http
POST /api/v1/integrations
{"kind":"instapaper","enabled":true,"config_json":{"username":"you@example.com","password":"***"}}
```

- Event: `save_entry` submits the link via Instapaper Simple API.

### Pushover

- Send push messages to your devices:

```http
POST /api/v1/integrations
{"kind":"pushover","enabled":true,"config_json":{"token":"APP_TOKEN","user":"USER_KEY"}}
```

- Event: `new_entries` sends compact notifications (title + url).

### Matrix

- Post messages to a Matrix room:

```http
POST /api/v1/integrations
{"kind":"matrix","enabled":true,"config_json":{"homeserver":"https://matrix.example","access_token":"AT","room_id":"!room:example.org"}}
```

- Event: `new_entries` posts text messages to the room.

## Job Statistics

- Inspect integration jobs:

```http
GET /api/v1/integrations/jobs?status=pending&limit=50
```

- Aggregated stats (near-term): `GET /api/v1/integrations/jobs/stats?window_hours=24`.
