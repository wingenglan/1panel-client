# Architecture

## 边界

1Panel Client 是多服务器桌面应用。React WebView 只负责展示和输入；所有持久化、凭据访问、SSH/SFTP、远程命令、解析与危险操作验证均位于 Rust 边界内。远端服务器只需现有 OpenSSH 与标准 Linux 工具。

```text
React features
  -> typed Tauri commands/events
Application state (repositories, task manager, SSH pool)
  -> domain services (server, metrics, files, process, services, nginx, docker)
Infrastructure (SQLite, OS keychain, russh, russh-sftp, platform adapters)
  -> SSH/SFTP
Remote Linux
```

## 本地数据

SQLite 使用编译期迁移创建 `servers`、`server_groups`、`tags`、`server_tags`、`known_hosts`、`recent_paths`、`app_settings`、`audit_events`、`command_shortcuts`、`metric_samples`、`task_records`、`http_monitors` 与 `http_monitor_checks`。数据库不保存 SSH/sudo secret、AI/API key、终端输入、远程命令输出、响应正文或远程日志内容；服务器凭据列只含随机 reference id，探活表只保存 URL 元数据、状态码、延迟和脱敏错误摘要。

## SSH 生命周期

每个服务器由连接管理器维护至多一个可复用认证会话，普通短命令通过 semaphore 限制为 8 个并发 channel。连接流程先完成 Host Key 校验，再读取密码/私钥凭据或委托 SSH Agent 签名认证。未知 Host Key 仅进入短期 pending challenge；用户确认后写入 `known_hosts`。密钥变化不会降级为重新信任。

## 远程命令

React 不传裸 shell。各 domain service 构建固定 program 与经过 POSIX shell escaping 的参数；默认注入 `LC_ALL=C`，并返回结构化 `RemoteCommandResult`。sudo 通过独立 privileged runner 的 stdin 通道实现，secret 永不进入 argv 或日志。

## 前端

前端按 feature 切分，TanStack Query 管理远端状态，Zustand 管理短期 UI session，SQLite 通过 typed IPC 提供快捷指令、指标历史和任务记录，Radix 提供可访问交互原语。列表达到大数据量时使用 TanStack Virtual；终端和日志使用 Tauri Channel/Event 流式传输。

Tools、Nginx/OpenResty、Docker、网站、安全、数据库、计划任务、应用商店、AI 和高级探活当前也遵循同一 Rust boundary：各 domain 生成固定命令并解析结构化 marker/JSON，前端不接触裸 shell、数据库密码或 Docker socket；Docker/Compose 通过 SSH 执行远程 CLI 的 JSON 输出。

## 跨模块边界

- 快捷指令只负责本地匹配、参数收集和安全插入终端，不负责自动执行；Enter 和危险操作确认仍由终端/现有 privileged runner 处理。分组保存在 `command_shortcuts.group_name`；建议可按本次或当前 shell 会话隐藏，每个 shell 的开关只影响本地 UI。
- 终端命令历史只保存在当前 `SessionTerminal` 内存状态，最多 200 条，用于定位 xterm scrollback 和回到底部；不写入 SQLite，也不跨应用重启恢复。
- 日志中心由统一 `LogQuery`/`LogSnapshot`/流式事件 IPC 聚合 system journal、systemd、固定 Nginx access/error 路径和 Docker/Compose 适配器；follow 使用可取消 Channel、30 秒服务端上限和有界前端缓冲，不提供任意远程文件日志。
- Overview 采样成功后写入本地短期 `metric_samples`，不会改变远程服务器，也不依赖远程常驻服务。
- 所有远程命令/流式长任务通过 `task_records` 保存状态、进度和可安全重试元数据；应用启动时只将 queued/running 标记为 `interrupted`，任务中心只导航回原模块供用户手动确认，禁止自动重放。

## 本地短期数据边界

- `metric_samples` 只保存 CPU、内存、load、网络速率、磁盘使用率和采样时间，按 24 小时与每服务器 20,000 条滚动清理。
- `task_records` 不保存密码、私钥、sudo 凭据、终端输入、命令输出或 Docker 敏感环境变量；错误文本经过截断和脱敏。
- 快捷指令模板与日志来源/目标属于本地配置和查询元数据；日志正文仅在内存中按固定行数和字节数展示。
