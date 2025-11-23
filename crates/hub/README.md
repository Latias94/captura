# captura-hub

`captura-hub` is the Hub routes and rules bridge crate for Captura.

It provides:

- Hub route metadata types (`RouteMeta`, `ParamMeta`, `Features`, `Radar`).
- The Hub handler types (`HubCtx`, `HubData`, `HubItem`, `Route`).
- A static registry of built-in routes discovered via `inventory`.
- A re-export of the rules DSL v1 (`RuleSpecV1`, `parse_rule_v1`) from
  `crates/extract`.

Conceptually, Hub routes are the Rust equivalent of RSSHub routes:

- Each route lives in `src/routes/<namespace>/<name>.rs`, for example
  `src/routes/hn/front.rs` or `src/routes/github/trending.rs`.
- One file contains both the route metadata and the async handler that
  fetches and parses content from the target site.
- Routes are registered via the `#[register_hub_route]` proc-macro and
  are discovered at runtime through `routes::registry::builtin_routes()`.

## Example: using a Hub route from Rust

Most applications should call Hub routes via the pipeline crate, but
you can also use them directly if needed:

```rust
use captura_hub::routes::registry;
use captura_hub::routes::types::HubCtx;

async fn run_hn_front() -> captura_common::Result<()> {
    // Look up a route by hub_id (e.g. "hn/front").
    let route = registry::builtin_routes()
        .iter()
        .find(|r| r.meta.hub_id == "hn/front")
        .expect("hn/front route not found");

    let mut params = serde_json::Map::new();
    params.insert("section".into(), "news".into());

    let mut ctx = HubCtx {
        hub_id: "hn/front",
        params: &params,
    };

    let data = (route.handler)(&mut ctx).await?;
    for item in data.items.into_iter().take(5) {
        println!("{} - {:?}", item.title, item.link);
    }
    Ok(())
}
```

In practice, Captura uses `captura_pipeline::execute_hub_route()` to
run Hub routes and convert `HubData` into `NormalizedEntry` values.

## Adding a new route

The typical steps for contributing a new Hub route:

1. Pick a module and file under `src/routes`, for example
   `src/routes/example/site.rs`.
2. Define a `RouteMeta` constant describing the route:
   `hub_id`, `path`, `categories`, `params`, `example`, `features`,
   `radar`, `name`, `url`, `description`, and optional `default_view`.
3. Implement an async handler:

   ```rust
   pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
       let param = ctx.param_str("foo").unwrap_or("default");
       let url = format!("https://example.com/list?foo={}", param);
       let html = util::get_html(&url).await?;

       let mut items = Vec::new();
       util::for_each_element(&html, "article.item", |el| {
           let link = util::extract_attr(&el, "a@href")
               .map(|href| util::absolutize(&url, &href));
           let title = util::extract_text(&el, "a.title");
           items.push(HubItem {
               title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
               description: None,
               link,
               author: None,
               pub_date: None,
               categories: Vec::new(),
           });
       })?;

       Ok(HubData {
           title: "Example Site".into(),
           description: None,
           link: Some(url),
           image: None,
           language: None,
           items,
           allow_empty: false,
       })
   }
   ```

4. Wrap the handler in a `HubHandlerFn` and register it with the macro:

   ```rust
   fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> HubHandlerFuture<'a> {
       Box::pin(handler(ctx))
   }

   #[register_hub_route]
   pub const ROUTE_EXAMPLE_SITE: Route = Route {
       meta: &META_EXAMPLE_SITE,
       handler: handler_fn,
   };
   ```

5. Export the new module from `src/routes/mod.rs`.

For a deeper explanation of Hub URLs (`captura_hub://`), preview APIs
and the relationship between Hub routes and the rules DSL, refer to the
project documentation or existing route implementations in this crate.

## Rules DSL v1

The `v1` module in this crate simply re-exports the rule DSL types
and helpers from `captura-extract`:

- `RuleSpecV1`
- `parse_rule_v1`
- `merge_rule_params_v1`

These are used by both Hub handlers and the pipeline rules engine.
