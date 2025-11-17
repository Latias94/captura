# Rules DSL v1 Specification

This document describes the **official v1** rule DSL for Captura.
It is designed to absorb practices from FreshRSS, Miniflux and RSSHub, while
remaining ergonomic for rule authors and stable for the engine.

The previous minimal DSL used before this refactor is considered **legacy**
and will be replaced by this v1 design.

---

## Quick Start (Minimal Rules)

For most simple sites (HTML list page + article detail page), you only need a
small subset of this DSL.

### Minimal `list_detail` rule

The following fields are typically enough:

- `id`, `version`
- `source.type: list_detail`
- `source.list.request.url`
- `source.list.item`, `source.list.link`, `source.list.title`
- `source.content.mode`, `source.content.selector`

Example:

```yaml
id: captura.example.news
version: 1
description: Example news list
examples:
  - https://example.com/news

source:
  type: list_detail

  list:
    request:
      url: "https://example.com/news"
    item: "article.post"
    link: "a@href"
    title: "a.title"

  content:
    mode: css
    selector: "div.article-content"
```

This is the “minimal mental model” for rule authors:

- grab the list: `list.request.url` + `list.item`
- find per‑item link and title: `link`, `title`
- extract full content from the detail page: `content.selector`

Everything else in this document is **optional** and intended for advanced use
cases:

- `match`, `params`, `filters`, `transform`
- `source.type: json`, `xpath`, `from_html`
- conditional full‑content fetch and merge strategies.

You can safely start with the minimal subset above and only adopt advanced
blocks when needed.

---

## 1. Design Goals

- **Ergonomic**: Most rules should be writable with only CSS selectors and a few
  URLs, without programming.
- **Expressive**: Cover the common cases:
  - HTML list + detail pages (news/blog listings).
  - Single-page sources.
  - JSON APIs turned into feeds.
  - HTML/XML + XPath, for complex but stable layouts.
- **Extensible**: Future additions (JSONPath, XPath extensions, script plugins)
  should not break existing rules.
- **Testable**: Every rule must be runnable in a test harness:
  *input URL → normalized entries snapshot*.

---

## 2. Top‑Level Structure

Each rule file is a YAML document with this top‑level layout:

```yaml
id: captura.route.github.trending
version: 1
description: GitHub Trending repositories
author: captura
tags: [github, trending]
default_view: articles
examples:
  - https://github.com/trending

match:
  url:
    host: "github.com"
    path_regex: "^/trending"

params:
  defaults:
    since: "daily"
  docs:
    since: "Trending range: daily/weekly/monthly"

fetch:
  user_agent: captura/0.1
  timeout_ms: 15000
  smart: false
  respect_robots: true
  proxies:
    - "http://proxy1.example.com:8080"
    - "socks5://proxy2.example.com:1080"

source:
  type: list_detail   # list_detail | single_page | json | xpath
  # ... (see sections below)

filters:
  # ... (optional)

transform:
  # ... (optional)
```

### 2.1 Meta

- `id` (string, required): globally unique rule identifier.
- `version` (integer, required): MUST be `1` for this DSL.
- `description` (string, optional): human‑readable description.
- `author` (string, optional): rule author/maintainer.
- `tags` (string[], optional): tags for discovery and grouping.
- `examples` (string[], optional): example URLs for testing and docs.
- `default_view` (string, optional): recommended `EntryView` for feeds created
  from this rule (e.g. `articles`, `pictures`, `videos`, `audios`, `social`,
  `notifications`). When a client creates a feed from a rule template and does
  not explicitly specify a view, this value can be used as the initial
  `feed.view`. The special logical view `all` is not stored in `feed.view` and
  should not be used here; use `articles` instead when you want the traditional
  article timeline.

### 2.2 Match

Rules can declare which URLs they are intended for. This is mainly used for:

- Auto‑selecting a rule when creating a feed from a URL.
- Documentation and linter suggestions.

```yaml
match:
  url:
    host: "example.com"
    path_regex: "^/news"
```

Fields:

