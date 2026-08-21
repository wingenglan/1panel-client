# Acceptance Status

更新时间：2026-08-22

标记：`[x]` 已由自动化或真实环境验证；`[~]` 已有核心实现但仍有覆盖范围或平台证据待补；`[ ]` 尚未实现。

## 本地自动化门禁

- [x] `pnpm lint`、`pnpm typecheck`、`pnpm test --run`（6 files / 13 tests）
- [x] `pnpm build`（Vite production bundle；Monaco 主 chunk 体积仅为 warning）
- [x] `cargo fmt --all --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test --all-features`（162 passed / 5 ignored / 0 failed；真实目录备份轮换、网站/应用备份、外部 SFTP 上传、存储快照与 WAF CRS 能力 SSH smoke 另行通过）
- [x] `pnpm tauri build`（Windows x64 MSI + NSIS；使用 NASM 3.02）

## 已有核心实现

- [x] 多服务器档案、密码/私钥/SSH Agent SSH、严格 Host Key、多级 ProxyJump（链路展示与循环检测）、SFTP 文件、PTY 终端、系统概览、进程/端口、systemd、日志、任务中心、审计和安全存储。
- [~] 工具箱：能力探测与受控安装；更多发行版工具仍待补齐。
- [~] 网站：OpenResty/Nginx 静态站点、反向代理、配置测试/reload/回滚、证书元数据、certbot/acme.sh HTTP-01、Cloudflare DNS-01、acme.sh 阿里云/DNSPod/腾讯云/AWS Route 53 DNS-01、PHP-FPM 探测/安装计划与静态站点 FastCGI 绑定、同域证书自动绑定、静态站点可指定容器内 PHP socket 覆盖。证书批量策略后端/IPC/前端面板已接，但面板 CSS 未完成且未做视觉验收。
- [~] Docker/Compose：容器、镜像、卷、网络、日志、Inspect、Stats、Exec、Pull/Run/Build（构建参数、流式输出、取消）、最近 Events 摘要和项目生命周期；完整 Docker Desktop 级细节仍待补。
- [~] 应用商店：Compose 目录、安装、启动/停止/重启/卸载与卸载后恢复、日志、官方最新模板升级回滚、升级前 Compose 哈希/行数差异预览、官方分支提交号同步显示、环境变量合并编辑、容器健康检查和基于 Docker project label 的清理候选预览；本地 TTL 缓存、离线回退、最多 8 个有序静态镜像节点故障转移、缓存清理已接入；可生成包含 metadata/版本 Compose/env 的静态镜像目录，并生成 HMAC-SHA256 catalog.sig、在客户端使用操作系统密钥链验签。
- [~] AI：OpenAI-compatible 供应商、系统密钥链、真实 Chat Completions 与可取消 SSE 流式输出、真实 `/models` 能力探测；只读服务器概览智能体（function calling、步数上限、真实 SSH 工具）、按供应商隔离的本地会话恢复/清理和 MCP stdio/远程 HTTP-SSE JSON-RPC 工具发现/调用已接入，远程 Bearer 令牌只存系统密钥链；更多工具编排和真实第三方 MCP 兼容性仍需验收。
- [~] 数据库：MySQL/MariaDB/PostgreSQL/Redis 探测、数据库/用户权限、备份恢复、服务动作与 apt/dnf/apk/pacman 引擎安装计划；安装命令按 Debian/RHEL/Alpine/Arch 适配，服务启动兼容 systemd/OpenRC；Redis ACL 用户创建/删除/授权/撤销/重置密码、键摘要/删除/FLUSHDB、字符串值读写/可选 TTL、hash/list/set/zset 受控逐项编辑、远端 DUMP/RESTORE 复杂值快照导入导出、当前会话默认用户/ACL 用户认证，以及支持 AUTH/AUTH2、保留类型/TTL 的 MIGRATE 跨实例/跨版本逐键迁移已接入；数据库级权限模型差异和更多安装后诊断待补。
- [~] 计划任务：真实 crontab Shell/URL/目录/网站/应用/数据库/日志备份类型创建；网站按远端受控域名解析并兼容 OpenResty 容器挂载，应用按已安装 Compose 目录解析；URL 多地址固定 curl 请求，备份使用固定 tar/mysqldump/pg_dump 模板、UTC 时间戳文件名、同目录临时文件和原子替换，并可按份数/天数清理同前缀归档；运行/受控删除与 systemd timer 展示；立即运行结果脱敏/截断后本地保留最近 200 条执行历史，版本化 JSON 导出/确认导入和本地历史清理已接入；本机目录/WebDAV/S3/SFTP 外部账号、密钥链 secret、手动归档流式上传和报告通知已接入；客户端在线调度器会读取有界远端事件文件，补传离线归档并发送一次成功/失败报告。
- [~] 安全：UFW/firewalld/nftables 规则快照与受控变更、SSH 配置备份/校验/reload；系统页已接入真实 lsblk/findmnt/df/fstab 快照、挂载/卸载和带 dry-run 回滚的 fstab 变更；分区格式化、RAID/LVM 与发行版专属设置待补。
- [~] 高级功能：WAF/ModSecurity 能力探测、宿主机及 OpenResty/NGINX 容器内规则读取与受控增删（备份、配置测试、reload 失败回滚）、内置 OWASP CRS 4.25.1 LTS/4.28.0 固定 SHA-256 来源（宿主机/容器安装、更新、移除与失败回滚）、主机及匹配 Docker 容器日志拒绝摘要、warning/error/critical 阈值、本地历史、每小时趋势、应用内提示和系统密钥链 webhook 外部通知（通用 JSON、Slack、Discord、钉钉、企业微信；钉钉可选 HMAC 签名）、远程 HTTP 状态/延迟探活；探活任务持久化、到期调度、失败记录和历史列表已接入，更多第三方签名源与策略编排待补。

