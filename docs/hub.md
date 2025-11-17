# Captura Hub Routes

Captura 支持使用本地的“Hub 路由”来快速创建基于模板的抓取订阅，语义上参考 RSSHub 的路由，但全部逻辑由 Captura 内置的 Rust handler 与 DSL 引擎实现，不依赖外部 RSSHub 服务。

Hub 路由在内部是一个 `Route { meta: RouteMeta, handler: HubHandlerFn }`，其中：

- `RouteMeta` 描述 `hub_id/path/categories/example/params/features/radar/name/maintainers/url/description`；
- `handler` 是一个异步函数，签名等价于：`async fn handler(ctx: &mut HubCtx<'_>) -> Result<HubData>`。

在代码结构上，我们刻意把 **RouteMeta + 抓取规则 + handler 实现放在同一个模块/文件里**（例如
`crates/hub/src/routes/github/trending.rs`），这是有意向 RSSHub 靠拢的设计选择：

- 对贡献者来说，打开一个路由文件就能同时看到“路由元信息（路径、参数、示例）”和“具体如何抓取网页、如何提取正文”的完整上下文，修改体验与 RSSHub 的 `lib/routes/*/*.ts` 非常类似；
- 规则（如何选元素、如何构造 URL）和 handler（如何组织成 HubData）本质上都是“如何抓取网页并输出条目”的一部分，放在一起更符合维护者的思维模型；
- 这种布局也方便未来从 RSSHub 迁移/对照路由：一条路由对应一个 Rust 模块，meta 与实现紧邻，降低认知负担。

因此，虽然内部还存在 DSL 抽象和规则执行引擎，但在 Hub 层我们刻意保持“贴近 RSSHub 的文件级组织”，优先服务贡献体验和路由可读性。

## 1. 订阅用法（captura_hub://）

订阅 Hub 路由时，使用 `captura_hub://` 方案，例如：

```text
captura_hub://github/trending?since=daily
captura_hub://github/trending?since=daily&language=rust
captura_hub://github/trending?since=weekly&language=javascript&spoken_language=en
captura_hub://hn/front
captura_hub://lobsters/front
captura_hub://medium/tag?tag=rust
captura_hub://zhihu/hotlist
```

- 当 `feed_url` 为 `captura_hub://{route}?{params}` 时：
  - `route` 会被解析为 `hub_id`（去掉前导 `/`），例如 `github/trending`；
  - `{params}` 解析为一个 JSON 对象，保存到 `feed.rule_params_json`，用于传参；
  - `feed.type` 将被设置为 `hub`；
  - 刷新时由 pipeline 调用 `execute_hub_route(hub_id, params)`，执行对应的 Hub handler。

当前内置 Hub 路由包括（示例）：

- `github/trending`
- `hn/front`
- `lobsters/front`
- `zhihu/hotlist`
- `reuters/top`
- `medium/tag`
- 一系列 `bilibili/*`（热搜、热门、用户空间、番剧等）

具体列表可以通过 `/api/v1/hub/routes` 获取（见下文）。

## 2. 验证与预览接口

### 2.1 验证 Hub 路由：`/api/v1/feeds/validate-hub`

- 本地验证，仅检查 Hub 路由是否存在，不对外发起 HTTP 请求。

请求示例：

```http
POST /api/v1/feeds/validate-hub
{"route": "github/trending?since=daily"}
```

响应示例（路由存在时）：

```json
{"ok": true, "status": null, "url": "captura_hub://github/trending?since=daily", "feed_type": "hub", "message": null}
```

若路由不存在：

```json
{"ok": false, "status": null, "url": "captura_hub://unknown/route", "feed_type": "unknown", "message": "unknown captura_hub route"}
```

### 2.2 列出所有 Hub 路由：`/api/v1/hub/routes`

```http
GET /api/v1/hub/routes
Authorization: Bearer <token>
```

返回一个 `routes` 数组，每项包含：