- `host` (string, optional): exact host (no wildcard) to match.
- `path_regex` (string, optional): regular expression applied to the path
  (e.g. `/news/...`).

If `match` is missing, the rule will never be auto‑selected; it can still be
used explicitly via `feed.rule_id`.

### 2.3 Params

Rules can be parameterized and receive values from `feed.rule_params_json`.

```yaml
params:
  defaults:
    cat: "news"
    page: "1"
  docs:
    cat: "Category slug on the site"
    page: "Page number (1-based)"
```

- `defaults` (map<string,string>, optional): default parameter values used when
  missing in `rule_params_json`.
- `docs` (map<string,string>, optional): documentation for each parameter, used
  by UI and tooling only.

Parameters are interpolated into strings using `{name}` syntax, e.g.:

```yaml
source:
  type: list_detail
  list:
    request:
      url: "https://example.com/{cat}?page={page}"
```

Interpolation is string‑based; non‑string values in `rule_params_json` will be
stringified when substituted.

### 2.4 Fetch Defaults

`fetch` defines default HTTP/crawler behaviour for the rule. Sub‑blocks (list,
content, JSON) may override these.

```yaml
fetch:
  user_agent: captura/0.1
  timeout_ms: 15000
  smart: false
  respect_robots: true
```

Fields:

- `user_agent` (string, optional): default User‑Agent.
- `timeout_ms` (integer, optional): request timeout in milliseconds.
- `smart` (bool, optional, default `false`):
  - `true`: allow using the spider “smart” path (headless/dynamic pages) where
    supported.
  - `false`: plain HTTP only.
- `respect_robots` (bool, optional, default `true`): whether spider should
  respect `robots.txt` when used.
- `proxies` (string[], optional): 一组代理 URL，用于该规则发起的 HTTP 请求；
  如果设置，将覆盖 feed‑级别的代理配置。规则级代理主要服务于需要
  特定出口或地区的站点（例如部分 RSSHub 路由中的地区线路）。

Additional HTTP fields (per request) are listed in §3.1.

---

## 3. Source Types

The `source` block describes how to transform upstream data into normalized
entries. It always has a `type` field:

```yaml
source:
  type: list_detail | single_page | json | xpath
```

### 3.1 Common request options

Several source types use a `request` object with these fields:

```yaml
request:
  url: "https://example.com/path"
  method: GET        # GET | POST (others may be added later)
  headers:
    X-Device: "pc"
  body: null         # string or application/x-www-form-urlencoded map (future)
  timeout_ms: 15000  # override fetch.timeout_ms
  smart: false       # override fetch.smart
  respect_robots: true
```

Semantics:

- `url` (string, required): final URL after parameter interpolation.
- `method` (string, optional, default `GET`).
- `headers` (map<string,string>, optional): additional HTTP headers.
- `body` (string, optional): request body for `POST` (v1 may keep this limited
  to simple cases).
- `timeout_ms`, `smart`, `respect_robots`: override `fetch` defaults for this
  specific call.

### 3.2 CSS selector shorthand

For HTML/XML selectors, the following shorthand is used:

- `"css-selector"` → text content of the first matching element.
- `"css-selector@attr"` → value of the attribute `attr` on the first matching
  element.

For example:

- `"a.title"` → text inside `<a class="title">...</a>`.
- `"a@href"` → `href` attribute of `<a>` element.

This is consistent with existing code in Captura.

---

### 3.3 `type: list_detail`

Common pattern: list page(s) contain links; each link points to a detail page
from which full content may be extracted.

```yaml
source:
  type: list_detail

  list:
    request:
      url: "https://example.com/news?page={page}"
    item: "article.post"
    link: "a@href"
    title: "a.title"
    summary: ".summary"
    published_at:
      selector: "time@datetime"
      format: "%Y-%m-%dT%H:%M:%S%z"

  content:
    mode: css          # css | readability | json_fragment (reserved)
    selector: "div.article-content, section.content"  # required when mode=css
    remove:
      - "script"
      - ".ad"
      - ".comments"
    fallback: summary  # none | summary | whole_page
    use_entry_url: true
```

#### 3.3.1 `list` block

