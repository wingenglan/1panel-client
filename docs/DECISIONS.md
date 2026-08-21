# Engineering Decisions

## ADR-001 · SQLite 使用 sqlx

状态：Accepted

原因：应用服务以 Tokio 为运行时，sqlx 提供异步连接池、migration 与强类型行映射，避免在 command handler 中阻塞 WebView 线程。

## ADR-002 · 凭据使用 OS Keychain

状态：Accepted

使用 `keyring` 抽象 Windows Credential Manager、macOS Keychain 和 Linux Secret Service。生产路径不会在 Keychain 不可用时退回明文文件；只在单元测试注入内存实现。

## ADR-003 · SSH/SFTP 使用 russh 生态

状态：Accepted

选择 async 纯 Rust 会话以支持连接复用、多 channel、PTY、严格 Host Key handler 与后续 `russh-sftp`。不依赖本机 ssh/sftp 命令。

## ADR-004 · Docker 采用 CLI over SSH

状态：Accepted

优先解析 `--format json`、inspect JSON 与真实 streaming 输出，不开放 daemon TCP，不要求本机 Docker。

## ADR-005 · 独立 managed Nginx 配置

状态：Accepted

GUI 新建代理写入已被现有配置 include 的独立目录；保存采用 backup → 临时文件 → 原子替换 → `nginx -t` → reload/rollback，未知指令保持原文。

## ADR-006 · 产品视觉

状态：Accepted

采用工业控制台风格的深色高密度界面，以酸绿色作为可用/在线状态强调色，以橙红色统一危险操作语义；不复制任何既有管理产品的专有界面。
