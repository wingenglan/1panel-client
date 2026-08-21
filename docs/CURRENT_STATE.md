# Current implementation state

最后更新：2026-08-21（Asia/Shanghai）。本文记录 `1panel-client` 的当前代码事实和可复现证据；模块边界与剩余缺口见 [`FUNCTION_MATRIX.md`](FUNCTION_MATRIX.md)。

## 仓库与技术栈

- 项目：`1panel-client`，不修改原 `server-manager`。
- Tauri 2 + Rust stable + Tokio；React 19 + TypeScript + Vite + React Router。
- TanStack Query、Zustand、Radix UI、ECharts、Monaco、xterm.js、russh、russh-sftp、SQLite/sqlx、OS Keychain。
- Windows 打包使用 Visual Studio Build Tools 2022、Rust MSVC、WebView2 和 NASM 3.02。

## 已实现模块

### 客户端与连接

- 多服务器档案、分组、收藏、复制、搜索和节点切换；支持通过已认证跳板建立多级 ProxyJump SSH 会话，表单显示链路，保存和连接前均拒绝循环、孤儿引用和超长链路。
- 密码/私钥/SSH Agent SSH、严格 Host Key trust/change detection、可复用会话、断线状态。
- SFTP 浏览、上传/下载、冲突策略、Monaco 编辑、原子保存、sudo 保存、Copy/Move/chmod/symlink。
- 多标签真实 PTY、resize、搜索、快捷指令、短期命令历史和任务中心。

### 运维工作区

- 概览：CPU/RAM/disk/load/network、系统身份、运行时、挂载点、top 进程和短期趋势图；根据真实能力/异常生成推荐卡，并按服务器在本机 SQLite 设置中保存备忘录（不发送到远端）；新增 root/sudo、关键命令以及 1Panel/OpenResty/Docker 目录访问权限诊断。
- 系统：端口/进程搜索、SIGTERM/SIGKILL 确认、systemd 状态/日志/启停/启用/禁用；磁盘页读取真实 lsblk/findmnt/df/fstab，挂载/卸载与 fstab 写入/移除均经过路径、文件系统和选项校验，并在变更前备份、dry-run 验证失败时自动回滚。
- 工具：平台能力探测、apt/dnf/apk/pacman 包管理器安装计划、流式安装与验证；数据库服务动作和安装命令兼容 systemd/OpenRC。
- Docker/Compose：容器、镜像、卷、网络、日志、Inspect、Stats、Top、Exec、Pull/Run/Build（构建参数、流式输出与取消）、最近 Docker daemon Events 有界摘要和项目生命周期；Compose 详情会从远端 project labels 计算实际卷、网络和孤儿容器清理候选，并提示 external/未创建资源。
- 日志/审计/诊断：systemd、Nginx、Docker/Compose 固定来源查询、follow、搜索、脱敏导出和本地审计。

### 1Panel 对齐模块

