# Captura Hub Routes

Captura 支持使用本地的“Hub 路由”来快速创建基于模板的抓取订阅，语义上参考 RSSHub 的路由，但完全由 Captura 内置模板与规则引擎实现，不依赖外部 RSSHub 服务。

## 用法

- 仅使用 `captura_hub://` 方案创建订阅，例如：

```
captura_hub://github/trending?since=daily
captura_hub://github/trending?since=daily&language=rust
captura_hub://github/trending?since=weekly&language=javascript&spoken_language=en
captura_hub://hn/front
captura_hub://lobsters/front
captura_hub://medium/tag?tag=rust
```

- Captura 将 `captura_hub://{route}` 映射为本地的规则模板（rule feed），并保存查询参数为模板参数。当前内置模板包括：
  - `github/trending` → `captura.route.github.trending`
  - `hn/front` → `captura.route.hn.front`
  - `lobsters/front` → `captura.route.lobsters.front`
  - `zhihu/hotlist` → `captura.route.zhihu.hotlist`
  - `reuters/top` → `captura.route.reuters.top`
  - `medium/tag` → `captura.route.medium.tag`

## 验证接口

- 提供基于模板映射的本地验证，不再对外发起 HTTP 请求：

```http
POST /api/v1/feeds/validate-hub
{"route": "github/trending?since=daily"}
```

- 响应示例（存在对应模板时）：

```json
{"ok": true, "status": null, "url": "captura_hub://github/trending?since=daily", "feed_type": "rule"}
```

- 若模板不存在：

```json
{"ok": false, "status": null, "url": "captura_hub://unknown/route", "feed_type": "unknown", "message": "rule template not found; run migrations or import templates"}
```

## 说明

- 仅支持 `captura_hub://`，不再兼容 `hub://` 或 `rsshub://`；也不再依赖 `CAPTURA_HUB_BASE` 或外部默认基址。
- 你也可以直接使用绝对 URL 作为常规订阅（RSS/Atom/JSON Feed），与 Hub 路由互不冲突。
- 可通过新增迁移或 API 导入更多模板；模板语法与规则引擎详见规则相关文档与示例。
