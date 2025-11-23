# captura-cli

`captura-cli` is a small developer companion tool for Captura.
It helps you iterate on rules and Hub routes from the command line,
without needing to run the full API service or Web UI.

## Commands

The binary exposes two main subcommands:

- `rule-try` – run a v1 rule (YAML) against a list URL.
- `hub-try` – run a built-in Hub route by `hub_id`.

Both commands log a short, human-readable summary so that you can see
quickly whether a rule or route behaves as expected.

### `rule-try`

Run a rules DSL v1 file against a target URL. This mirrors the
`/api/v1/rules/try` endpoint.

```sh
cargo run -p captura-cli -- rule-try \
  --yaml rules/example.yaml \
  --url https://example.com/news \
  --limit 5
```

Options:

- `--yaml` – path to the rule YAML file (use `-` to read from stdin).
- `--url` – list page URL to fetch; overrides `source.list.request.url`.
- `--proxy` – optional HTTP proxy URL (e.g. `http://127.0.0.1:10809`).
- `--limit` – maximum number of entries to print (default: 5).

Internally, `rule-try`:

1. Parses the YAML into `RuleSpecV1` using `captura-hub`/`captura-extract`.
2. Builds a temporary `feed::Model` with type `Rule`.
3. Executes the rule via `captura_pipeline::refresh_rule_v1`.
4. Prints a summary: rule id, URL, proxy, total items and the first few titles.

This makes it easy to iterate on selectors and rule parameters before
persisting a rule via the API.

### `hub-try`

Run a built-in Hub route (RSSHub-style) by its `hub_id`.

```sh
cargo run -p captura-cli -- hub-try \
  --hub hn/front \
  --params '{"section":"news","view":"sources"}' \
  --limit 5
```

Options:

- `--hub` – hub id, such as `hn/front` or `v2ex/topics`.
- `--params` – optional JSON object with route parameters.
- `--limit` – maximum number of items to print (default: 5).

Internally, `hub-try` calls
`captura_pipeline::execute_hub_route(hub_id, &params)` and prints the
resulting `HubData` summary (title/link/items).

## Logging

The CLI uses `RUST_LOG` or the `--log` flag for its log level, for example:

```sh
cargo run -p captura-cli -- --log debug hub-try --hub hn/front
```

This is useful when debugging HTTP issues, crawler behaviour or rule
execution.

## Where to go next

For a deeper understanding of the rules DSL, Hub routes and the refresh
pipeline, consult the project documentation or browse the corresponding
crates (`crates/extract`, `crates/hub`, `crates/pipeline`).