- 网站：OpenResty/Nginx 静态站点、反向代理、HTTPS 证书元数据、certbot/acme.sh HTTP-01 签发/续期、certbot Cloudflare DNS-01、acme.sh Cloudflare/阿里云/DNSPod/腾讯云/AWS Route 53 DNS-01（临时 0600 环境文件）、PHP-FPM 探测/安装/站点 FastCGI 绑定、同域受控站点证书自动绑定、配置测试/reload/回滚和启停/删除。
- 安全：UFW/firewalld/nftables 快照与受控规则增删，SSH 配置读取/备份/校验/reload。
- 数据库：MySQL/MariaDB/PostgreSQL/Redis 探测，数据库/用户/权限与真实数据库级权限矩阵，SQL 备份/恢复、服务生命周期与 apt/dnf/apk/pacman 引擎安装计划/可取消执行/安装后验证；验证同时检查客户端命令、版本和 systemd/OpenRC 状态，并兼容 PostgreSQL 版本化服务名。Redis 支持 ACL 用户创建/删除/授权/撤销/重置密码与脱敏规则读取、键扫描、类型/TTL/内存摘要、删除、确认后的 FLUSHDB、字符串值读取/写入和可选 TTL、hash/list/set/zset 受控逐项编辑、远端 DUMP/RESTORE 复杂值快照导入导出，以及支持目标 AUTH/AUTH2、保留类型/TTL 的 MIGRATE 跨实例/跨版本逐键迁移。
- 应用商店：读取官方 GitHub appstore 或用户配置的静态镜像目录、Compose 模板详情、安装/启动/停止/重启/卸载、卸载后恢复和日志；目录/详情使用本地 TTL 缓存并支持离线回退、最多 8 个有序镜像节点故障转移、缓存清理，更新会拉取当前来源最新模板并备份回滚，升级前可在远端计算当前/最新 Compose 哈希与行数差异，环境变量支持键摘要与合并保存，已安装应用可读取容器健康摘要；应用商店页面可生成包含 metadata、版本 Compose、env 和 catalog.json 的静态镜像目录，写出 HMAC-SHA256 catalog.sig，并用操作系统密钥链中的令牌验签。
- 计划任务：真实 crontab Shell/URL/目录/网站/应用/数据库/日志备份类型创建/运行/受控删除；URL 地址按 HTTP/HTTPS、主机、认证信息、数量和长度校验后生成固定参数 curl 命令；网站按实时受控域名解析并将 OpenResty 容器运行时根目录映射到宿主机挂载目录，应用按固定已安装 Compose 目录解析；目录/网站/应用/日志备份使用固定 tar.gz 模板，数据库备份使用 mysqldump/pg_dump，并通过同目录临时文件和原子替换避免半成品；备份可用 UTC 时间戳文件名按份数/天数清理同前缀归档；任务类型用独立 marker 保存，旧 Shell 任务保持兼容；systemd timer 只读展示；立即运行结果经脱敏/截断后本地保留最近 200 条执行历史；支持版本化 JSON 导出/确认导入（受支持类型保留 marker，未知类型安全降级为 Shell）和本地历史清理；同一客户端且凭据有效的备份账号引用会恢复，失效或跨客户端引用会被剔除并计数；归档任务写入有界事件文件，客户端在线调度器会补传离线归档并发送一次成功/失败报告，调度开关可在设置页关闭。
- 计划任务外部账号与通知：设置页支持本机目录、WebDAV、S3 兼容对象存储和 SFTP 账号；公共字段存本地 SQLite，密码/secret 存操作系统密钥链，SFTP 支持密码或私钥认证及可选 SHA-256 Host Key 指纹；手动运行归档后客户端从远端下载并以流式/临时文件方式原子上传，S3 使用流式 SHA-256 + Signature V4，SFTP/WebDAV 也使用临时目标替换；报告通知支持通用 JSON、Slack、Discord、钉钉 HMAC 和企业微信，通知失败只写入脱敏执行摘要，不覆盖任务本身结果。客户端在线调度器每 60 秒读取远端有界事件文件，对离线期间完成的归档补传并发送一次成功/失败报告；账号上传状态和调度开关仅保存在本机。
- AI：OpenAI-compatible 供应商、OS Keychain、真实 Chat Completions 与可取消 SSE 流式响应、真实 `/models` 探测；只读服务器智能体通过 function calling 调用真实 SSH 工具，提供概览、网站、Docker、安全四类有界快照并限制最多 6 步；本地会话按供应商持久化、恢复、删除和清理，历史只保存脱敏消息引用，key 不写入 SQLite/日志；可选 MCP stdio 或远程 HTTP/SSE JSON-RPC initialize/tools/list/tools/call、Bearer 令牌密钥链保存、会话 ID、工具命名空间、结果上限和默认只读策略已接入。
- 高级功能：WAF/ModSecurity 能力探测、宿主机及 OpenResty/NGINX 容器内固定规则文件读取与受控 SecRule/SecAction 增删（备份、配置测试、reload 失败回滚）、3 个后端固定的敏感文件/危险方法/扫描器标识策略模板（重复规则 ID 防护）、内置 OWASP CRS 4.25.1 LTS/4.28.0 固定 URL+SHA-256+签名指纹来源（宿主机/容器安装、更新、移除和失败回滚）、固定主机日志及匹配 Docker 容器日志拒绝事件摘要、warning/error/critical 阈值过滤、本地有界历史与出现次数、每小时趋势、30 秒轮询的应用内新增告警提示、系统密钥链保存的通用 JSON/Slack/Discord/钉钉/企业微信 webhook 通知（钉钉可选 HMAC 签名）、远程 HTTP/HTTPS 状态码与延迟探活；探活任务持久化、15 秒调度轮询、失败记录和最近历史已接入。

## 关键安全边界

- React 不构造裸远程 shell；Rust domain 只生成固定命令和经过 shell escaping 的参数。
- 密码、私钥口令、sudo 凭据和 AI/API key 只在 IPC/Rust/OS Keychain 短暂流转，不进入 SQLite、LocalStorage、日志、审计或诊断包。
- Host Key 首次连接必须人工核对；密钥变化直接阻断连接。
- 远程写入使用明确确认、临时文件、备份、配置测试、reload 后验证和失败回滚。
- Docker 只通过 SSH 上的 CLI，不开放 Docker TCP API。

## 自动化证据（2026-08-21）

