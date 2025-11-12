# 搜索语法与行为

本文档说明 Captura 的搜索能力、语法与排序规则。

## 数据库与引擎

- PostgreSQL：使用全文检索（tsvector + GIN），解析 `websearch_to_tsquery`（接近 Google 语法）。
- 其他数据库：回退到模糊查询（`LIKE`），功能子集。

## 语法（PG）

- 普通检索：`rust async`
- 短语：`"async runtime"`
- 逻辑：`AND/OR/NOT`（如：`rust AND tokio`、`rust OR go`、`NOT windows`）
- 字段增强（扩展能力）：
  - 标题：`title:tokio`、`title:"async runtime"`
  - 作者：`author:Alice`
  - 链接：`url:example.com`
  - 标签：`#news`、`#"深度阅读"`

上述字段语法为 Captura 增强功能，不影响与 Miniflux 的兼容性。

## 排序规则

- 有搜索（PG）：默认按相关性排序（`ts_rank_cd`），并以发布时间、创建时间降序作为兜底排序。
- 可显式指定：`sort_by=relevance|published_at|created_at` 与 `order=asc|desc`。
- 非 PG：按发布时间/创建时间排序（相关性不可用）。

## 兼容性

- Miniflux 兼容接口（`/v1/entries`）：传入 `search` 时默认相关性排序。
- 自有接口（`/api/v1/entries`）：
  - 若 PG 且 `search` 存在，默认相关性排序；
  - 若传 `sort_by`，则按指定字段排序。

## 注意事项（PG）

- 为避免 `tsvector` 限制，系统对标题、摘要、正文参与索引的内容各自截断至 500k 字符。
- 当前词典使用 `simple`，可满足大多数英文/通用场景。

