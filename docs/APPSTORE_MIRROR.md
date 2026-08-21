# 应用商店静态镜像契约

`1panel-client` 的应用商店设置支持官方 GitHub 源和最多 8 个按顺序故障转移的静态镜像节点。镜像源只需要 HTTP(S) 静态文件服务，不需要 GitHub API；公网镜像必须使用 HTTPS，客户端不会把镜像 URL 中的凭据、查询参数或片段保存到本地设置。目录或详情请求命中任意节点后，客户端会把命中的基础地址保留在响应中，生成器也会从同一节点下载资源。

## 目录布局

假设基础地址为 `https://mirror.example.com/1panel`，至少提供：

```text
catalog.json
apps/<key>/data.yml
apps/<key>/versions.json
apps/<key>/<version>/docker-compose.yml
apps/<key>/<version>/.env                 # 可选
```

`catalog.json` 使用 camelCase 字段：

```json
{
  "repository": "company/1panel-appstore-mirror",
  "branch": "stable",
  "sourceRevision": "2026-08-20T00:00:00Z",
  "items": [
    {
      "key": "openresty",
      "name": "OpenResty",
      "description": "Web server",
      "category": "Web 服务器",
      "metadataUrl": "/apps/openresty/data.yml"
    }
  ]
}
```

`data.yml` 复用 1Panel 应用 metadata 的 `name`、`description`、`tags` 和 `additionalProperties.website/github` 字段。`versions.json` 是版本对象数组；Compose 和 `.env` URL 可以省略，客户端会按固定布局生成同源地址：

```json
[
  {
    "version": "1.0.0",
    "composeUrl": "https://mirror.example.com/1panel/apps/openresty/1.0.0/docker-compose.yml",
    "envUrl": "https://mirror.example.com/1panel/apps/openresty/1.0.0/.env"
  }
]
```

客户端会校验版本目录和文件 URL，拒绝路径穿越、跨源地址、凭据、查询参数和片段；Compose 下载仍限制为 4 MiB，并在远端执行 `docker compose config -q` 后才启动。

## 缓存和离线行为

- 目录和详情缓存存储在客户端 SQLite 的 `app_settings` 表，不包含服务器密码、AI key 或 Compose secrets。
- 缓存 TTL 可在界面设置为 300–86400 秒；网络失败时会回退到最近缓存，并在界面显示缓存年龄。
- 开启离线模式后只读取缓存；没有缓存会返回明确的 `APPSTORE_CACHE_MISS`，不会伪造应用清单。
- 镜像设置界面每行填写一个基础地址，第一项为主节点；切换官方/镜像源或任一镜像节点会自动清除旧源缓存；“清理缓存”只删除本地数据，不触碰远端服务器。