- `request` (required): see §3.1.
- `item` (string, required): CSS selector for each list item element.
- `link` (string, optional): CSS/attribute shorthand used relative to `item`:
  - If missing, the engine may try `@href` on the `item` itself.
- `title` (string, optional): CSS shorthand for title, relative to `item`.
- `summary` (string, optional): CSS shorthand for summary, relative to `item`.
- `published_at` (optional):

  ```yaml
  published_at:
    selector: "time@datetime"
    format: "%Y-%m-%dT%H:%M:%S%z"
  ```

  - `selector`: CSS shorthand; if absent, published date remains `null`.
  - `format`: chrono‑compatible format string.

#### 3.3.2 `content` block

- `mode` (string, optional, default `"css"`):
  - `"css"`: use `selector` to grab content.
  - `"readability"`: use the internal readability engine to select main
    article content (current implementation is a heuristic; pluggable later).
  - `"json_fragment"`: reserved for future use.
- `selector` (string, required when `mode = "css"`): CSS selector(s) applied to
  the detail page to extract HTML fragments.
- `remove` (string[], optional): CSS selectors to remove from the extracted
  content (ads, comments, etc.).
- `fallback` (string, optional, default `"summary"`):
  - `"none"`: no fallback; content may be empty.
  - `"summary"`: fall back to the list summary text if available.
  - `"whole_page"`: fall back to the entire HTML body.
- `use_entry_url` (bool, optional, default `true`):
  - If `true`, the detail page URL is taken from the `link` field.
  - If `false`, the list page HTML may be reused as the content source.

#### 3.3.3 `detail_extra` block (optional)

For list-detail rules that need a second HTTP request per item (for example,
to fetch JSON/GraphQL details), v1 supports an optional `detail_extra` block:

```yaml
source:
  type: list_detail
  # ... list/content as above ...

  detail_extra:
    request:
      url: "https://api.example.com/item/{id}"
      method: GET
      # headers/timeout_ms/body follow the same schema as §3.1 Request
    params_from:
      id: "a@data-id"
    root: "data"
```

Fields:

- `request` (object, required):
  - Reuses the same schema as `RequestSpec` (§3.1):
    - `url` is treated as a template and rendered with parameters,
      using `{name}` placeholders.
    - `method`, `headers`, `timeout_ms`, `body` behave as in other requests.
- `params_from` (map<string,string>, optional):
  - Keys are parameter names; values are CSS/attribute expressions evaluated
    relative to each list item node:
    - e.g. `id: "a@data-id"` extracts the `data-id` attribute from the first
      `<a>` inside the item.
  - The resulting key/value pairs are merged into the parameter map used to
    render `detail_extra.request.url`, in addition to `params.defaults` and
    `feed.rule_params_json`.
- `root` (string, optional):
  - Dot-notation JSON path inside the extra response (e.g. `"data.item"`).
  - If set, the value at that path is used; otherwise the full JSON body is
    used.

Semantics:

- For each entry produced by the list:
  1. Build parameters from rule defaults + feed params + `params_from` on the
     corresponding list item element.
  2. Render `detail_extra.request.url` with `{name}` placeholders.
  3. Perform the HTTP request (currently treated as JSON).
  4. Parse the response as JSON; if `root` is set, navigate to that path.
  5. Assign the resulting JSON value to `entry.extras`.

This allows expressing “HTML list + per-item JSON/GraphQL detail” patterns in
DSL v1 without writing custom Rust handlers.

---

### 3.4 `type: single_page`

Used when a single page represents one logical entry (e.g. a static article).

```yaml
source:
  type: single_page

  request:
    url: "https://example.com/blog/{slug}"
    smart: true

  content:
    mode: readability
    remove:
      - "nav"
      - "footer"
```

Fields:

- `request` (required): see §3.1.
- `content` (required): same fields as in list_detail, but applied to a single
  page.

The engine produces a single `NormalizedEntry` per rule execution.

---

### 3.5 `type: json`

Used for JSON APIs that should be turned into feeds (inspired by FreshRSS
dot‑notation and RSSHub APIs), and for cases where JSON is embedded inside an
HTML document.