- `hub_id`
- `path`
- `categories`
- `example`
- `parameters`（参数名 + 描述）
- `name`
- `url`
- `description`

WebUI 的 `/hub` 页面就是基于该接口渲染的。

### 2.3 预览 Hub 路由：`/api/v1/hub/preview`

预览接口作用类似 RSSHub 的调试：给定一个 `captura_hub://` URL，返回经 Hub handler 抓取生成的 `HubData`。

请求示例：

```http
POST /api/v1/hub/preview
Authorization: Bearer <token>
Content-Type: application/json

{"url": "captura_hub://github/trending?since=daily&language=rust"}
```

响应体是一个 `HubData` 对象（title/link/items 等），可用于 UI 预览或调试。

## 3. 规则 try 接口 vs Hub 预览

Captura 还提供了一个 **规则试运行接口**（针对 DSL 规则）：

- `POST /api/v1/rules/try`：输入 RuleSpecV1（或 rule_id）+ 一个 list URL，返回基于 DSL 执行结果的概要（条目数、前若干条 title/url 等）。

这与 Hub 预览的关系是：

- `/rules/try`：只针对 **DSL 规则**，用于调试 YAML/JSON 规则的 list_detail 抽取；
- `/hub/preview`：针对 **Hub 路由**，调用 Rust handler（内部可以复用 DSL 或任意逻辑），输出完整 `HubData`。

在设计上，Hub 路由是“产品级、官方/社区维护”的入口，DSL 规则更多用于探索和本地自定义，两者都可以在 WebUI 中被调用。

## 4. 贡献新的 Hub 路由

Hub 路由的代码位于 `crates/hub/src/hub`，每个站点一个模块，例如：

- `github/trending`：`crates/hub/src/hub/github/trending.rs`
- `hn/front`：`crates/hub/src/hub/hn/front.rs`
- `bilibili/*`：`crates/hub/src/hub/bilibili/*.rs`

新增一条 Hub 路由的步骤大致如下：

1. **定义 RouteMeta**

   在对应站点模块下新建文件，例如 `crates/hub/src/hub/foo/bar.rs`：

   ```rust
   pub const META_FOO_BAR: RouteMeta = RouteMeta {
       hub_id: "foo/bar",
       path: "/foo/bar/:param?",
       categories: &["example"],
       example: "/foo/bar/demo",
       params: &[
           ParamMeta {
               name: "param",
               description: "demo param",
               default: None,
               options: &[],
           },
       ],
       features: Features { /* ... */ },
       radar: &[Radar { source: &["example.com"], target: "/foo/bar" }],
       name: "Foo Bar",
       maintainers: &["your-id"],
       url: "https://example.com/foo",
       description: "Foo Bar route.",
   };
   ```

2. **实现 handler**

   ```rust
   pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
       let param = ctx.param_str("param").unwrap_or("default");
       let url = format!("https://example.com/foo?param={}", param);
       let html = util::get_html(&url).await?;
       let mut items = Vec::new();
       util::for_each_element(&html, "article", |el| {
           let link = util::extract_attr(&el, "a@href")
               .map(|href| util::absolutize(&url, &href));
           let title = util::extract_text(&el, "a");
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
           title: "Foo Bar".into(),
           description: None,
           link: Some(url),
           image: None,
           language: None,
           items,
           allow_empty: false,
       })
   }
   ```

3. **导出并注册 Route 常量**

   ```rust
   fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> HubHandlerFuture<'a> {
       Box::pin(handler(ctx))
   }

   #[register_hub_route]
   pub const ROUTE_FOO_BAR: Route = Route {
       meta: &META_FOO_BAR,
       handler: handler_fn,
   };
   ```

   通过在 `Route` 常量上添加 `#[register_hub_route]`，路由会自动注册到全局 Hub registry，
   无需再手动维护 `ROUTES` 数组或修改 `registry.rs`。
