# Remote Compatibility

| 能力 | Ubuntu / Debian | Rocky / Alma / RHEL | Alpine | Arch | Fallback |
|---|---|---|---|---|---|
| 系统识别 | `/etc/os-release` | `/etc/os-release` | `/etc/os-release` | `/etc/os-release` | 基础 Linux，标记 unsupported |
| 包管理 | `apt-get` / `dpkg-query` | `dnf`，其次 `yum` / `rpm` | `apk` | `pacman` | 禁用安装 |
| 服务 | `systemctl` / `journalctl` | `systemctl` / `journalctl` | `rc-service` / `rc-update` | `systemctl` / `journalctl` | 仅探测，不伪装支持 |
| 端口 | `ss` | `ss` | `lsof`（如存在） |
| 网络 | `/proc/net/dev` + `ip` | `/proc/net/dev` + `ip` | 仅 `/proc` 指标 |
| 文件 | SFTP v3 | SFTP v3 | 无 SFTP 时明确报错 |
| Docker | 远程 `docker` CLI | 远程 `docker` CLI | 未安装/权限不足分开呈现 |
| Nginx | `nginx -T` + systemd | `nginx -T` + systemd | 编译参数推导路径 |

工具中心目前使用固定 registry：Ubuntu/Debian 通过 `apt-get`，Rocky/Alma/RHEL 优先 `dnf`，Alpine 使用 `apk`，Arch 使用 `pacman`（仅当探测到包管理器时生成计划），不会执行 `curl | sh`。Nginx GUI 新配置只写 `/etc/nginx/conf.d/relay-*.conf`，且必须从 `nginx -T` 实际输出检测到对应 include；否则只提供只读解析和通用文件编辑入口。

所有能力先进入 `ServerCapabilities`；前端据此禁用不可用操作并显示原因。发行版差异集中在 `PlatformAdapter`，不散落在 UI 或业务 service。