```yaml
source:
  type: json

  request:
    url: "https://api.example.com/articles?cat={cat}"

  root: "items"

  mapping:
    title: "title"
    url: "url"
    summary: "summary"
    content_html: "content_html"
    author: "author.name"
    published_at:
      path: "published_at"
      format: "%Y-%m-%dT%H:%M:%S%z"
    enclosure:
      url: "enclosure.url"
      type: "enclosure.mime"
      length: "enclosure.size"
```

Fields:

- `request` (required unless `from_html` is used): see §3.1.
- `root` (string, required): dot‑notation path to the list array inside the
  JSON document, e.g. `"items"` or `"data.items"`.
- `mapping` (required): describes how to populate `NormalizedEntry` fields from
  each JSON item.

Supported mapping keys:

- `title`, `url`, `summary`, `content_html`, `author`:
  - Value is a dot‑notation path (e.g. `"title"`, `"author.name"`).
- `published_at`:

  ```yaml
  published_at:
    path: "published_at"
    format: "%Y-%m-%dT%H:%M:%S%z"
  ```

  - `path`: dot‑notation path inside the item.
  - `format`: chrono‑compatible datetime format.

- `enclosure`:

  ```yaml
  enclosure:
    url: "enclosure.url"
    type: "enclosure.mime"
    length: "enclosure.size"
  ```

  - All fields are optional; if `url` is missing or empty, no enclosure is
    created.

Multi-source example:

```yaml
source:
  type: json

  sources:
    - request:
        url: "https://api.example.com/listA"
      root: "data.items"
      mapping:
        title: title
        url: link
    - request:
        url: "https://api.example.com/listB"
      root: "payload.results"
      mapping:
        title: name
        url: url
```

When `sources` is present, the top-level `request/root/mapping/from_html`
fields are ignored and instead each entry in `sources` defines:

- `request`: per-source JSON request (same schema as above, without `from_html`).
- `root`: optional JSON path for that source.
- `mapping`: field mapping for that source.

Semantics:

- For each `sources[i]`:
  1. Render `request.url` with params.
  2. Fetch JSON from the rendered URL (honouring `method/headers/timeout_ms`).
  3. Navigate to `root` (if set) or use the whole JSON value.
  4. Expect an array at that location; apply `mapping` for each item to build entries.
- All entries from all sources are concatenated into a single list.

#### 3.5.1 Extracting JSON from HTML (optional)

Some sites expose structured data as JSON embedded in HTML (for example inside
`<script type="application/ld+json">`). To support this pattern, `json` sources
MAY provide a `from_html` block:

```yaml
source:
  type: json

  from_html:
    request:
      url: "https://example.com/page/{slug}"
    selector: "script[type='application/ld+json']"
    multiple: true      # optional, default false
```

Fields:

- `from_html.request` (optional):
  - If present, overrides the top‑level `request` for this source.
  - If missing, the top‑level `request` is used, but the response is treated as
    HTML instead of JSON.
- `selector` (string, required): CSS selector used to locate node(s) whose
  text content is JSON.
- `multiple` (bool, optional, default `false`):
  - `false`: only the first matching node is considered; its text content is
    parsed as a single JSON document.
  - `true`: all matching nodes are considered; each node’s text content is
    parsed as JSON, then combined into an array. The effective `root` path is
    applied to this combined array.

Semantics:

- When `from_html` is present, the engine:
  1. Fetches HTML using `from_html.request` or `request`.
  2. Selects node(s) with `selector`.
  3. Reads each selected node’s text as JSON (ignoring nodes that fail to
     parse).
  4. If `multiple=false`, the first successful JSON document becomes the root
     document; if `multiple=true`, all successful documents are aggregated into
     a JSON array.
  5. The `root` and `mapping` rules are then applied as described above.

Initial v1 implementations may choose to support only the `multiple=false`
case; the schema is defined to allow more complete implementations later.

**Concrete example**

HTML page:

```html
<html>
  <body>
    <script id="data" type="application/json">
      {"items":[{"title":"FromHtml Title","url":"https://example.com/from_html"}]}
    </script>
  </body>
</html>
```

