# 1Panel Client 交接入口

最后更新：2026-08-22（Asia/Shanghai）

当前版本：`0.1.0`。项目从 `server-manager` 的真实 SSH/SFTP/Tauri 能力内核派生，产品壳与导航按 1Panel 社区版 v2.2.5 重建，并新增客户端特有的多服务器节点切换器。网站、安全、数据库、计划任务和高级探活已落到真实远端命令；完整覆盖范围见功能矩阵。

## 必读顺序（下一位 Agent 拿到本文件后按此顺序读）

1. `README.md` — 产品定位、技术栈、开发命令、验证命令与许可证。
2. `docs/HANDOFF_PROTOCOL.md` — 跨 Agent 交接规范：哪些文档必须更新、如何记录事实、如何验收。
3. 本文件 `HANDOFF.md` — 当前阶段、最近证据、测试环境与第一条任务。
4. `docs/CURRENT_STATE.md` — 当前代码事实、模块地图、已完成/部分完成/未完成与技术债。
5. `docs/NEXT_STEPS.md` — 按依赖排序的下一步、大阶段视觉验收流程与固定质量门。
6. `docs/FUNCTION_MATRIX.md` — 1Panel v2.2.5 模块覆盖与剩余缺口。
7. `docs/SECURITY.md` — 凭据、Host Key、sudo、WebView 与远端安全边界。
8. `docs/ARCHITECTURE.md` — SSH/SFTP/Tauri 分层与远端安全边界。
9. `docs/ACCEPTANCE.md` — 证据驱动的里程碑/场景状态。
10. `docs/TEST_SERVER.md` — 测试服务器 SSH 凭据与 1Panel 面板链接（已授权可随意操作）。
11. `AGENTS.md` — 持续生效的开发行为规则。

## 当前已完成

