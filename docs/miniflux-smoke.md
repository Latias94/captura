# Miniflux 兼容层快速验证（Smoke）

本文件提供一组最小 HTTP 示例（基于 curl），用于验证 Captura 的 Miniflux 兼容端点是否工作正常。

前置条件

- 服务已启动：默认监听 `http://localhost:8080`
- 已安装 `curl`；建议安装 `jq` 便于解析 JSON（可选）

步骤概览

1) 创建用户与登录（/api/v1）
2) 通过 X-Auth-Token 调用 Miniflux 兼容端点（/v1）
3) 覆盖：分类/订阅源/刷新/条目/标签/发现/版本/统计

快速脚本

建议直接运行脚本：`scripts/miniflux_smoke.sh`（可重复运行）。

手动命令（示例）

1) 登录获取 token（若首次运行需先创建用户）

```bash
BASE=http://localhost:8080
USER=alice
PASS=secret

# 创建用户（仅当系统无用户时）
curl -sS -X POST "$BASE/api/v1/users" \
  -H 'content-type: application/json' \
  -d '{"username":"'$USER'","password":"'$PASS'"}'

# 登录
TOKEN=$(curl -sS -X POST "$BASE/api/v1/auth/login" \
  -H 'content-type: application/json' \
  -d '{"username":"'$USER'","password":"'$PASS'"}' | jq -r .token)
echo "TOKEN=$TOKEN"
```

2) 创建分类与订阅源

```bash
# 分类
curl -sS -X POST "$BASE/v1/categories" \
  -H "X-Auth-Token: $TOKEN" -H 'content-type: application/json' \
  -d '{"title":"tech"}'

# 订阅源（示例：Rust 官方博客）
FEED_ID=$(curl -sS -X POST "$BASE/v1/feeds" \
  -H "X-Auth-Token: $TOKEN" -H 'content-type: application/json' \
  -d '{"url":"https://blog.rust-lang.org/feed.xml"}' | jq -r .id)
echo "FEED_ID=$FEED_ID"
```

3) 刷新与条目查询

```bash
# 单源刷新
curl -sS -X POST "$BASE/v1/feeds/$FEED_ID/refresh" -H "X-Auth-Token: $TOKEN"

# 列表条目
curl -sS "$BASE/v1/entries?limit=5" -H "X-Auth-Token: $TOKEN" | jq .
```

4) 条目标签写入

```bash
ENTRY_ID=$(curl -sS "$BASE/v1/entries?limit=1" -H "X-Auth-Token: $TOKEN" | jq -r .entries[0].id)
curl -sS -X POST "$BASE/v1/entries/$ENTRY_ID/tags" \
  -H "X-Auth-Token: $TOKEN" -H 'content-type: application/json' \
  -d '{"tags":["test","rust"]}'
curl -sS "$BASE/v1/tags" -H "X-Auth-Token: $TOKEN" | jq .
```

5) Discover（发现订阅）

```bash
curl -sS -X POST "$BASE/v1/discover" \
  -H "X-Auth-Token: $TOKEN" -H 'content-type: application/json' \
  -d '{"url":"https://blog.rust-lang.org/"}' | jq .
```

6) 统计与版本

```bash
curl -sS "$BASE/v1/feeds/counters" -H "X-Auth-Token: $TOKEN" | jq .
curl -sS "$BASE/v1/version" -H "X-Auth-Token: $TOKEN" | jq .
```

兼容性说明

- `/v1/*` 端点的设计基线是 **Miniflux 官方 API**，路径、请求参数和错误格式都尽量保持一致；
- 为了方便客户端使用，Captura 在 `/v1` 下还会提供少量 **扩展端点**，例如：
  - `/v1/tags/{name}/mark-all-as-read`：按标签将所有相关条目标记为已读。
- 这些扩展并不属于 Miniflux 官方 API 规范的一部分，只在 Captura 中可用；只要客户端仅依赖官方文档中的端点，就不会受到这些扩展的影响。

错误返回说明

- Miniflux 兼容层（/v1/*）：错误为 `{ "error_message": "..." }`
- 原生 API（/api/v1/*）：错误为 `{ "code": "...", "message": "..." }`

统一时间线与 Miniflux 映射（实现层说明）

> 本小节面向服务端实现者，说明 Miniflux `/v1/entries` 与 Captura 内部统一时间线模型的对应关系。普通客户端只需按 Miniflux 官方文档使用 `/v1/*` 端点，无需关心下述细节。

Captura 内部使用一个统一的 `TimelineQuery` 模型（见 `crates/service/src/query.rs`），所有时间线类查询（包括 `/api/v1/entries`、SmartView、以及 Miniflux `/v1/entries`）最终都会构造一个 `TimelineQuery` 并调用 `list_entries_for_user`。

Miniflux 兼容层中，`/v1/entries` 参数与 `TimelineQuery` 的映射大致如下：

- `status=unread|read|starred` → `TimelineStatus::Unread|Read|Starred`
- `feed_id` → `TimelineQuery.feed_ids = [feed_id]`
- `category_id` → `TimelineQuery.category_ids = [category_id]`
- `search` → `TimelineQuery.search`
- `before_id` / `before_entry_id` → `TimelineQuery.before_id`
- `after_id` / `after_entry_id` → `TimelineQuery.after_id`
- `order=id|published_at` → `TimelineQuery.sort_by = "id"|"published_at"`
- `direction=asc|desc` → `TimelineQuery.sort_order`

部分 Miniflux 特有参数目前仍作为“外挂过滤”附加在统一查询之上：

- `published_before/after`：在 `build_timeline_select` 结果上额外按 `entry.published_at` 做时间窗口过滤；
- `changed_before/after`：在同一查询上按 `entry.updated_at` 做时间窗口过滤；
- `starred=true` 等价于 `status=starred`；`starred=false` 则在统一查询结果上追加 `is_starred = false` 过滤；
- `content=true|false`：仅影响返回 JSON 是否包含 `content_html` 字段，不参与查询条件。

这样设计的目的：

- 内部只有一条时间线查询管线（`TimelineQuery`），便于演进和性能优化；
- Miniflux 客户端仍然可以按照官方语义使用 `/v1/entries`，不需要理解 Captura 的视图/SmartView/Timeline 概念；
- 当 `/api/v1/entries` 的行为（搜索、排序等）改进时，Miniflux 兼容层会自动受益，只需确保兼容测试继续通过即可。
