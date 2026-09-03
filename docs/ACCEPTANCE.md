# Acceptance Status

更新时间：2026-08-29

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
- [~] 网站：OpenResty/Nginx 静态站点、反向代理、配置测试/reload/回滚、证书元数据、certbot/acme.sh HTTP-01、Cloudflare DNS-01、acme.sh 阿里云/DNSPod/腾讯云/AWS Route 53 DNS-01、PHP-FPM 探测/安装计划与静态站点 FastCGI 绑定、同域证书自动绑定、静态站点可指定容器内 PHP socket 覆盖。证书批量策略后端/IPC/前端面板已接，面板 CSS 完成，并已在测试服务器桌面应用真实运行验收（详见下节）。
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
- [x] 证书批量策略真实运行验收（2026-08-28）：通过客户端 UI 创建受控 HTTPS 静态站点 `renew-panel-test.wingeng.xyz`（临时自签证书，到期 2026-09-22，到期时间经容器内 `openssl x509 -enddate` 读取并在卡片/面板显示）；「证书批量策略」面板出现真实行（域名 + 即将到期徽标 + 到期时间 + 续期按钮）；提前续期天数在 30 显示行、10 显示空态（面板与阈值输入框保留）、365 仍显示行；随后经 UI 删除站点——站点卡片、面板消失，受控站点恢复 0（验证 onSuccess 失效重取生效）；SSH 核对 conf.d 无残留、`/opt/1panel/www/sites/renew-panel-test.wingeng.xyz` 已删除、`openresty -t` 通过、无业务数据残留。
- [~] 文件上传/下载、终端 PTY、Docker 生命周期、应用安装等更广泛真实流程仍需后续逐项验收。

## 大阶段视觉运行验收

- [x] 浏览器打开 1Panel 面板（http://8.138.151.118:28085/8fefacfdc6，已登录会话），对照 v2.2.5 的网站/证书页面：确认 1Panel 以表格呈现网站（名称/类型/网站目录/状态/协议/过期时间/证书过期时间/备注/操作）与证书（域名/申请方式/状态/自动续签开关/过期时间/操作栏「申请」等），行内操作按钮为文字式轻量按钮；客户端「证书批量策略」用卡片式行（域名 + 状态徽标 + 到期时间 + 申请/续期按钮 + 提前续期天数阈值）表达同一语义，徽标/到期时间/操作文案与 1Panel 约定一致，行操作用户按钮采用与卡片操作一致的轻量次级样式，无需调整代码。
- [x] 启动客户端 `pnpm tauri dev`，用 [`TEST_SERVER.md`](TEST_SERVER.md) 的 SSH 凭据登录并首次核对 Host Key（此前已完成，本轮继续使用）。
- [x] 打开本轮改动涉及页面（网站→证书批量策略），验证加载/空态（阈值高于到期日）/表单（阈值输入）/点击（续期→证书表单、删除→confirm→远端删除）/结果（受控站点计数、面板消失），与 1Panel 对照后无需额外修复；页面无报错。
- [x] 把截图结论与证据写回本文件与 `docs/CURRENT_STATE.md`。

前置条件：Computer Use / Chrome 的 Node REPL（`node_repl js`）服务已挂到当前会话。未挂载时只能由用户手动操作，不能把静态构建当作视觉验收。

## 布局 1:1 专项验收（2026-08-29，桌面实测）

背景：客户端此前为「顶部主控栏 + 内容工具条」布局，与 web 1Panel（左侧唯一菜单 + 内容页签 + 页面内子标签）不符；用户要求每个功能/样式先对照 web 版再在客户端同位置点击，逐项修正。

本轮修改（全部落地并通过 `npx tsc --noEmit` 与 `npm run build`，dev server localhost:1430 + onepanel-client.exe 实时生效）：

- 左侧改为唯一主导航（与 web 1Panel 相同的 概览/应用商店/AI/网站/数据库/容器/系统/终端/计划任务/工具箱/高级功能/日志审计/面板设置，AI/网站/系统/高级功能为可折叠分组，默认收起；「系统」含 文件/防火墙/进程管理/SSH 管理/服务 及 规划中 置灰项）；搜索/任务中心/添加服务器与节点切换器移入侧栏底部。
- 内容顶部改为 web 式页签条（`el-tabs` 样式，支持关闭），页签标题统一为「页面-子页」格式（面板设置-面板、容器-概览、应用-全部、工具箱-快速设置、日志审计-操作日志、数据库-MySQL、文件-文件等）。
- 全部页面移除原「workspace-header」标题行，操作按钮右移为顶部工具栏（.page-toolbar：刷新/创建/上传等）或与子标签同行（.page-tabbar：容器 事件/刷新、应用商店 刷新目录）；工具箱/日志审计将 180px 左栏导航改为顶部横向分段（快速设置/工具集/缓存清理…；面板日志/主机日志/网站日志 + 操作日志/任务日志）。
- 面板设置页：外观改为 亮色/深色/跟随系统 单选；「菜单标签页」由 checkbox 改为 web 式 启用/停用 单选，与页签条实时联动（localStorage + `1panel-client:prefs` 事件），并修复全局路由（面板设置/AI）下侧栏因 activeServerId 丢失而全禁用的问题。
- 应用商店新增「可升级」子标签（复用升级差异预览：未检查/已是最新/vN 可升级），不新增顶层入口。

桌面实测（UI Automation 点击 + 截图核对）：