- 独立项目名、Tauri product/identifier、开发端口和 Rust crate 已改为 1Panel Client。
- 1Panel 蓝白 token、180/75px 折叠侧栏、42px 菜单、白卡片和底部状态栏已接入。
- 多服务器节点切换器、节点总览、多级 ProxyJump（链路展示与循环检测）、全节点 ProxyJump 拓扑批量诊断（自引用/孤儿/循环/超长）和原有真实服务器工作区已接入。
- 系统页新增真实 lsblk/findmnt/df/fstab 磁盘与挂载管理；挂载、卸载和 fstab 变更均经过路径/文件系统/选项校验，并在 dry-run 失败时恢复备份。
- 高级页新增 OWASP CRS 4.25.1 LTS/4.28.0 固定 URL+SHA-256 来源；宿主机与 OpenResty/NGINX 容器均使用备份、配置测试、reload 和失败回滚的安装/更新/移除流程。
- 计划任务备份支持 UTC 时间戳归档以及按份数/天数清理同前缀文件；保留策略写入独立 marker、随 JSON 导出/导入并在真实测试服务器完成轮换 smoke；设置页支持本机目录/WebDAV/S3/SFTP 账号，secret 进入系统密钥链，手动归档可经 SFTP/WebDAV/S3 原子上传；客户端在线调度器每 60 秒读取有界远端事件文件，补传离线归档、去重记录账号状态并发送一次成功/失败报告；计划任务页支持通用 JSON/Slack/Discord/钉钉/企业微信报告通知。
- 概览、文件、终端、进程/端口、systemd、工具、Nginx/OpenResty、Docker、应用商店、数据库、计划任务、安全、日志、设置和高级探活均走 Rust typed IPC；概览另有按真实能力生成推荐卡、本机 SQLite 备忘录和 root/sudo、关键命令及 1Panel/OpenResty/Docker 目录权限诊断；应用商店官方/静态镜像来源、TTL 缓存/离线回退/最多 8 个有序镜像节点故障转移/缓存清理、升级回滚/卸载后恢复、升级前 Compose 哈希/行数差异预览、官方分支提交号同步显示、环境变量合并保存、应用健康检查、Compose project label 清理候选与 Docker Events 有界摘要、Docker Build（构建参数/流式输出/取消）、ACME HTTP-01/DNS-01（Cloudflare/阿里云/DNSPod/腾讯云/AWS Route 53 acme.sh）、PHP-FPM 安装与站点绑定、同域受控站点证书自动绑定、WAF 规则/4 个固定策略模板（敏感文件/危险方法/扫描器标识/备份遗留文件）/主机及匹配容器日志拒绝摘要/阈值/本地历史/系统密钥链 webhook（通用 JSON、Slack、Discord、钉钉、企业微信；钉钉可选 HMAC 签名）、SSH Agent、Redis ACL 用户创建/删除/授权/撤销/重置密码、键/字符串/hash/list/set/zset 受控编辑、复杂值 DUMP/RESTORE 快照与支持目标 AUTH/AUTH2 的 MIGRATE 跨实例迁移、数据库真实权限矩阵与 apt/dnf/apk/pacman 安装后命令/版本/systemd/OpenRC 验证、计划任务 Shell/URL/目录/网站/应用备份/数据库备份/日志备份类型、URL 多地址固定 curl、网站容器路径映射、应用 Compose 目录实时解析、版本化 JSON 任务导出/确认导入、本地执行历史清理、立即运行结果脱敏后的本地历史、HTTP 探活任务持久化/调度/历史，以及带步数上限的只读服务器概览 AI 智能体、可取消 AI SSE、真实 `/models` 探测、按供应商隔离的本地 AI 会话恢复/清理和 MCP stdio/远程 HTTP-SSE 工具发现/调用已接入；应用商店还可生成静态镜像目录、catalog.sig 并用操作系统密钥链验签。
- 测试服务器已完成真实 SSH 连接、OpenResty 临时站点创建/删除、UFW 临时规则添加/删除、数据库探测、数据库安装计划读取、应用商店目录/已安装 OpenResty 环境键摘要与远程 HTTPS 探活；临时资源已清理，未执行数据库、PHP、WAF 规则或应用安装。
- GPL-3.0、上游来源、独立社区客户端声明与功能覆盖矩阵已加入。
- 本轮新增能力（均已通过前端 lint/typecheck/test/build 与 Rust test/clippy）：
  - WAF 内置策略模板由 4 个扩为 7 个（新增路径穿越/SQL 注入/XSS 拦截，规则正文固定在 Rust 端，重复规则 ID 防护）。
  - 工具中心由 12 项扩为 18 项（新增 fail2ban/certbot/tree/ncdu/iperf3/make，并扩展守护进程运行检测）。
  - Monaco 与 ECharts 拆分到懒加载 chunk，入口 chunk 由约 1.6MB 降到约 435KB（gzip 136KB）。
  - 多节点 ProxyJump 拓扑批量诊断：对全部档案检测自引用/孤儿/循环/超长，并在节点总览展示告警。
  - 终端新增左右分屏会话（独立 PTY，可关闭/重开）。
  - AI 只读 function-calling 工具由 4 个扩为 5 个（新增 server_processes，按 CPU 排序的进程与监听端口摘要）。
  - 数据库新增 Redis 只读连接诊断（PING 延迟/版本/角色/连接数/内存）与权限矩阵安全诊断（通配主机/全库授权/ALL 权限/Redis 过宽 ACL）。
  - 系统-存储：读取真实 lsblk 后自动汇总块设备拓扑（磁盘/分区/RAID 阵列/LVM 卷/其他设备计数），在磁盘页顶部展示摘要；后端新增 `StorageTopology` 与 `compute_topology`，并补充单测。
  - 数据库权限矩阵安全诊断扩展为按引擎区分：MySQL/MariaDB 的 Grant Option（可继续授权）、PostgreSQL 的跨库 CREATE（创建范围过广）；并修复前端权限面板，让诊断列表在任意权限矩阵下始终展示（此前只在无权限条目时显示）。
  - 网站：静态站点支持填写「容器内 PHP socket」覆盖自动探测，用于容器化 OpenResty 无法直连宿主机 PHP-FPM socket 的场景；后端校验后直接生成 `fastcgi_pass`，仅允许静态站点。
  - 网站证书批量策略（部分完成）：后端已新增 `certificate_renewal_plan`（按到期天数识别缺失/即将到期的启用 HTTPS 站点）及单测；已注册 IPC `get_certificate_renewal_plan`，前端已接入类型、API 与「证书批量策略」面板 JSX，但**面板 CSS 尚未补充，也未做视觉/真实连接验收**，见 `docs/NEXT_STEPS.md` 第一条。

## 本次验证

