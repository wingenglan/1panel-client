# 1Panel 功能覆盖矩阵

基准：1Panel 社区版 v2.2.5，在线面板与上游提交 `14728f889e810d5a0c19eaa4a923110921ebe7b7`。

状态：`[x]` 已有真实前后端实现；`[~]` 已覆盖核心子集；`[ ]` 尚未实现。没有真实远程环境证据的能力仍需验收，不能仅凭编译通过视为完整完成。

| 1Panel 模块 | 状态 | 1Panel Client 当前能力 | 主要缺口 |
|---|---:|---|---|
| 多节点 | [x] | 本地服务器档案、分组、节点切换、密码/私钥/SSH Agent SSH、通过已认证跳板建立多级 direct-tcpip ProxyJump；表单展示链路并拒绝循环/孤儿/超长配置 | 更多拓扑批量诊断 |
| 概览 | [x] | CPU、内存、磁盘、网络、系统信息、运行时摘要、短期历史、按真实节点能力生成推荐卡、按服务器隔离的本地备忘录 | 更丰富的运营指标与推荐策略 |
| 应用商店 | [~] | 官方 Compose 应用目录、安装/启动/停止/重启/卸载与卸载后恢复、日志、当前来源最新版本升级回滚、升级前 Compose 哈希/行数差异预览、官方分支提交号同步显示、环境变量键摘要与合并保存、容器健康检查、基于 Docker project labels 的 Compose 清理候选预览；目录/详情本地 TTL 缓存、离线回退、最多 8 个有序静态镜像节点故障转移和缓存清理；可生成完整静态镜像目录（metadata、版本 Compose、env、catalog.json），并以 HMAC-SHA256 签名、通过操作系统密钥链验签 | 完整 1Panel 应用编排细节 |
| AI | [~] | OpenAI-compatible 供应商、OS 密钥链、真实 Chat Completions 请求与 SSE 流式输出（可取消）、真实 `/models` 能力探测；只读服务器概览/网站/Docker/安全四类有界快照 function-calling 智能体与步数上限；本地会话按供应商持久化、恢复、删除和清理；可选 MCP stdio 或远程 HTTP/SSE JSON-RPC 工具发现/调用、Bearer 令牌密钥链保存、会话 ID 和只读策略 | 更多工具编排与远程 MCP 生态兼容性验收 |
| 网站 | [~] | 静态站点/反向代理、OpenResty 配置测试/reload、证书元数据、certbot/acme.sh HTTP-01 签发/续期、certbot Cloudflare DNS-01、acme.sh Cloudflare/阿里云/DNSPod/腾讯云/AWS Route 53 DNS-01、PHP-FPM 探测/安装计划与静态站点 FastCGI 绑定、同域受控站点证书自动绑定 | 容器内 PHP socket 映射和证书批量策略 |
| 数据库 | [~] | MySQL/MariaDB/PostgreSQL/Redis 探测；数据库/用户/权限与真实数据库级权限矩阵、备份恢复、服务生命周期、apt/dnf/apk/pacman 引擎安装计划与可取消流式执行；安装后按命令、版本和 systemd/OpenRC 状态重新验证；Redis 键扫描、类型/TTL/内存摘要、删除、确认后的 FLUSHDB，字符串值读写/可选 TTL、hash/list/set/zset 受控逐项编辑，远端 DUMP/RESTORE 复杂值快照导入导出，Redis ACL 用户创建/删除/授权/撤销/重置密码与脱敏规则读取，当前会话 Redis 默认用户/ACL 用户认证，以及支持 AUTH/AUTH2、保留类型/TTL 的 MIGRATE 跨实例/跨版本逐键迁移 | 更多权限模型差异和安装后诊断 |
| 容器 | [~] | 容器、镜像、卷、网络、Compose、日志、Inspect、Stats、Exec、Pull/Run、远端 Docker Build（构建参数、流式输出、取消）、最近 Docker daemon Events 有界摘要 | 完整 Docker Desktop 级细节 |
| 系统 | [~] | 文件、监控、进程、端口、systemd 服务、日志；真实 lsblk/findmnt/df/fstab 磁盘与挂载快照，带路径/文件系统/选项校验的挂载、卸载和 fstab 备份+dry-run 回滚；防火墙与 SSH 配置；概览只读诊断 root/sudo、systemctl/docker/nginx 命令和 1Panel/OpenResty/Docker 目录访问状态 | 更多发行版专属系统设置、分区格式化和 RAID/LVM 编排 |
| 终端 | [x] | 多标签真实 PTY、搜索、快捷指令、断线状态 | Split pane |
| 计划任务 | [~] | 真实 crontab Shell/URL/目录/网站/应用/数据库/日志备份类型创建，URL 多地址固定 curl 请求；网站按实时受控域名解析并处理 OpenResty 容器根目录到宿主机挂载路径，应用按固定已安装 Compose 目录解析；备份使用固定 tar/mysqldump/pg_dump 模板、UTC 时间戳文件名、同目录临时文件和原子替换，支持按份数/天数清理同前缀归档；执行/受控删除、systemd timer 只读展示、立即执行结果脱敏后本地保留最近 200 条历史；支持版本化 JSON 导出/确认导入（受支持类型保留 marker，未知类型安全降级为 Shell）和本地历史清理；计划任务可选本机目录、WebDAV、S3 兼容对象存储和独立 SFTP 账号，账号公共配置写入 SQLite、secret 写入系统密钥链，手动运行或客户端在线补传时下载归档并原子上传；支持通用 JSON、Slack、Discord、钉钉（可选 HMAC）和企业微信报告通知；同一客户端且凭据有效的导出账号引用会恢复，失效或跨客户端引用会安全剔除并报告 | 更多对象存储协议与跨客户端账号映射 |
| 工具箱 | [~] | Nginx/Docker/Git/curl 等探测与受控安装 | 1Panel 全部工具页 |
| 高级功能 | [~] | WAF/ModSecurity 能力探测、宿主机及 OpenResty/NGINX 容器内固定规则文件读取与受控 SecRule/SecAction 增删（备份、配置测试、reload 失败回滚）、3 个后端固定的敏感文件/危险方法/扫描器标识策略模板（重复规则 ID 防护）、内置 OWASP CRS 4.25.1 LTS/4.28.0 固定 URL+SHA-256+签名指纹来源（宿主机/容器安装、更新、移除，配置测试/reload 失败回滚）、主机固定日志及匹配 Docker 容器日志拒绝事件摘要、warning/error/critical 阈值过滤、本地有界历史与出现次数、每小时趋势、30 秒轮询的应用内新增提示、系统密钥链保存的通用 JSON/Slack/Discord/钉钉/企业微信 webhook 外部通知（钉钉可选 HMAC 签名）、远程 HTTP 探活、状态码/延迟结果；探活任务持久化、到期调度、失败记录和历史列表 | 更多第三方 CRS/签名规则源与策略编排 |
| 日志审计 | [~] | systemd/Nginx/Docker/Compose 日志、本地操作审计 | 1Panel 登录/操作日志字段完全对齐 |
| 面板设置 | [~] | 主题、语言结构、备份、诊断、安全存储 | 官方更新通道、通知、完整国际化 |

## 当前阶段成功标准

1. `1panel-client` 可独立安装依赖、构建和启动，不修改原 `server-manager`。
2. 已实现模块全部读取真实 SSH/SFTP/远程命令结果，不使用生产 Mock。
3. UI 使用 1Panel v2 的浅色蓝白设计 token、180/75px 可折叠侧栏、42px 菜单和白色卡片。
4. 侧栏底部提供客户端特有的多服务器节点切换器。
5. 未实现模块明确禁用并说明原因，不制造“已复刻完成”的假象。