## 测试服务器真实证据

- [x] 通过客户端 UI 建立密码 SSH 会话并完成 Host Key 核对。
- [x] 创建临时 OpenResty 静态站点，配置测试/reload 成功，随后通过 UI 删除；站点列表恢复原状。
- [x] 添加临时 UFW 高位端口规则，确认规则出现，再通过 UI 删除；规则数量恢复原状。
- [x] 数据库页读取真实远端引擎/用户探测结果；测试机当前未安装可管理数据库引擎，页面正确显示空状态。
- [x] 高级功能页检测远端 curl/WAF 能力，并从服务器探活 `https://example.com` 返回 HTTP 200 与延迟。
- [x] 真实测试服务器只读 CRS 能力 smoke：固定 2 个来源可读取；本次仅展示来源与能力状态，未执行远端下载、安装或 reload。
- [x] 应用商店读取官方目录（263 个模板），读取测试机已安装 OpenResty 项目，并打开环境变量键摘要；远端秘密只显示掩码，未执行安装/升级/环境写入。
- [x] 上述临时资源均已清理；没有写入用户业务数据。
- [x] 计划任务真实 SSH smoke：目录备份按 2 份/30 天策略生成 UTC 时间戳 tar.gz 并验证轮换命令，网站和已安装 Compose 应用备份 marker 写入、版本化导出、导入新 marker、手动运行、受控删除与临时资源清理闭环成功。
- [x] 外部 SFTP 备份账号真实 SSH smoke：从测试节点读取临时归档，通过独立 SFTP 会话上传到临时目标目录，校验内容后删除账号、远端目录和密钥链条目。
- [x] 存储真实 SSH smoke：读取测试服务器 lsblk/findmnt/df/fstab，确认返回根挂载点；未执行挂载、卸载或 fstab 写入，未改变远端状态。
- [~] 文件上传/下载、终端 PTY、Docker 生命周期、应用安装等更广泛真实流程仍需后续逐项验收。

## 大阶段视觉运行验收

- [ ] 浏览器打开 `https://panel.wingeng.xyz/`，对照 1Panel v2.2.5 的当前页面与操作流程。
- [ ] 启动客户端 `pnpm tauri dev`，用 [`TEST_SERVER.md`](TEST_SERVER.md) 的 SSH 凭据登录并首次核对 Host Key。
- [ ] 逐页打开本轮改动涉及页面，验证加载/空态/错误态/表单/点击/结果，并与 1Panel 对照。
- [ ] 把截图结论、异常与控制台错误写回本文件与 `docs/CURRENT_STATE.md`。

前置条件：Computer Use / Chrome 的 Node REPL（`node_repl js`）服务已挂到当前会话。未挂载时只能由用户手动操作，不能把静态构建当作视觉验收。

## 安装包

- [x] MSI/NSIS：本轮产物已生成并校验 SHA-256。
  - `1Panel Client_0.1.0_x64_en-US.msi` — 15,958,016 bytes；`EBA4C9A76EBF507C45A782CA9EF59A56C07B12AD2B2F646B148C37270C4D84BD`
  - `1Panel Client_0.1.0_x64-setup.exe` — 11,888,291 bytes；`679168DD3911A5E33C77477155D70D2F6E5CA5AA009C35054B30D1B736D689DA`
  - `onepanel-client.exe` — 39,620,096 bytes；`7DC8F12675233E9C9205FCF8ACA36675ACE9A2387251E28389475D4F7C5A16AD`

剩余升级、安装后启动/卸载、macOS/Linux 包和 1Panel 专属深度能力，按 [`FUNCTION_MATRIX.md`](FUNCTION_MATRIX.md) 继续迭代；不把未验证项标记为完全完成。