- `pnpm lint`、`pnpm typecheck`：通过。
- `pnpm test --run`：通过，6 个测试文件 / 13 个测试。
- `pnpm build`：通过；Monaco 与 ECharts 均已拆分到懒加载 chunk（文件/容器/概览页按需加载），入口 chunk 降到约 435KB/gzip 136KB；仅保留页面级 chunk 体积提醒。
- `cargo fmt --all --check`、严格 Clippy：通过。
- `cargo test --all-features`：通过，162 个测试通过、5 个 ignored、0 失败（本轮新增 WAF 模板、工具箱、ProxyJump 拓扑诊断、Redis 连接诊断、权限矩阵诊断、存储拓扑、数据库引擎差异诊断、网站容器 PHP socket 覆盖共 14 个单测）；另有显式环境变量控制的真实目录备份轮换、网站/应用备份、外部 SFTP 账号上传、存储快照与 WAF CRS 能力 SSH smoke 通过。
- `pnpm tauri build`：本轮通过，已生成 Windows x64 MSI/NSIS；release 使用 NASM 3.02。
- `.github/workflows/package.yml`：已配置 tag/手动触发的 Linux x64、macOS 和 Windows x64 Tauri bundle 矩阵；Linux 依赖与 Windows NASM 会在 runner 自动安装，跨平台产物尚未在本机执行。
- 本地浏览器视觉验收：展开/折叠侧栏、空节点首屏、节点切换器和 720px 高度布局通过；控制台无 error/warn。
- 未完成/未验证：本轮新增的「网站证书批量策略」前端面板已接 JSX 但 **CSS 尚未补充**，且未进行桌面应用真实启动与连接验收；此项目已列入 `docs/NEXT_STEPS.md` 第一条，不视为完成。

NASM 已安装在 `C:\Users\dd\AppData\Local\Programs\NASM\3.02\nasm-3.02`。Windows Rust/release 命令需在 Visual Studio 2022 Developer Command Prompt 中，并将该目录加入当前进程 `PATH`。

本轮 release 产物（2026-08-21）已包含当前代码：

  - `src-tauri/target/release/bundle/msi/1Panel Client_0.1.0_x64_en-US.msi` — 15,958,016 bytes，SHA-256 `EBA4C9A76EBF507C45A782CA9EF59A56C07B12AD2B2F646B148C37270C4D84BD`
  - `src-tauri/target/release/bundle/nsis/1Panel Client_0.1.0_x64-setup.exe` — 11,888,291 bytes，SHA-256 `679168DD3911A5E33C77477155D70D2F6E5CA5AA009C35054B30D1B736D689DA`
  - `src-tauri/target/release/onepanel-client.exe` — 39,620,096 bytes，SHA-256 `7DC8F12675233E9C9205FCF8ACA36675ACE9A2387251E28389475D4F7C5A16AD`

## 下一步

1. 完成「网站证书批量策略」的收尾：补充 `certificate-renewal-panel` 相关 CSS，跑一次全量门禁，然后按本文件下方「大阶段视觉验收」流程用桌面应用真实操作验收。
2. 按 `docs/FUNCTION_MATRIX.md` 继续补齐跨客户端账号映射、更多对象存储协议与计划任务容灾，再推进更多第三方 CRS/签名规则源与策略编排、数据库权限模型差异诊断、AI 更多工具编排和真实第三方 MCP 兼容性，以及日志审计字段完全对齐与完整国际化。

## 测试环境

测试服务器 SSH 与 1Panel 面板凭据见 [`docs/TEST_SERVER.md`](docs/TEST_SERVER.md)。该服务器已授权可随意操作；真实写操作前仍需用户在客户端 UI 显式确认，首次连接必须人工核对 Host Key。1Panel 面板用于对标 v2.2.5 的 UI 与操作流程。

## 大阶段视觉验收（每个大阶段完成后必做）

按 `docs/HANDOFF_PROTOCOL.md` 收尾后，除代码门禁外，还必须做一次真实运行验收：

1. 先读 [`docs/TEST_SERVER.md`](docs/TEST_SERVER.md) 获取 SSH 与 1Panel 面板凭据。
2. 浏览器打开 `https://panel.wingeng.xyz/`，对照 1Panel 当前页面和操作流程。
3. 启动客户端：`pnpm tauri dev`，用测试服务器凭据登录（首次核对 Host Key）。
4. 逐个打开本轮改动涉及页面，验证加载、空态、错误态、表单、点击与结果，并与 1Panel 对照。
5. 把实际看到的结果、截图结论、异常与控制台错误写回 `docs/ACCEPTANCE.md` 和 `docs/CURRENT_STATE.md`。

前置条件：此流程依赖 Computer Use / Chrome 的 Node REPL（`node_repl js`）服务已挂到当前会话；未挂载时只能由用户手动操作，不能把静态构建当作视觉验收。

## 安全提醒

不要把测试机密码放入 shell、源码、fixture 或日志。首次连接必须人工核对 Host Key；未经用户在 UI 明确触发，不执行远端安装、删除或服务变更。
