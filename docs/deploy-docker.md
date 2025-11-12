# Docker 部署指南

本文档介绍如何使用 Docker 与 Docker Compose 部署 Captura，自带最小依赖（PostgreSQL 18）与可选反向代理（Traefik/Caddy）。

## 快速开始（仅 API + PostgreSQL）

1) 准备环境

- 已安装 Docker 与 Docker Compose v2
- 可选：复制 `.env.example` 为 `.env` 并按需修改

2) 启动

```
docker compose build
docker compose up -d
```

3) 验证

- 访问健康检查：`http://localhost:8080/healthz`
- API 默认监听 `0.0.0.0:8080`

> 说明：`docker-compose.yml` 使用 `postgres:18-alpine`，DB 就绪后再启动 API。API 进程内会自动执行数据库迁移。

## 环境变量

- `DATABASE_URL`：数据库连接串，默认 `postgres://captura:captura@db/captura`
- `RUST_LOG`：日志级别，默认 `info`
- `SCHEDULER_ENABLED`：是否启用内置调度器（定时拉取/任务队列），默认启用（设置为 `false` 或 `0` 关闭）

> 我们遵循“最小必要”环境变量（对齐 Miniflux 风格）。后续新增配置会在文档中补充。

## 数据持久化与备份

- PostgreSQL 持久化卷：`pgdata`
- 备份示例：

```
docker exec -t captura-db pg_dump -U captura captura > captura_$(date +%F).sql
```

- 恢复示例：

```
cat captura_2025-01-01.sql | docker exec -i captura-db psql -U captura captura
```

## 可选：反向代理（Traefik/Caddy）

### Traefik（推荐）

1) 启动带 Traefik 的组合：

```
CAPTURA_HOST=captura.localhost docker compose -f docker-compose.yml -f docker-compose.traefik.yml up -d
```

2) 访问：

- `http://captura.localhost`（Traefik 转发到 API 8080）
- Traefik 仪表盘：`http://localhost:8081`

> 提示：在生产中请配置 HTTPS 与 ACME；可根据需要添加 `--entrypoints.websecure.address=:443` 和相应证书设置。

### Caddy（最简）

```
docker compose -f docker-compose.yml -f docker-compose.caddy.yml up -d
```

访问 `http://localhost`，Caddy 会反向代理到 `api:8080`。

## PostgreSQL 调优（可选）

中小规模部署可使用默认设置。若需要，可参考 FreshRSS 的示例进行简单参数调优：

```
services:
  db:
    image: postgres:18-alpine
    command:
      - -c
      - shared_buffers=1GB
      - -c
      - work_mem=32MB
```

请根据主机内存与负载实际评估调整。

## 镜像构建说明（cargo-chef）

- 参考 `repo-ref/cargo-chef/README` 的最佳实践：
  - 三阶段：planner(prepare) → builder(cook + build) → runtime
  - 所有阶段保持相同 Rust 版本
  - cook 与 build 使用同一工作目录（/app），确保依赖缓存命中
- 我们的 Dockerfile 已按此实现。

## 常见问题

- 健康检查失败
  - API 容器内置 curl，Compose 健康检查使用 `curl -fsS http://localhost:8080/healthz`；请确认端口未被占用，或检查日志 `docker compose logs api`。
- 无法连接数据库
  - 检查 `DATABASE_URL` 与 `db` 容器健康状态；等待健康检查通过后 API 才会启动。
- 调度器不需要
  - 设置 `SCHEDULER_ENABLED=false` 关闭。

