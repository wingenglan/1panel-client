# Next steps

本文记录 `1panel-client` 的后续迭代顺序，不把未实现的 1Panel 深度能力伪装成已完成。

## 当前状态

1. 「网站证书批量策略」已收尾完成（2026-08-28）：CSS 与空态/阈值行为、站点增删后按 key 失效重取已落地；已跑全量门禁并在测试服务器完成桌面应用真实运行验收（创建 HTTPS 受控站点→面板真实行→阈值 30/10 空态/365→UI 删除→SSH 清理→`openresty -t` 通过）与 1Panel 面板网站/证书页对照，详见 `docs/ACCEPTANCE.md`。下一位 Agent 可直接从下方「下一轮优先级」继续。

## 开发节奏

- 不要在每写完一个小改动后就跑全量测试；**一个大阶段完成后**才统一跑一次 `pnpm lint/typecheck/test/build` 与 `cargo fmt/clippy/test`。
- 每个大阶段收口后，除代码门禁外，还必须做真实运行验收：启动客户端、用测试服务器登录、逐页点击查看效果，并打开 `https://panel.wingeng.xyz/` 对照 1Panel 的 UI 与操作。
- 测试服务器与面板凭据见 `docs/TEST_SERVER.md`；视觉验收前置条件是 Computer Use / Chrome 的 Node REPL 服务已挂到当前会话。

## 已完成的当前基线

- 多服务器 SSH/SFTP、Host Key、多级 ProxyJump（链路展示与循环检测）、终端、文件、系统状态、进程/端口、systemd、真实磁盘/挂载/fstab 管理、日志、任务中心和安全存储；概览包含 root/sudo、关键命令及管理目录的权限诊断。
- OpenResty/Nginx 静态站点与反向代理、配置测试/reload/回滚、证书元数据与 ACME HTTP-01/Cloudflare DNS-01/acme.sh 阿里云/DNSPod/腾讯云/AWS Route 53 DNS-01；PHP-FPM 探测、安装计划与静态站点绑定、同域受控站点证书自动绑定、宿主机及容器内 WAF 规则文件读取与受控增删、4 个后端固定 WAF 策略模板及重复 ID 防护、OWASP CRS 4.25.1 LTS/4.28.0 固定来源安装/更新/移除和失败回滚。
- Docker/Compose（含 Build 流式任务、Events 和清理预览）、应用商店（含官方/静态镜像来源、TTL 缓存、离线回退、缓存清理、升级回滚、升级前 Compose 差异预览、卸载后恢复、环境变量合并和容器健康检查）、真实数据库探测/用户权限（含 Redis ACL）/可取消引擎安装、Redis 键摘要/删除/FLUSHDB/字符串与 hash/list/set/zset 受控编辑/复杂值快照导入导出/当前会话认证/MIGRATE 跨实例迁移、Shell/多地址 URL/目录备份/数据库备份/日志备份 crontab/systemd timer（含 UTC 时间戳、按份数/天数轮换）、本机目录/WebDAV/S3/SFTP 备份账号、手动归档流式上传和通用/Slack/Discord/钉钉/企业微信报告通知、安全中心和高级 HTTP 探活。
- OpenAI-compatible 供应商配置、真实 `/models` 探测、真实 Chat Completions 与可取消 SSE 流式响应、只读服务器概览/网站/Docker/安全/进程快照 function-calling 智能体、可选 MCP stdio JSON-RPC 工具；API key 只进入本机系统密钥链。
- 高级探活任务持久化、到期调度、失败记录和最近历史列表；WAF 主机固定日志及匹配容器日志摘要支持 warning/error/critical 阈值、本地有界历史、每小时趋势、出现次数、30 秒轮询的应用内新增提示和系统密钥链 webhook 外部通知（通用 JSON、Slack、Discord、钉钉、企业微信；钉钉可选 HMAC 签名），宿主机及容器内规则能力已覆盖固定候选文件，提供 7 个后端固定策略模板和 OWASP CRS 固定来源安装/更新/移除回滚。
- Windows x64 release MSI/NSIS 已生成，NASM 3.02 已安装并纳入打包环境。

## 下一轮优先级

1. **WAF/告警**：继续补齐更多第三方 CRS/签名规则源和策略编排；OWASP CRS 4.25.1 LTS/4.28.0 已提供固定 URL/SHA-256 来源、宿主机/容器安装更新移除、配置测试/reload 失败回滚，既有规则摘要、安全编辑、4 个内置策略模板、日志摘要、阈值、应用内通知、本地历史、趋势、清理和 webhook 已接入。
2. **应用编排**：继续补齐完整 1Panel 编排细节；静态镜像目录生成器、HMAC-SHA256 签名校验、最多 8 个有序镜像节点故障转移、官方分支提交号同步显示、TTL 缓存/离线回退、升级前 Compose 哈希/行数差异预览、基于远端 Docker project labels 的清理预览和应用卸载后的 Compose 恢复已接入。
3. **数据库**：继续补齐权限模型差异诊断和 Redis 连接诊断；数据库级权限矩阵、apt/dnf/apk/pacman 安装后命令/版本/systemd/OpenRC 验证、Redis 源/目标 AUTH/AUTH2、当前会话凭据与安装流式取消已接入；权限矩阵安全诊断已覆盖 Redis 过宽 ACL、MySQL/MariaDB Grant Option 与 PostgreSQL 跨库 CREATE；新增能力先写 parser/安全校验测试。
4. **AI**：继续增加更多只读工具、远程 MCP 传输与认证；概览/网站/Docker/安全四类快照、按供应商隔离的本地会话持久化、恢复、删除和清理、模型能力探测、MCP stdio 工具发现/调用已接入，模型 key 由用户提供后再做真实验收。
5. **跨平台与质量**：运行 `.github/workflows/package.yml` 生成 Linux/macOS/Windows bundle，并完成安装后启动/卸载验收、更多发行版适配、断网/权限不足/取消路径测试和性能压测；多级 ProxyJump 链路与循环检测已接入。
6. **计划任务深度覆盖**：客户端在线调度器已通过有界远端事件文件补传离线归档并发送一次成功/失败报告；下一步继续补齐跨客户端账号映射、更多对象存储协议和远端代理容灾。

## 固定质量门

```bash
pnpm lint
pnpm typecheck
pnpm test --run
pnpm build
cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

涉及远端能力的改动必须在用户明确授权的测试节点执行真实读写，并记录临时资源的创建、验证和清理结果；涉及凭据的改动不得将 secret 写入源码、日志、SQLite 或命令行参数。

以上命令只在**一个大阶段收口时**运行一次。每个大阶段完成后，还必须完成 `HANDOFF.md` 中「大阶段视觉验收」一节：启动客户端真实操作并打开 1Panel 面板对照，结果写入 `docs/ACCEPTANCE.md`；不能只用静态构建代替视觉验收。