Rule (single embedded JSON document, `multiple=false` by default):

```yaml
id: "test.rule.json_from_html"
version: 1
description: "json from html pipeline"

source:
  type: json

  from_html:
    request:
      url: "https://example.com/html_json"
    selector: "script#data"
    # multiple: false (default)

  root: "items"
  mapping:
    title: "title"
    url: "url"
```

Semantics:

- The engine:
  1. Fetches `https://example.com/html_json`;
  2. Locates `<script id="data">…</script>` via `selector`;
  3. Parses its text as JSON, yielding `{"items":[...]}`;
  4. Applies `root = "items"` to obtain the array of items;
  5. For each item, maps `title` and `url` into normalized entries.

---

### 3.6 `type: xpath`

For complex HTML/XML sources where CSS is insufficient, rules can use XPath
expressions (inspired by FreshRSS HTML/XML XPath modes).

```yaml
source:
  type: xpath

  request:
    url: "https://example.com/news"
    accept: "html"   # html | xml (reserved for future)

  xpath:
    item: "//ul/li"
    title: ".//h2/text()"
    url: ".//a/@href"
    content_html: ".//div[@class='entry-content']"
    published_at:
      expr: ".//time/@datetime"
      format: "%Y-%m-%dT%H:%M:%S%z"
```

Fields:

- `request` (required): see §3.1.
- `xpath` (required):
  - `item` (string, required): XPath expression that yields a node set of items.
  - `title`, `url`, `content_html` (string, optional): XPath expressions
    evaluated relative to each item node.
  - `published_at`:

    ```yaml
    published_at:
      expr: ".//time/@datetime"
      format: "%Y-%m-%dT%H:%M:%S%z"
    ```

    - `expr`: XPath expression evaluated relative to each item node.
    - `format`: chrono datetime format.

Implementation of XPath is optional in early v1 code, but the schema is
established for forwards compatibility.

---

## 4. Filters

Filters control which entries are kept, inspired by Miniflux block/keep filters
and FreshRSS conditional full‑content retrieval.

```yaml
filters:
  entry_include:
    - ".*"            # regex on title+summary+content
  entry_exclude:
    - ".*广告.*"
    - ".*sponsored.*"

  fetch_full_content_when:
    - field: title
      regex: ".*阅读全文.*"
    - field: summary
      regex: ".*本文.*"
```

Fields:

- `entry_include` (string[], optional):
  - If non‑empty, an entry is **kept only if** at least one regex matches the
    concatenated string `title + "\n" + summary + "\n" + content_html`.
- `entry_exclude` (string[], optional):
  - If any regex matches the same concatenated string, the entry is dropped.

If both lists are empty or missing, no filter is applied at the DSL level
(database‑level feed filters may still apply).

- `fetch_full_content_when` (object[], optional):

  ```yaml
  fetch_full_content_when:
    - field: title        # title | summary | content_html
      regex: ".*阅读全文.*"
  ```

  - Each item describes a condition on a single field.
  - `field`: which field to test:
    - `"title"`: the entry title string (or empty when missing).
    - `"summary"`: the entry summary string (or empty when missing).
    - `"content_html"`: the HTML content as text.
  - `regex`: regular expression evaluated against the chosen field.

  Semantics:

  - If the list is non‑empty and *any* condition matches for a given entry,
    engines **may** trigger a “full content fetch” for that entry (for example
    by invoking the pipeline helper `fetch_and_extract_entry` on the entry URL
    and merging the result, see §5 of `docs/rules-engine.md`).
  - v1 implementations are allowed to ignore this block initially (best‑effort
    hint), but should treat the schema as stable.

---

## 5. Transform

Transform rules modify URLs and content once entries have been built, combining
ideas from Miniflux rewrite rules and FreshRSS DOM filters.