- [x] 容器-概览：docker-tabs（概览/容器/镜像/网络/存储卷/编排）+ 右侧 事件/刷新；引擎/接口/系统/存储驱动/资源总览信息卡。
- [x] 工具箱-快速设置：横向子标签 + 只读/刷新/重启面板/重启服务器/DNS 等；页签「工具箱-快速设置」生成。
- [x] 日志审计-操作日志：顶部横向分段（面板日志/主机日志/登录日志/网站日志 + 操作日志/任务日志）+ 按需读取/刷新；表格列 时间/资源/操作/结果/服务器。
- [x] 应用商店：全部/已安装(1)/可升级/设置 + 刷新目录；可升级视图（比较说明、应用/项目/最新模板/操作 表头、openresty 行「未检查/检查升级」、底部安全说明）。
- [x] 面板设置-面板：无标题行；外观 亮色/深色/跟随系统 与 菜单标签页 启用/停用 radio 可点。
- [x] 主题回归（用户反馈「切换主题色后无法点击」）：深色→亮色后点击侧栏「概览」成功导航；亮色→深色后点击「数据库」「文件」均成功；radio 切换期间侧栏始终保持可点。
- [x] 菜单标签页 radio：停用→页签条整体消失，启用→恢复（已打开页签保留）。
- [x] 系统子菜单导航：折叠/展开「系统」，进入「文件」→ 文件页（右上 上传文件/上传文件夹/新建文件夹/新建文件 + 路径栏 + 书签/表格）。
- [x] 页签关闭：点「概览」页签 × 后该页签从页签条移除，侧栏菜单不受影响。
- [x] 各页无 workspace-header，内容区直接以子标签/卡片起始；整窗视觉（深色/数据库页）与 web 1Panel 结构一致。
- [x] 门禁：`npx tsc --noEmit` 无输出；`npm run build` 通过（仅 chunk-size 提示）。

## Docker/Compose 修复验收（2026-08-29，桌面实测）

- [x] 崩溃根因修复：点击 容器→编排 中的 compose 项目卡片触发 `thread 'main' has overflowed its stack`。Tauri IPC 协程在 1 MiB 默认栈线程上构造，`compose_details` 的 `tokio::join!` 8 个分支（5 个 compose_execute + 3 个 docker_label_list）各自内联 russh SSH 状态机，协程对象超过 1 MiB，栈溢出于协程构造时（早于函数体执行）。修复：8 个分支与整体命令 future 全部 `Box::pin`（`commands::docker_compose_details` 同步处理），并对 `operations::snapshot` 的 3 个 execute_probe 分支预防性同样处理。验证：点击 openresty 卡片后详情正常加载（服务表 + 脱敏 yaml 编辑器 + 清理预览），崩溃不再出现，编译通过（无 E0382 等借用错误）。
- [x] 关联缺陷修复：本服务器 `docker compose ls --format json` 输出缺少 WorkingDir 键，此前 `compose_project.workingDir` 为空字符串导致详情命令无 working_dir 而报「读取 Compose 渲染配置失败」；`parse_compose` 现从 ConfigFiles 首项的父目录回退推导。另 `docker compose ps --format json` 在本服务器输出为逐行 JSONL（而非数组），`parse_compose_services` 现同时兼容数组与 JSONL 两种形状（flash 服务器上）。
- [x] 编排详情新增「配置」标签页：读取 compose.yaml 同目录 `.env` 真实文件（readText/sudo 变体），textarea 编辑、保存走 saveText/saveTextPrivileged（携带 size/modifiedAt 防覆盖），成功后回写状态；验证读取到测试服务器真实 .env（RESTY_CONFIG_OPTIONS_MORE、PANEL_APP_PORT_HTTPS=443、WEBSITE_DIR 等）。
- [x] 容器标签页工具栏与 web 对照去重：移除顶部内联 启动/停止/重启/终止/暂停/恢复/删除 七个按钮（仅保留 创建/清理容器 + 搜索/刷新频率/compose 过滤/sudo），批量操作统一走表格底部「批量操作」下拉，与 web 1Panel 布局一致。
- [x] compose 卡片/详情状态语义与 web 对齐：容器数为 0 →「已退出」；全部运行 →「运行」；部分运行 →「运行 x/y」警告色。
- [x] 证据截图归档至 `docs/screenshots/`（2026-08-29-compose-detail.png / compose-env.png / compose-log.png / container-toolbar.png），临时 wg-* 截图与 CDP/SSH 辅助脚本已清理；临时资源创建、验证、清理均记录于本文件。

已知差距（沿用 FUNCTION_MATRIX 追踪，不在本表中标记完成）：镜像页 只读（缺 inspect/tag/push/save/load/清理构建缓存）；compose 卡片缺「备份」按钮（无后端支持）；镜像仓库与编排模版数据依赖 1Panel 服务端上传接口（当前为空占位）；监控/磁盘管理为「规划中」占位页；web 代码编辑器文件树/右键菜单尚未迁移。

## 安装包

- [x] MSI/NSIS：本轮产物已生成并校验 SHA-256。
  - `1Panel Client_0.1.0_x64_en-US.msi` — 15,958,016 bytes；`EBA4C9A76EBF507C45A782CA9EF59A56C07B12AD2B2F646B148C37270C4D84BD`
  - `1Panel Client_0.1.0_x64-setup.exe` — 11,888,291 bytes；`679168DD3911A5E33C77477155D70D2F6E5CA5AA009C35054B30D1B736D689DA`
  - `onepanel-client.exe` — 39,620,096 bytes；`7DC8F12675233E9C9205FCF8ACA36675ACE9A2387251E28389475D4F7C5A16AD`

剩余升级、安装后启动/卸载、macOS/Linux 包和 1Panel 专属深度能力，按 [`FUNCTION_MATRIX.md`](FUNCTION_MATRIX.md) 继续迭代；不把未验证项标记为完全完成。