```text
pnpm lint                         PASS
pnpm typecheck                    PASS
pnpm test --run                   PASS (6 files, 13 tests)
pnpm build                        PASS (Vite production; chunk-size warning only)
cargo fmt --all --check           PASS
cargo clippy --all-targets ...    PASS (-D warnings)
cargo test --all-features         PASS (148 passed, 5 ignored, 0 failed; ignored 项为显式环境变量控制的真实备份/存储/WAF SSH smoke)
real cron backup SSH smoke         PASS (目录 2 份/30 天轮换、网站、应用备份；marker 写入、导出、导入新 marker、tar.gz 生成、手动运行、删除与清理)
pnpm tauri build                  PASS (Windows x64 MSI + NSIS; NASM 3.02)
release smoke launch              PASS (运行 8 秒后安全退出)
```

## 测试服务器证据

- 客户端 UI 密码登录成功，Host Key 指纹已核对并保存。
- 临时 OpenResty 静态站点创建、配置测试/reload、列表确认和删除闭环成功。
- 临时 UFW 高位端口规则添加、列表确认和删除闭环成功；规则恢复原状。
- 数据库页真实探测成功；测试机当前无可管理数据库引擎，页面正确显示空状态。
- 高级页真实探活 `https://example.com` 成功返回 HTTP 200 与延迟；远端 curl/WAF 能力状态正确显示。
- 临时站点和防火墙规则均已清理，没有写入业务数据。
- 应用商店真实读取官方目录 263 个模板和测试机已安装 OpenResty 环境键摘要；秘密值只显示掩码，未触发安装/升级/环境写入。静态镜像生成器、catalog.sig 签名和客户端验签已通过单元测试，未向测试服务器写入镜像资源。
- 计划任务真实 SSH smoke 已创建临时目录和网站备份任务，并使用测试机已安装 Compose 应用验证应用备份；目录任务启用 2 份/30 天保留策略，验证 UTC 时间戳 tar.gz 生成与清理命令；同时验证 marker 写入、版本化导出、导入新 marker、手动执行、受控删除和临时资源清理；未保留远端业务数据。
- 存储真实 SSH smoke 只读读取测试机 lsblk/findmnt/df/fstab，确认根挂载点和容量字段可解析；未执行挂载、卸载或 fstab 写入，远端状态未改变。
- WAF CRS 真实 SSH smoke 只读读取固定 2 个规则源及能力状态；未执行下载、安装或 reload，远端状态未改变。

## Release 产物

本轮 Windows x64 release 已重新生成并通过 Tauri bundler（2026-08-21）：

- `src-tauri/target/release/bundle/msi/1Panel Client_0.1.0_x64_en-US.msi` — 15,958,016 bytes；SHA-256 `EBA4C9A76EBF507C45A782CA9EF59A56C07B12AD2B2F646B148C37270C4D84BD`
- `src-tauri/target/release/bundle/nsis/1Panel Client_0.1.0_x64-setup.exe` — 11,888,291 bytes；SHA-256 `679168DD3911A5E33C77477155D70D2F6E5CA5AA009C35054B30D1B736D689DA`
- `src-tauri/target/release/onepanel-client.exe` — 39,620,096 bytes；SHA-256 `7DC8F12675233E9C9205FCF8ACA36675ACE9A2387251E28389475D4F7C5A16AD`

## 仍需补齐的 1Panel 深度能力

更多第三方 CRS/签名规则源与策略编排、数据库权限模型差异诊断、AI 更多工具编排和真实第三方 MCP 兼容性仍按功能矩阵继续开发；`.github/workflows/package.yml` 已加入 Linux/macOS/Windows Tauri bundle 矩阵，本机未在 Windows 外执行跨平台构建。固定日志告警摘要、阈值过滤/本地历史、每小时趋势、30 秒轮询的应用内提示、匹配容器日志摘要、宿主机及容器内 WAF 规则文件安全编辑、内置 WAF 策略模板、OWASP CRS 固定来源安装/更新/移除回滚、系统密钥链 webhook 通知、多级 ProxyJump 链路与循环检测、只读服务器智能体、MCP stdio/远程 HTTP-SSE 工具、可取消 AI 流式请求、真实 `/models` 能力探测、本地 AI 会话持久化、Compose 卸载后恢复、升级前 Compose 差异预览、官方分支提交号显示、基于真实 Docker label 的清理候选预览、支持源/目标 AUTH/AUTH2 的 Redis MIGRATE 逐键迁移、Redis 页面当前会话认证、数据库权限矩阵和可取消数据库安装、apt/dnf/apk/pacman 与 systemd/OpenRC 发行版适配、静态镜像目录生成与 catalog.sig 验签已落地。