```yaml
transform:
  url_rewrite:
    - "s/\\?utm_[^&]+//g"
    - "ref=\\w+=>"

  content_rewrite:
    - "# remove sponsor wording"
    - "s/赞助内容//g"

  content_remove_selectors:
    - ".ad"
    - ".sponsor"
    - "script"

  content_merge:
    mode: replace   # replace | prepend | append

  description_template: |
    <p><strong>{title}</strong></p>
    <p>{summary}</p>
    <p><a href="{url}">查看原文</a></p>
```

### 5.1 Rewrite syntax

Each rewrite rule is a single line string, using the same conventions as the
existing pipeline:

- Sed‑like syntax:

  - `s/pattern/repl/flags` (flags are optional; initial v1 may ignore them).
  - Example: `s/\\?utm_[^&]+//g`

- Fallback syntax:

  - `pattern => replacement`
  - Example: `"ref=\\w+=>"`

Both `url_rewrite` and `content_rewrite` apply these rules in order, where the
engine:

1. Tries to parse as sed‑like rule; if parsing fails,
2. Falls back to `pattern => replacement`.

### 5.2 DOM removal

- `content_remove_selectors` (string[], optional):
  - CSS selectors applied to the extracted HTML content to remove unwanted
    nodes (ads, tracking widgets, comments, etc.).
  - Semantics: parse the current HTML fragment as DOM, remove nodes matching
    any selector, then re‑serialize.

- `content_merge` (object, optional):

  ```yaml
  content_merge:
    mode: replace   # replace | prepend | append
  ```

  - `mode` controls how new “full content” (for example obtained via
    readability or an external extractor) should be combined with any existing
    content/summary when the rule is used as a full‑content filter:
    - `replace` (default): replace the existing content with the new full
      content.
    - `prepend`: prepend the new full content before the existing content.
    - `append`: append the new full content after the existing content.

  This setting is primarily intended for engines that apply rules to existing
  entries (e.g. “fetch full content” actions on already‑stored items). Rule‑type
  feeds that generate entries from scratch may ignore `content_merge` or treat
  `replace` as the default.

- `description_template` (string, optional):
  - A simple string template used to build `content_html` for each entry.
  - Supported placeholders:
    - `{title}` – entry title (empty if missing).
    - `{summary}` – entry summary (empty if missing).
    - `{url}` – entry URL (empty if missing).
    - `{author}` – entry author (empty if missing).
    - `{content_html}` – current HTML content (empty if missing).
  - The engine applies this template after filters and conditional
    full-content fetch, and overwrites `entry.content_html` with the rendered
    and sanitized HTML.

---

## 6. Versioning and Compatibility

- All new rules MUST set `version: 1`.
- Future DSL versions will increment the `version` field and keep v1 parsing
  available for backwards compatibility.
- The legacy DSL used before v1 should be treated as pre‑v1 and migrated to v1
  over time.

---

## 7. Implementation Notes (non‑normative)

This section is informational for Captura’s Rust implementation and is not part
of the stable DSL contract.

- Parsing:
  - The v1 schema is implemented in `crates/extract::v1` as `RuleSpecV1`,
    `SourceType` and related types, and re-exported from `crates/hub` as
    `captura_rules::v1::*` for convenience.
  - `parse_rule_v1` / `validate_v1` use `serde_yaml` for YAML deserialization
    and enforce:
    - `id` non‑empty,
    - `version == 1`,
    - required fields for each `source.type`,
    - regex fields compile.
- Execution:
  - Database-aware execution for rule feeds lives in
    `crates/pipeline/src/rules_engine.rs`:
    - `refresh_rule_v1(feed, &RuleSpecV1) -> Result<Vec<NormalizedEntry>>`.
  - Stateless JSON execution (used by Hub handlers and tools) lives in
    `crates/extract/src/v1_exec.rs`:
    - `execute_json_v1_stateless(spec, ctx)`.
  - Both reuse existing components where relevant:
    - `fetch_html_strategy` for HTTP/spider logic (pipeline),
    - `captura_extract::extract_from_html` / readability interface for
      full-content extraction,
    - existing URL/content rewrite and entry filter helpers.
- Testing:
  - Keep sample rules under `rules/` with corresponding integration tests.
  - For each rule, test at least one `examples` URL (when allowed) or mock
    responses in a local test server.
