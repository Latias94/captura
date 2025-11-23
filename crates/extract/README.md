# captura-extract

`captura-extract` is the content extraction and rules DSL crate used by
Captura. It focuses on:

- Extracting readable article HTML and titles from web pages.
- Defining and validating the rules DSL v1 schema.
- Executing v1 rules against HTML and JSON sources.

Entry extraction
----------------

The entry-level API is intentionally small and independent of database
models:

- `EntryExtractConfig` – DTO-style config for full-content extraction
  (URL, cookies, basic auth, user-agent, timeout, scraper_rules).
- `fetch_and_extract_entry_dto` – fetch a page according to
  `EntryExtractConfig` and return `ExtractResult`.
- `fetch_and_extract_entry` – convenience helper that takes only a URL.
- `extract_from_html` – run the extraction pipeline on an in-memory HTML
  string, optionally applying Miniflux-style `scraper_rules` (one CSS
  selector per line).

All HTTP fetching in this crate is delegated to `captura-net` so that
User-Agent, timeout and proxy behaviour stay consistent across the
workspace.

Rules DSL v1
------------

The v1 rules model is defined around `RuleSpecV1`:

- `RuleSpecV1` – top-level structure describing list/detail sources,
  JSON sources and XPath-based sources.
- `parse_rule_v1` – parse and validate a YAML string into `RuleSpecV1`.
- `validate_v1` – lightweight structural validation used internally.
- `FiltersSpec` / `TransformSpec` – filter and transform sections used
  to post-process entries (include/exclude, description templates, etc.).

The v1 schema is designed to cover:

- `source.type = list_detail` – HTML list + detail pages.
- `source.type = single_page` – a single HTML page turned into entries.
- `source.type = json` – JSON APIs mapped into entries.
- `source.type = xpath` – XPath-based extraction for more complex HTML.

Stateless JSON rule execution
-----------------------------

For JSON-oriented rules, this crate exposes a small execution API:

- `RuleExecHttpCtx` – HTTP execution context (headers, cookies, proxy,
  timeout, user-agent, smart/respect_robots flags).
- `RuleExecCtx` – combines `RuleExecHttpCtx` with optional runtime
  parameters for a rule.
- `execute_json_v1_stateless` – run a JSON-based v1 rule given a
  `RuleSpecV1` and `RuleExecCtx`, returning normalized entries.

Runtime helpers
---------------

The `v1_runtime` module contains helpers that other crates reuse:

- `extract_html` – HTML extraction primitives used by list/detail rules.
- `json_get_path` – dot-notation accessor for JSON trees.
- `xpath_to_css_like` – small XPath→CSS adapter for common patterns.
- `apply_rule_filters_v1` – apply v1 `entry_include` / `entry_exclude`
  filters to a list of entries.
- `apply_description_template_v1` – apply v1 description templates to
  entries.

Example: basic extraction
-------------------------

```rust
use captura_extract::{fetch_and_extract_entry, ExtractResult};

async fn demo() -> captura_common::Result<()> {
    let ExtractResult { content_html, title } =
        fetch_and_extract_entry("https://example.com/article").await?;
    println!("Title: {:?}", title);
    println!("Content length: {}", content_html.len());
    Ok(())
}
```

In normal usage, higher-level crates such as `captura-pipeline` and
`captura-service` orchestrate this crate together with Hub routes and
storage, so most applications will not need to call it directly.
