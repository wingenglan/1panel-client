# Server Manager 多服务器桌面管理器 —— 第一阶段 Codex / AI Coding Agent 总控开发提示词

> **用途**：将本文件完整交给 Codex、Claude Code、Cursor Agent、GitHub Copilot Coding Agent 或其他具备代码库读写能力的 AI Coding Agent，作为第一阶段项目的最高优先级产品需求、架构约束、开发规范与验收标准。
>
> **项目名称**：Server Manager（产品名：Relay）
>
> **文档目标**：不允许 Coding Agent 因需求不清晰而自行缩减范围、用 Mock 替代真实能力、跳过安全处理、擅自改变产品形态，或把“可选功能”误当作“以后再做”。除明确列入“第一阶段非目标”的内容外，本文标记为 `MUST`、`必须`、`P0`、`P1` 的功能均应实际完成。

---

# 0. 给 Coding Agent 的最高优先级指令

你是一名负责交付可运行桌面产品的资深全栈工程师、Rust 工程师、Linux 运维工程师和产品工程师。你必须直接在当前代码库中实现本需求，不要只输出设计建议、伪代码、Demo 页面或静态原型。

必须遵循以下原则：

1. **这是一个桌面端产品，不是 Web SaaS，不是服务器控制面板。**
2. **远程 Linux 服务器不得安装本产品的常驻服务、Daemon、Sidecar 或常驻采集程序。**
3. 所有远程管理能力必须主要通过 **SSH + SFTP + 远程标准命令** 完成。
4. 可以调用远程服务器上已经存在的 `docker`、`systemctl`、`nginx`、`ss`、`ps`、`journalctl` 等命令，但未经用户明确点击“安装”或“执行”不得擅自安装软件。
5. 不得要求用户在每台服务器上部署额外服务才能使用基础功能。
6. 产品必须是 **Local-first**：服务器配置、偏好、缓存等默认存储在用户本机。
7. SSH 密码、私钥口令、sudo 密码等机密信息不得明文写入 SQLite、JSON、日志、配置文件或前端 LocalStorage。
8. 不得为了方便而关闭 SSH Host Key 校验。
9. 不得直接把远程 `/var/run/docker.sock` 暴露到 TCP 网络。
10. 所有破坏性操作必须具备清晰确认、错误反馈和执行结果验证。
11. 功能必须使用真实 SSH/SFTP/系统命令工作；生产代码中不得残留 Mock 数据路径。
12. 每完成一个纵向功能切片，都必须保证项目可编译、可运行、已有测试不被破坏。
13. 如果实现细节存在多个合理方案，优先采用本文明确指定的方案；本文未指定时，选择安全、跨平台、可维护、依赖少的方案，不要停下来询问用户。
14. 所有依赖使用实现时的稳定版本，并写入 lockfile；不要使用 nightly Rust，除非某个依赖不可避免且有明确书面说明。
15. UI 不得一比一复制 Docker Desktop、1Panel、Portainer 等产品的商标、视觉资产或专有界面；可以实现同类功能和优秀交互，但要保持独立的信息架构和视觉设计。

---

# 1. 产品定位

开发一个 **多服务器、图形化 Linux 运维桌面客户端**。

用户只需在自己的 Windows/macOS/Linux 电脑安装一次本软件，然后添加多台 Linux 服务器 SSH 登录配置，即可完成：

- SSH 终端；
- 像本地文件管理器一样浏览和操作远程文件；
- 查看 CPU、内存、磁盘、负载、网络等系统信息；
- 管理 systemd 服务；
- 搜索进程、监听端口、服务，并快速释放被占用端口；
- 检测和安装常用工具；
- 图形化管理 Nginx，特别是反向代理；
- 图形化管理 Docker，核心体验达到 Docker Desktop 常用功能的大约 80% 功能覆盖度；
- 管理多套服务器登录配置并安全导入/导出。

产品体验目标：

> **Termius 的连接管理 + WinSCP/Finder/Explorer 的文件体验 + Cockpit/1Panel 的系统状态 + Docker Desktop 的 Docker 管理体验**，通过现有安全连接完成远程管理。

---

# 2. 第一阶段技术栈：固定，不要自行替换

## 2.1 桌面框架

- **Tauri 2**
- Rust stable
- Tokio async runtime

## 2.2 前端

- **React + TypeScript**
- Vite
- React Router
- TanStack Query：远程状态查询、缓存、刷新、失效
- Zustand：纯客户端 UI 状态
- shadcn/ui + Radix primitives：基础交互组件
- Tailwind CSS：样式
- Monaco Editor：文本/配置文件编辑
- xterm.js：SSH Terminal 与 Docker Exec Terminal
- ECharts：系统资源图表
- 虚拟列表库：用于大目录、大进程列表、大容器列表；可选 TanStack Virtual

## 2.3 Rust 核心

优先采用纯 Rust 或可跨平台打包方案：

- SSH：优先 `russh` 生态，要求 async、多 channel、PTY、Host Key 校验；如实现过程中确认 `russh-extra` / `russh-sftp` 能更稳定满足需求，可使用对应稳定 crate。
- SFTP：使用与 SSH 会话兼容的 SFTP 实现，不依赖用户本机必须存在 `sftp` 命令。
- SQLite：`sqlx` 或 `rusqlite`，二选一；优先 async 友好的方案。
- 密钥/凭据：优先 OS Keychain (`keyring` 类能力)；如需要应用级 Vault，使用 Tauri Stronghold。SQLite 仅保存 secret reference，不保存明文。
- 序列化：serde / serde_json
- 错误：thiserror + anyhow（边界层）
- 日志：tracing；日志必须经过 secret redaction。

## 2.4 不允许的核心架构

第一阶段禁止：

- Electron；
- 把整个 Rust 核心替换为 Node.js 后端；
- 远端常驻管理服务；
- 要求用户自己运行一个本地 Web Server 才能打开 UI；
- Redis、PostgreSQL 等额外本地服务依赖；
- 把 SSH 密码存 LocalStorage；
- 通过公网明文暴露 Docker API；
- 为了 UI 快速实现而把所有 Linux 命令直接拼接在 React 中。

---

# 3. 支持范围

## 3.1 桌面端

架构必须跨平台：

- Windows 10/11 x64 / arm64（能力允许时）；
- macOS 13+，Apple Silicon 为一等支持，Intel 不得在代码层硬编码排除；
- Linux 桌面为支持目标，至少 Ubuntu Desktop / 常见 WebKitGTK 环境能够构建。

如果 CI 成本有限，第一阶段发布包优先 Windows + macOS，但代码不得写成仅 Windows 可运行。

## 3.2 远程服务器

P0 正式支持：

- Ubuntu / Debian 系；
- Rocky Linux / AlmaLinux / RHEL 系；
- systemd；
- OpenSSH Server；
- Bash 或 POSIX Shell 基本可用。

通过 `/etc/os-release` 自动识别：

- `os_family`；
- `distribution`；
- `version`；
- `package_manager`；
- `init_system`；
- 可用命令能力。

其他发行版允许“尽力读取基础信息”，但不能伪装为完整支持。

---

# 4. 总体产品信息架构

主窗口推荐结构：

```text
┌─────────────────────────────────────────────────────────────────┐
│ 顶部：全局搜索 / 快捷连接 / 当前任务 / 通知 / 设置             │
├──────────────┬──────────────────────────────────────────────────┤
│              │ 当前 Server Workspace                            │
│ Servers      │                                                  │
│              │ Overview | Files | Terminal | Ports & Processes  │
│ 分组/标签     │ Services | Tools | Nginx | Docker | Logs         │
│ 收藏          │                                                  │
│ 最近          │                                                  │
│              │                                                  │
├──────────────┴──────────────────────────────────────────────────┤
│ 底部状态：连接状态 / 当前任务 / 传输速度 / 错误提示             │
└─────────────────────────────────────────────────────────────────┘
```

必须具备：

- 多服务器列表；
- 服务器分组；
- 标签；
- 收藏；
- 搜索；
- 在线/离线状态；
- 最近连接；
- 多工作区 Tab；
- 深色/浅色模式；
- 中文为第一语言；架构预留 i18n；
- 全局 Command Palette（例如 `Ctrl/Cmd + K`）；
- 长任务中心；
- 统一 Toast / Notification；
- 错误详情可展开复制。

---

# 5. 核心架构

## 5.1 分层

```text
React UI
  ↓ Tauri IPC
Application Services
  ↓
Domain Services
  ├─ SSH Connection Manager
  ├─ Remote Command Runner
  ├─ SFTP/File Service
  ├─ System Probe Service
  ├─ Process/Port Service
  ├─ Systemd Service Manager
  ├─ Package Manager
  ├─ Nginx Manager
  ├─ Docker Manager
  ├─ Transfer Manager
  ├─ Task Manager
  └─ Credential Store
  ↓
SSH / SFTP
  ↓
Remote Linux
```

React 层只调用 typed commands/events，不直接构造 shell 命令。

## 5.2 SSH Connection Manager

每台在线服务器维护可复用 SSH Session：

- 单 Session 支持多个 SSH Channel；
- 心跳/keepalive；
- 连接超时；
- 命令超时；
- 空闲回收；
- 网络断开检测；
- 可手动 Reconnect；
- 后台自动重连采用退避算法，但不得无限频繁重试；
- 终端、监控、Docker 日志、文件传输可以并发；
- 避免每执行一个 `free`/`df` 都重新 TCP + SSH 握手。

推荐默认值：

- connect timeout：10s；
- command default timeout：30s；
- keepalive：30s；
- selected server overview refresh：5s；
- servers list lightweight refresh：30s；
- 同一服务器普通短命令并发上限：8；
- 全局后台服务器探测并发上限：5，可配置。

## 5.3 Remote Command Runner

统一封装：

```rust
RemoteCommandRequest {
  server_id,
  command_kind,
  args,
  stdin,
  timeout,
  privilege,
  env,
  stream_mode,
}
```

统一返回：

```rust
RemoteCommandResult {
  exit_code,
  stdout,
  stderr,
  duration_ms,
}
```

要求：

- 默认 `LC_ALL=C`，确保解析稳定；
- 能用 JSON 输出的工具优先 JSON；
- shell 参数必须安全转义；
- 不允许把用户输入直接拼进裸 shell 字符串；
- sudo password 只能通过专门的 privileged runner 传递，不能出现在命令参数、日志或错误详情；
- 支持 streaming stdout/stderr；
- 支持 cancellation；
- 所有模块复用该抽象。

---

# 6. 登录配置与凭据管理（P0）

## 6.1 Server Profile

至少包含：

```text
id
name
description
host
port (default 22)
username
auth_type: password | private_key | ssh_agent
private_key_path / private_key_secret_ref
password_secret_ref
key_passphrase_secret_ref
sudo_mode: none | passwordless | password
sudo_password_secret_ref
group_id
tags
favorite
connect_timeout
keepalive
encoding (default UTF-8)
created_at
updated_at
last_connected_at
```

P1 支持：

- ProxyJump / Bastion Host；
- 自定义 SSH options；
- SSH Agent。

## 6.2 Host Key

必须实现 OpenSSH 风格 Host Key 信任流程：

首次连接：

1. 获取远端 Host Key；
2. UI 展示算法与 fingerprint；
3. 用户选择“信任并保存”后才继续；
4. 后续 key 变化必须中止并给出高风险警告；
5. 不允许默认 `StrictHostKeyChecking=no` 等价行为。

## 6.3 凭据存储

必须：

- OS Keychain / Stronghold 保存 secret；
- SQLite 只保存 reference id；
- UI 默认不回显密码；
- 日志 redact；
- Crash report 不包含 secret；
- 剪贴板复制密码如实现，应支持自动清理。

## 6.4 配置导入/导出

必须提供两种导出：

### 普通导出

默认仅导出：

- 主机；
- 端口；
- 用户名；
- 分组；
- 标签；
- 非敏感设置。

不导出 secret。

### 加密完整备份

用户显式开启“包含凭据”后：

- 要求用户输入导出密码；
- KDF：Argon2id；
- 内容使用现代 AEAD（例如 AES-256-GCM 或 XChaCha20-Poly1305）；
- 文件中保存 KDF 参数、salt、nonce、ciphertext；
- 不保存明文导出密码；
- 导入时校验格式版本；
- 导入完成后 secret 重新进入本机安全存储。

支持 JSON-based versioned schema，例如：

```json
{
  "format": "agentless-server-manager-backup",
  "version": 1,
  "encrypted": true,
  "payload": "..."
}
```

---

# 7. 终端模块（P0）

必须保留完整 SSH Terminal 交互模式。

## 7.1 技术实现

```text
xterm.js
  ↕ Tauri event / channel
Rust SSH PTY Channel
  ↕
Remote shell
```

## 7.2 必须功能

- 创建真实 PTY；
- bash/zsh 等交互 Shell；
- Terminal resize -> SSH window change；
- ANSI 色彩；
- UTF-8；
- 粘贴多行命令前可按设置弹提醒；
- 鼠标选择；
- Copy/Paste；
- `Ctrl+C`、`Ctrl+Z` 等控制字符正确透传；
- 多 Terminal Tab；
- Terminal Tab 重命名；
- 清屏；
- 搜索终端输出；
- 字体大小；
- 滚动缓冲；
- 连接断开有明确提示；
- 一键重新连接；
- 服务器切换时不得误把输入发到另一个终端。

P1：

- Split pane；
- Terminal profile；
- SFTP 当前目录联动。

默认不得记录完整终端输入日志，因为可能含 secret。

---

# 8. 文件管理模块（P0，重点）

目标：用户感受必须接近 Windows Explorer / macOS Finder，而不是“简单 SFTP 列表”。

## 8.1 协议

普通文件操作优先使用 **SFTP**：

- list；
- stat/lstat；
- read；
- write；
- mkdir；
- rename；
- remove；
- chmod；
- symlink（能力可用时）。

只有权限提升、特殊系统文件、超大日志 tail 等场景才使用远程命令补充。

## 8.2 文件浏览 UI

必须具备：

- 面包屑路径；
- Back / Forward / Up；
- 当前路径直接输入；
- 文件名、大小、类型、权限、owner、group、mtime；
- 排序；
- 搜索/过滤；
- 隐藏文件显示切换；
- List / Detail view；
- 大目录虚拟滚动；
- 目录懒加载；
- 右键菜单；
- 多选；
- keyboard navigation；
- `F2` Rename；
- `Delete` 删除；
- `Ctrl/Cmd+C/X/V`；
- 刷新；
- 新建文件；
- 新建文件夹。

## 8.3 拖拽

必须支持：

1. 本地 -> 远程：拖文件/文件夹上传；
2. 远程 -> 本地：拖出下载（平台能力受限时至少提供拖到已知本地目标或 Download）；
3. 同一远程目录/服务器内部拖动：Move；
4. `Ctrl/Option` 等修饰键可切换 Copy/Move 时，遵循平台惯例；
5. 文件夹递归传输；
6. 传输进度、速度、已传大小、ETA（ETA 无法稳定计算可不显示，但不能假数据）；
7. Pause/Resume 如果底层难以可靠支持可 P1，但 Cancel 必须 P0；
8. 同名冲突：Skip / Replace / Rename / Apply to all。

## 8.4 双击打开

规则固定：

- 文件夹：进入；
- 可识别文本文件且 <= 默认 10MB：内置 Monaco 打开；
- 超大文本：使用 Large File Viewer，不整文件载入内存；
- 图片等常见可预览格式：P1；
- 二进制/未知类型：允许“下载到安全临时目录并用系统默认应用打开”。

## 8.5 内置编辑

Monaco 必须支持：

- 常见语言高亮；
- YAML/JSON/nginx/conf/shell；
- Find/Replace；
- 行号；
- 自动换行；
- 修改未保存标记；
- `Ctrl/Cmd+S` 保存；
- Reload；
- 编码默认 UTF-8；
- 保存失败绝不能假装成功。

保存前记录远端 `mtime + size`；保存时发现远端文件被其他人修改：

- 阻止直接覆盖；
- 提示 Reload / Compare / Force overwrite。

普通文件保存建议采用：

1. 上传临时同目录文件；
2. fsync 能力允许时完成写入；
3. rename 原子替换；
4. 尽量保留原权限。

## 8.6 root 权限文件

例如 `/etc/nginx/nginx.conf`，当前 SSH 用户无写权限时：

- 不允许静默失败；
- 提供“使用 sudo 保存”；
- 先上传到用户可写远程临时文件；
- privileged runner 校验目标路径；
- 备份原文件；
- `sudo install`/安全 copy 到目标；
- 尽量保留 mode/owner；
- 删除临时文件；
- secret 不进日志。

## 8.7 删除

- 文件删除：确认；
- 非空目录：确认并明确显示递归删除；
- `/`、`/etc`、`/usr`、`/var` 等高风险系统目录本身禁止通过普通 UI 直接递归删除；
- 高风险删除要求二次确认；
- 删除后刷新并确认对象不存在。

## 8.8 传输中心

全局 Transfer Manager：

- queued；
- running；
- success；
- failed；
- cancelled；
- retry；
- error details。

关闭文件页不能导致进行中的传输消失或失控。

---

# 9. Overview / 服务器状态（P0）

连接后 3 秒左右应尽快展示首屏可用信息；慢命令不得阻塞整个 Overview。

## 9.1 基础信息

显示：

- hostname；
- OS / distribution / version；
- kernel；
- architecture；
- uptime；
- 当前时间 / timezone；
- SSH 登录用户；
- CPU model；
- logical cores；
- memory total；
- swap；
- primary IP；
- default gateway（能力可用时）；
- virtualization/container environment（可检测时）；
- package manager；
- systemd 状态。

## 9.2 实时状态

至少：

- CPU usage；
- load 1/5/15；
- RAM used / available / total；
- Swap；
- 磁盘使用率；
- 文件系统 mount；
- network rx/tx；
- top CPU processes；
- top memory processes。

使用 Linux 标准接口优先：

- `/proc/stat`；
- `/proc/meminfo`；
- `/proc/loadavg`；
- `/proc/net/dev`；
- `df -P` / `df -B1`；
- `lsblk`；
- `ip`。

CPU/网络速率必须基于两个采样点计算，不能拿累计值当瞬时使用率。

## 9.3 运行能力摘要

卡片显示：

- Docker：not installed / stopped / running + version；
- Nginx：not installed / stopped / running + version；
- failed systemd services 数量；
- listening ports 数量；
- disk warning；
- memory warning。

默认 warning：

- disk >= 85%；
- memory available <= 10%；
- load 超过 CPU cores 持续多个采样点时提示。

阈值可配置。

---

# 10. 进程 / 端口 / 服务统一管理（P0，重点）

用户最高频流程之一：**查端口 -> 看谁占用 -> 结束 -> 确认端口释放。**

## 10.1 顶部统一搜索

搜索框输入任意：

- `8080`；
- `node`；
- `nginx`；
- `mysql`；
- PID；
- service name；

同时搜索：

- Processes；
- Listening Ports；
- systemd Services。

结果按类型分组。

## 10.2 Process 数据

至少展示：

- PID；
- PPID；
- user；
- state；
- CPU%；
- MEM%；
- RSS；
- elapsed time；
- command；
- executable path（权限允许）；
- cwd（权限允许）；
- 监听端口；
- 所属 systemd unit（可推断时）。

敏感环境变量默认不显示。

## 10.3 Port 数据

优先 `ss`：

- TCP Listen；
- UDP Listen；
- local address；
- port；
- PID；
- process name；
- user（可获得时）；
- IPv4/IPv6；
- service mapping。

如果非 root 无法看到进程信息，UI 显示“权限不足”，并提供“使用 sudo 重新扫描”。

## 10.4 释放端口流程

禁止直接隐藏执行 `kill -9 $(...)`。

必须：

1. 用户搜索 `8080`；
2. 列出所有占用该端口的 socket/process；
3. 展示 PID、command、user、service；
4. 用户点击“结束进程”或“停止服务”；
5. 默认优先 `SIGTERM`；
6. 等待短时间后重新扫描；
7. 若仍存在，提供“强制结束 SIGKILL”；
8. 再次扫描；
9. UI 明确显示“8080 已释放”或仍被哪个 PID 占用。

如果进程由 systemd 自动拉起，优先提示用户停止对应 service，避免 kill 后立即重启。

## 10.5 风险保护

以下类型终止必须高风险提示：

- PID 1；
- sshd 当前主进程/当前连接相关；
- systemd；
- kernel threads；
- 用户当前管理连接可能依赖的网络组件；
- Docker daemon（从 Processes 页 kill 时）。

推荐通过专用 Service/Docker 页面操作这些服务。

---

# 11. Systemd Services（P0）

独立 Services 页，同时被全局搜索复用。

功能：

- running / stopped / failed；
- service file state；
- description；
- active since；
- main PID；
- memory（可获得时）；
- start；
- stop；
- restart；
- reload（支持时）；
- enable；
- disable；
- 查看最近 journal；
- `journalctl -u <unit>` 流式日志；
- 搜索/过滤；
- failed filter。

所有需要权限的操作走 privileged runner。

---

# 12. 常用工具检测、推荐与安装（P0）

## 12.1 工具中心

至少包含：

- Docker Engine；
- Nginx；
- Git；
- curl；
- wget；
- jq；
- unzip；
- rsync；
- tmux；
- htop；
- lsof。

UI 展示：

- Installed / Not installed；
- Version；
- Running（有 daemon 时）；
- 推荐原因；
- Install；
- Upgrade（P1）；
- Uninstall（P1，危险操作）。

## 12.2 安装策略

根据 OS Adapter：

- Debian/Ubuntu -> apt；
- RHEL/Rocky/Alma -> dnf/yum。

安装前：

1. 显示将执行的高层计划；
2. 提示需要 sudo；
3. 用户确认；
4. 执行；
5. streaming output；
6. 验证 binary/version/service；
7. UI 更新状态。

不得自动执行网上未知来源 `curl | sh`。

Docker 安装必须优先遵循 Docker 官方仓库方式，且把 distro/version 不支持错误清晰呈现。

Nginx 可优先系统官方 package repository；如果用户选择 Nginx 官方 repo，需明确区分。

---

# 13. Nginx 专属模块（P0，重点）

目标：不仅显示 nginx 在运行，还必须让用户快速看懂“当前 Nginx 到底代理了什么”。

## 13.1 Detection

检测：

- `command -v nginx`；
- `nginx -v`；
- `nginx -V`；
- systemd unit；
- master/worker；
- compile arguments；
- 主 config path；
- prefix；
- error log path；
- access log path（解析 config）；
- `nginx -T` 完整有效配置。

`nginx -T` 可能需要 sudo，必须正确处理。

## 13.2 Overview

展示：

- version；
- status；
- PID；
- uptime/active since；
- config path；
- config test status；
- listen ports；
- server blocks 数量；
- reverse proxy 数量；
- upstream groups 数量；
- SSL server blocks 数量。

提供：

- Start；
- Stop；
- Restart；
- Reload；
- Test Config。

Reload 前必须 `nginx -t`。

## 13.3 Config Parser

不要用脆弱的 regex 单独解析整个 Nginx 配置。

必须构建最小 AST / token parser，至少能可靠理解：

- `http`；
- `server`；
- `location`；
- `upstream`；
- `include`；
- `listen`；
- `server_name`；
- `proxy_pass`；
- `proxy_set_header`；
- `root`；
- `ssl_certificate`；
- `ssl_certificate_key`；
- `return`；
- comments/source positions。

对不认识的 directive：

- 保留；
- 不删除；
- 不重排；
- generic editor 可原样编辑。

解析 `nginx -T` 输出时要保存 source file + line mapping，能告诉用户某个 proxy 来自哪个 `.conf`。

## 13.4 Reverse Proxy 列表

必须从实际配置推断：

| Server Name | Listen | Location | Upstream | Target Host | Target Port | SSL | Source File |
|---|---|---|---|---|---|---|---|

支持：

- domain 搜索；
- target port 搜索；
- target host 搜索；
- source file 打开；
- 直接跳到对应行；
- 测试后端连通性（用户点击触发）。

## 13.5 便捷添加反向代理

必须提供 GUI Wizard。

字段：

```text
Name
Server Name / Domain
Listen HTTP Port (default 80)
Enable HTTPS (optional)
HTTPS Port (default 443)
Certificate path (if HTTPS)
Certificate key path (if HTTPS)
Location path (default /)
Upstream scheme: http | https
Target host
Target port
WebSocket support
Preserve Host header
X-Real-IP / X-Forwarded-For
Connect timeout
Read timeout
Client max body size (optional)
```

第一阶段不强制实现 ACME 自动签证书；如果用户选择 HTTPS，要求已有证书路径。

## 13.6 配置写入安全流程

对 GUI 新建代理，优先写入本产品管理的独立配置文件，例如：

```text
/etc/nginx/conf.d/server-manager/<safe-name>.conf
```

实际路径要根据当前 Nginx include 结构判断；如果该目录未被 include，不得假设生效，应让用户选择已 include 目录或提供明确的、安全的 include 变更流程。

任何配置修改：

1. 读取最新文件；
2. 创建 timestamped backup；
3. 写临时文件；
4. 替换；
5. `nginx -t`；
6. test 失败 -> 自动恢复 backup；
7. test 成功 -> 用户确认/按设置 reload；
8. `systemctl reload nginx` 或兼容方式；
9. 再次检查 status；
10. UI 展示结果。

绝对不允许：配置 test 失败后仍 reload。

## 13.7 高级编辑

提供：

- config tree；
- Monaco editor；
- syntax highlight；
- source files；
- diff；
- save + test；
- reload。

---

# 14. Docker 专属模块（P0/P1，第一阶段最重要模块之一）

## 14.1 产品要求

“80% 还原 Docker Desktop”必须解释为：

> **实现 Docker Desktop 对单机 Docker Engine 的主要日常 GUI 管理能力的约 80% 功能覆盖，而不是复制它的视觉设计。**

以下功能在第一阶段视为 `MUST`，不能只做一个 `docker ps` 列表。

## 14.2 Docker 连接原则

远程主机已有 Docker 时：

- 通过 SSH 执行远程 Docker CLI；
- 优先使用 `--format json`、`docker inspect` 等结构化输出；
- stream log/progress 时使用独立 SSH channel；
- 不在公网开放 Docker API；
- 不要求本机安装 Docker Desktop；
- 不要求本机 Docker CLI；
- 如果远程用户无 docker 权限，提供 sudo mode。

后续可以实现“通过 SSH tunnel 访问 Engine API”的优化，但不是第一阶段必须条件。

## 14.3 Docker Overview

显示：

- Docker Engine version；
- API version；
- OS/Arch；
- storage driver；
- cgroup version；
- running/stopped/paused containers；
- images count；
- volumes count；
- networks count；
- disk usage；
- Docker root dir；
- Docker service status。

按钮：

- Install Docker（未安装）；
- Start Docker；
- Restart Docker；
- Open Docker service logs。

## 14.4 Containers 页面（MUST）

列表字段：

- Name；
- ID short；
- Image；
- Status；
- Health；
- Created；
- Ports；
- CPU；
- Memory；
- Compose Project；
- Restart Policy。

支持：

- Search；
- status filter；
- Compose project group；
- start；
- stop；
- restart；
- pause；
- unpause；
- kill；
- delete；
- force delete（二次确认）；
- rename；
- open mapped HTTP port in browser；
- copy container id/name；
- refresh。

### Container Detail

Tab 至少：

1. Summary；
2. Logs；
3. Inspect；
4. Stats；
5. Processes；
6. Exec；
7. Files/Mounts（最低可先实现 mounts + volume jump；容器内部文件浏览列 P1）。

#### Summary

- image；
- command/entrypoint；
- env（默认对疑似 secret key/value 做 mask）；
- labels；
- ports；
- mounts；
- networks；
- restart policy；
- healthcheck；
- created/start times。

#### Logs

- real-time follow；
- stdout/stderr；
- timestamps；
- search；
- pause auto-scroll；
- clear view（不删除容器日志）；
- copy；
- download current output；
- tail lines 设置；
- reconnect。

#### Inspect

- formatted tree view；
- raw JSON；
- search；
- copy JSON path/value。

#### Stats

至少实时：

- CPU%；
- memory usage/limit；
- memory%；
- network I/O；
- block I/O；
- PIDs。

图表至少保留当前打开页面的短期 session history。

#### Processes

展示 `docker top` 等价信息并支持 copy/search。

#### Exec

- 新建 interactive shell；
- shell 自动探测 `/bin/bash` -> `/bin/sh`；
- xterm.js；
- resize；
- 多 exec session；
- 明确显示当前 container。

## 14.5 Run Container Wizard（MUST）

从镜像页面/容器页面可以创建容器。

字段至少：

- image；
- name；
- command；
- entrypoint advanced；
- port mappings；
- environment variables；
- bind mounts；
- named volumes；
- network；
- restart policy；
- working dir；
- hostname；
- auto remove；
- privileged（高风险确认）；
- capabilities（P1）；
- CPU/memory limits（P1）。

创建前给出配置摘要。

## 14.6 Images 页面（MUST）

字段：

- Repository；
- Tag；
- Image ID；
- Size；
- Created；
- In use；
- dangling。

支持：

- Pull image；
- Pull progress；
- Search/filter；
- Run；
- Remove；
- Force remove（二次确认）；
- Tag；
- Inspect；
- History；
- Prune dangling；
- Save image to remote/local tar（P1）；
- Load image tar（P1）。

Pull 示例 UI：

```text
nginx:latest
[downloading] layer1 68%
[extracting]  layer2 100%
...
Done
```

必须真实解析 streaming progress，不显示假进度。

## 14.7 Volumes 页面（MUST）

功能：

- list；
- search；
- create；
- inspect；
- show attached containers；
- delete unused/specified；
- force safeguards；
- size（能可靠获取时）；
- jump to related containers。

P1 为达到更接近 Docker Desktop 的体验，补充：

- browse volume files；
- export tar；
- import/restore；
- clone；
- empty。

实现 volume browse 时不得假设 Docker volume mountpoint 对普通 SSH 用户可读；必须通过受控 sudo 或明确错误处理。

## 14.8 Networks 页面（MUST）

- list；
- driver；
- scope；
- subnet/gateway；
- attached containers；
- inspect；
- create bridge network；
- remove；
- connect container；
- disconnect container。

保护默认系统网络：`bridge` / `host` / `none` 不允许普通删除。

## 14.9 Compose 页面（MUST）

Docker Compose v2 优先。

展示：

- project；
- status；
- config files；
- working directory；
- services；
- containers；
- ports。

操作：

- up；
- down；
- start；
- stop；
- restart；
- pull；
- build；
- logs；
- ps；
- config；
- open compose file in Monaco；
- save；
- `docker compose config` 验证；
- apply changes。

`down -v` 必须作为高风险单独操作，不得默认勾选。

## 14.10 Build（P1，但第一阶段应尽量完成）

- 选择远端 context directory；
- Dockerfile path；
- tag；
- build args；
- no-cache；
- pull；
- streaming build logs；
- cancel；
- 成功后跳转 image。

## 14.11 Events / 全局 Logs（P1）

- `docker events` stream；
- 按 container/event type filter；
- 统一 container logs viewer；
- 搜索。

## 14.12 Docker 清理

提供“Disk Usage / Cleanup”：

- `docker system df`；
- dangling images；
- stopped containers；
- unused networks；
- unused volumes；
- build cache（P1）。

清理前必须明确列出类型和潜在影响。

Volumes 永远不能隐藏在一个“一键全部清理”的默认选择里。

---

# 15. Logs 模块（P1，但基础能力应随模块实现）

统一日志体验：

- systemd journal；
- Nginx access/error；
- Docker logs；
- 用户选择的普通文本 log file。

能力：

- tail/follow；
- pause；
- search；
- regex filter（P1）；
- timestamps；
- copy；
- download；
- max buffer；
- 自动断线恢复。

不要一次把几 GB log 全部下载到前端。

---

# 16. Task / Progress 系统（P0）

任何耗时操作不得用阻塞 UI 的方式实现。

Task 类型：

- file upload/download；
- docker pull；
- docker build；
- package install；
- nginx test/reload；
- compose up/pull/build；
- recursive file delete/copy；
- server probe。

统一状态：

```text
queued
running
success
failed
cancelled
```

字段：

```text
id
type
server_id
title
progress(optional)
bytes(optional)
started_at
finished_at
error
cancel_supported
```

长任务在用户切换页面后仍应继续，并可从全局任务中心查看。

---

# 17. OS Adapter / Capability 系统（P0）

不要把 Ubuntu/Rocky 条件散落在全代码库。

定义：

```text
PlatformAdapter
├─ DebianFamilyAdapter
└─ RhelFamilyAdapter
```

抽象：

- package manager；
- service manager；
- install package；
- package query；
- firewall detection；
- command paths；
- distro-specific Docker install；
- nginx default paths only as fallback。

同时建立 `ServerCapabilities`：

```text
has_systemd
has_sudo
sudo_passwordless
has_docker
has_docker_compose_v2
has_nginx
has_ss
has_ip
has_journalctl
has_lsof
has_tar
has_gzip
...
```

UI 根据 capability disable 功能并说明原因，不要点击后才报模糊错误。

---

# 18. 推荐 Rust Service 接口

至少拆分：

```text
src-tauri/src/
  app/
  commands/
  domain/
    server/
    ssh/
    files/
    metrics/
    process/
    ports/
    services/
    packages/
    nginx/
    docker/
    credentials/
    tasks/
  infra/
    db/
    ssh/
    sftp/
    keychain/
    platform/
  security/
  errors/
```

核心 trait/struct：

```text
SshConnectionManager
RemoteCommandRunner
PrivilegedCommandRunner
SftpService
FileTransferManager
SystemProbeService
ProcessService
PortService
SystemdServiceManager
PackageManagerService
NginxService
DockerService
CredentialStore
TaskManager
ServerRepository
SettingsRepository
```

每个模块返回强类型 DTO，不让前端解析 shell 文本。

---

# 19. 前端目录建议

```text
src/
  app/
  components/
  features/
    servers/
    overview/
    files/
    terminal/
    processes/
    services/
    tools/
    nginx/
    docker/
    logs/
    settings/
  hooks/
  lib/
  stores/
  types/
  routes/
```

原则：

- feature-based；
- API 类型集中；
- Query Key 规范；
- React component 不执行 shell；
- destructive action 使用统一 ConfirmDialog；
- tables 使用统一 DataGrid 风格；
- loading/empty/error state 统一。

---

# 20. SQLite 数据模型

最低表：

## servers

```text
id TEXT PK
name TEXT
host TEXT
port INTEGER
username TEXT
auth_type TEXT
password_secret_ref TEXT NULL
private_key_path TEXT NULL
private_key_secret_ref TEXT NULL
key_passphrase_secret_ref TEXT NULL
sudo_mode TEXT
sudo_secret_ref TEXT NULL
group_id TEXT NULL
favorite INTEGER
settings_json TEXT
last_connected_at DATETIME NULL
created_at DATETIME
updated_at DATETIME
```

## server_groups

```text
id
name
sort_order
created_at
```

## tags / server_tags

标准 many-to-many。

## known_hosts

```text
server_identity
key_type
fingerprint
public_key
first_seen_at
last_seen_at
```

## recent_paths

每服务器记录最近文件路径。

## app_settings

key/value 或 typed JSON。

## audit_events

本地操作审计，不记录 secret：

```text
id
server_id
action
resource_type
resource_id
result
summary
created_at
```

例如记录：

- restart nginx；
- kill pid 123；
- delete container；
- edit nginx config；

但不记录密码、完整敏感 env、私钥内容。

数据库必须 migration-based。

---

# 21. 安全要求（不可降级）

## 21.1 Tauri

- 最小 Capability；
- 禁止 frontend 获得任意 shell 执行权限；
- CSP；
- 不加载不可信远程 HTML 到高权限 WebView；
- 外链通过系统浏览器打开；
- IPC commands 白名单。

## 21.2 SSH

- Host Key 验证；
- 不记录密码；
- key passphrase 安全存储；
- 超时；
- 输入 escape；
- 远程命令参数编码；
- privilege 明确。

## 21.3 Docker

- 不暴露 docker.sock TCP；
- privileged container 创建高风险确认；
- bind mount `/`、`/var/run/docker.sock` 等高风险路径警告；
- 删除 volume 高风险确认；
- env 中命中 `PASSWORD|SECRET|TOKEN|KEY|AUTH` 等字段默认 mask，仅用户点击后临时显示。

## 21.4 Nginx

- 保存前 backup；
- `nginx -t`；
- fail rollback；
- reload 验证；
- 不把 certificate private key 内容无故拉到前端。

## 21.5 文件

高风险系统路径删除保护。

## 21.6 日志

统一 redact：

- passwords；
- key passphrase；
- sudo password；
- private key；
- token；
- Authorization header。

---

# 22. 错误模型

定义结构化 `AppError`：

```text
code
category
message
details
server_id
recoverable
suggested_action
source(optional, dev only)
```

至少分类：

- NETWORK_TIMEOUT；
- SSH_CONNECT_FAILED；
- SSH_AUTH_FAILED；
- HOST_KEY_UNKNOWN；
- HOST_KEY_CHANGED；
- SUDO_REQUIRED；
- SUDO_AUTH_FAILED；
- PERMISSION_DENIED；
- COMMAND_NOT_FOUND；
- COMMAND_TIMEOUT；
- REMOTE_COMMAND_FAILED；
- SFTP_FAILED；
- FILE_CONFLICT；
- DOCKER_NOT_INSTALLED；
- DOCKER_DAEMON_STOPPED；
- DOCKER_PERMISSION_DENIED；
- NGINX_NOT_INSTALLED；
- NGINX_CONFIG_INVALID；
- PARSE_FAILED；
- CANCELLED。

UI 必须把“认证失败”和“网络超时”区分开。

禁止永远转圈无反馈。

---

# 23. 性能目标

这些是工程目标，不要求在所有公网条件下绝对保证，但必须据此设计。

- 已保存服务器点击连接后，网络正常时尽快 <3s 得到 workspace；
- Overview 首批卡片增量加载，不等待全部探测完成；
- 单目录 1,000 项使用虚拟列表仍流畅；
- 10,000 项不得一次渲染 10,000 DOM rows；
- 文件传输 streaming，不整文件读进内存；
- Docker logs streaming；
- Terminal 输入响应不经过数据库；
- 50 台服务器列表后台探测使用并发限制；
- 未打开服务器不进行 5 秒高频全指标轮询；
- selected server metrics 5 秒，后台列表摘要 30 秒；
- 断开连接后停止相关 poller/stream。

---

# 24. UX 细节要求

## 24.1 一致的危险操作语义

红色 destructive actions：

- Kill process；
- Force kill；
- Delete file recursively；
- Delete container；
- Remove image in use；
- Remove volume；
- Compose down -v；
- Nginx destructive config overwrite。

确认框必须显示具体对象名，不使用泛泛的“确定删除吗”。

## 24.2 快捷操作

Server list 右键：

- Connect；
- Open Terminal；
- Open Files；
- Copy Host；
- Edit Profile；
- Disconnect；
- Export Profile；
- Delete Profile。

## 24.3 Global Search / Command Palette

支持：

```text
> prod-01
> open files prod-01
> terminal prod-01
> port 8080
> nginx
> docker containers
```

第一阶段不必做自然语言理解，使用 command/filter 即可。

---

# 25. 第一阶段必须自行补充的基础功能

用户可能没有逐条说出，但完整产品必须有：

- 设置页；
- About/version；
- 自动更新能力预留；
- 应用日志；
- Debug diagnostics export（必须脱敏）；
- Connection test；
- Server profile edit/delete；
- Duplicate profile；
- 主题；
- 语言架构；
- 快捷键；
- 网络错误处理；
- sudo capability detection；
- Server offline 状态；
- Retry；
- Loading/empty/error states；
- Context menus；
- 分页/虚拟化；
- Copy values；
- 人类可读 bytes；
- 时间/时区格式统一；
- 软件启动恢复上次 workspace（可设置）；
- 崩溃后不自动重放危险任务。

---

# 26. 第一阶段明确非目标

Coding Agent 不要偷偷扩张这些需求导致核心功能做不完：

- Kubernetes 全功能管理；
- MySQL/PostgreSQL SQL GUI；
- Redis GUI；
- 远程桌面 RDP/VNC；
- SaaS 账号体系；
- 团队 RBAC；
- 云厂商 API；
- CMDB；
- Prometheus 长期监控；
- 30 天历史指标；
- 强制服务器端常驻管理服务；
- 自动 ACME/Let's Encrypt 证书签发；
- 应用商店；
- 完整 Ansible 替代品。

这些未来可做，但不允许以它们为理由牺牲 P0。

---

# 27. 开发顺序：必须按纵向切片，不要一次铺满空页面

## Milestone 0：项目骨架

完成：

- Tauri 2 + React + TS；
- routing；
- design tokens；
- Rust IPC；
- SQLite migrations；
- keychain/stronghold abstraction；
- tracing/redaction；
- AppError；
- test setup；
- CI build。

验收：应用可启动，设置/空服务器页面正常。

## Milestone 1：Server + SSH

完成：

- add/edit profile；
- password/key auth；
- host key trust；
- credential store；
- connect/disconnect；
- connection pool；
- terminal。

验收：真实 Ubuntu/Rocky 服务器可登录并稳定终端交互。

## Milestone 2：Files

完成全部 P0 文件管理：

- SFTP browsing；
- upload/download；
- drag/drop；
- editor；
- sudo save；
- conflict；
- transfer manager。

验收：日常编辑 `/etc/nginx/*.conf`、上传项目文件可使用。

## Milestone 3：Overview + Processes + Ports + Services

重点验收：

- 查询 `8080`；
- 找到监听进程；
- SIGTERM；
- 再扫描；
- 确认端口释放。

## Milestone 4：Tools + Nginx

完成：

- tool detection/install；
- nginx detection；
- reverse proxy parser；
- reverse proxy wizard；
- config backup/test/rollback/reload。

## Milestone 5：Docker Core

完成：

- overview；
- containers；
- logs；
- inspect；
- stats；
- exec；
- images；
- pull；
- run container。

## Milestone 6：Docker Extended

完成：

- volumes；
- networks；
- compose；
- cleanup；
- P1 builds/events 尽可能完成。

## Milestone 7：Import/Export + Polish

- backup/import；
- keyboard shortcuts；
- error polish；
- performance；
- diagnostics；
- packaging；
- docs；
- full acceptance tests。

---

# 28. 测试策略

## 28.1 Rust Unit Tests

必须覆盖解析器：

- `/etc/os-release`；
- `/proc/meminfo`；
- `/proc/stat` CPU delta；
- `df`；
- `ss`；
- `ps`；
- systemd；
- Docker JSON outputs；
- Nginx parser；
- command escaping；
- secret redaction；
- export encryption。

使用 fixtures，不依赖开发者本机状态。

## 28.2 Frontend Tests

- Vitest；
- React Testing Library；
- important stores/hooks；
- destructive confirm；
- file conflict UI；
- Docker filters；
- port search results；
- Nginx wizard validation。

## 28.3 Integration Test Targets

提供自动化或文档化测试环境：

- Ubuntu SSH target；
- Rocky/Alma SSH target；
- password auth；
- key auth；
- sudo passwordless；
- sudo password；
- Docker installed target；
- Nginx reverse proxy target。

可以用本地 VM/容器作为测试 target，但**产品自身仍不能要求生产服务器安装常驻管理程序**。

## 28.4 手工 Acceptance Checklist

发布前必须人工验证本文第 30 节。

---

# 29. Coding Agent 工作规则

1. 开始实现前先阅读本文件全部内容。
2. 创建 `docs/ARCHITECTURE.md`，同步真实架构。
3. 创建 `docs/DECISIONS.md`，记录重要不可逆技术决定。
4. 创建 `docs/SECURITY.md`，列出 secret 流向、Host Key、sudo 模型。
5. 创建 `docs/REMOTE_COMPATIBILITY.md`，列支持发行版和命令 fallback。
6. 创建 `docs/ACCEPTANCE.md`，复制并维护验收状态。
7. 不要创建数十个空页面然后声称完成 UI。
8. 每个里程碑至少完成一个真实端到端路径。
9. Rust `cargo fmt`、`cargo clippy`、tests 必须通过。
10. 前端 lint、typecheck、tests 必须通过。
11. 所有 `TODO` 必须可追踪；P0 不得以 TODO 留空。
12. 对第三方命令输出解析必须保存测试 fixture。
13. 不要假设命令一定存在；先 capability detect。
14. 不要吞错误。
15. 不要用 `unwrap()` 处理正常可失败业务路径。
16. 不要在前端使用 `dangerouslySetInnerHTML` 显示远程内容。
17. 不要执行远程返回的任意字符串作为本地代码。
18. 不要未经确认升级/安装/删除远程软件。
19. 不要通过“改成 root 登录”绕过 sudo 设计。
20. 如果某个 P1 无法在第一阶段稳定完成，可留下明确 feature flag 和 issue，但 P0 必须完成。

---

# 30. 最终硬性验收场景

以下场景全部通过，第一阶段才算完成。

## A. 多服务器登录

- 添加 Server A：password auth；
- 添加 Server B：private key auth；
- 重启应用；
- 两台配置仍存在；
- 凭据未明文出现在 SQLite；
- 一键重新连接成功；
- Host Key 改变时能阻止连接。

## B. Terminal

- 打开 Server A；
- `top` / `htop` 正常显示；
- resize 正常；
- `Ctrl+C` 正常；
- 多 terminal tabs 正常；
- 断网后 UI 明确显示 disconnected。

## C. Files

- 打开 `/etc/nginx`；
- 双击 `.conf`；
- Monaco 展示；
- 修改保存；
- 如果需要 sudo，正确提权；
- 并发修改冲突能阻止覆盖；
- 从桌面拖文件上传；
- 拖整个目录上传；
- 下载大文件不占用等量内存；
- rename/delete/new folder 正常。

## D. Server Overview

- CPU、RAM、disk、load、network 真实更新；
- Docker/Nginx 状态正确；
- OS 与 kernel 正确；
- 断开后停止刷新。

## E. Port Release

测试进程监听 `8080`：

- 搜索 `8080`；
- 找到 PID、command；
- 点击结束；
- 默认 SIGTERM；
- 重新扫描；
- UI 显示 8080 已释放；
- systemd 服务占用时提示优先 stop service。

## F. Tool Install

在没有 Nginx 的支持发行版：

- Tools 显示 Not installed；
- 点击 Install；
- 显示计划；
- sudo；
- streaming output；
- 成功后 version + running status 更新。

Docker 同理。

## G. Nginx Reverse Proxy

准备：

```nginx
server {
    listen 80;
    server_name demo.example.com;
    location / {
        proxy_pass http://127.0.0.1:3000;
    }
}
```

必须：

- UI 自动识别 `demo.example.com`；
- 显示 target `127.0.0.1:3000`；
- 显示 source file；
- 可打开源配置；
- Wizard 新增 `api.example.com -> 127.0.0.1:8080`；
- 保存前备份；
- `nginx -t`；
- reload；
- 新代理出现在列表；
- 故意写错误配置时能够 test fail 并 rollback，不中断原 Nginx。

## H. Docker Containers

- 查看 running/stopped；
- start/stop/restart；
- 查看 logs 并 follow；
- inspect JSON；
- stats 实时；
- exec shell；
- 查看 ports/mounts/networks；
- 删除容器有确认。

## I. Docker Images

- Pull `nginx:latest`；
- 展示真实 progress；
- pull 完成后列表出现；
- 点击 Run；
- 设置 `8088:80`；
- 创建容器；
- 打开 `http://<server>:8088` 或给出正确 URL；
- stop/delete；
- image remove。

## J. Docker Compose

- 找到已有 Compose project；
- 展示 services/containers；
- logs；
- restart；
- 打开 compose yaml；
- 修改；
- `docker compose config` 验证；
- apply。

## K. Backup/Import

- 普通导出不含 secrets；
- 完整导出要求密码；
- 新环境导入；
- profile 恢复；
- secret 恢复到安全存储；
- 错密码无法解密。

---

# 31. Definition of Done

每个 feature 的 Done 必须同时满足：

- UI 完成；
- Rust backend 完成；
- 真实远程环境运行；
- loading/error/empty state；
- 权限不足场景；
- tests；
- docs；
- 无 secret 泄露；
- 无明显 panic；
- 断网处理；
- destructive confirm；
- operation result verification。

不能用“API 已写但没接 UI”或“UI 已有但数据是假”判定 Done。

---

# 32. 交付物

最终仓库必须包含：

```text
README.md
LICENSE (根据项目决定)
docs/
  ARCHITECTURE.md
  SECURITY.md
  DECISIONS.md
  REMOTE_COMPATIBILITY.md
  ACCEPTANCE.md
src/
src-tauri/
tests/
fixtures/
.github/workflows/ 或等价 CI
```

README 至少包含：

- 产品说明；
- 截图位置预留；
- 本地开发依赖；
- 安装步骤；
- `pnpm install`；
- `pnpm tauri dev`；
- Rust requirements；
- Build；
- Test；
- Packaging；
- 安全说明；
- 支持的远端系统。

---

# 33. 推荐的实现约束与设计取舍

以下决定视为默认架构，不要重复争论：

1. **Tauri + React + Rust**，不是 Electron。
2. **SSH/SFTP 直连管理**，不在每台服务器安装管理面板。
3. 系统监控是即时/短期状态，不做 Prometheus 长期时序数据库。
4. 文件管理 SFTP-first，不用 `ls` 模拟文件系统。
5. Docker 第一阶段 CLI-over-SSH，不暴露 daemon TCP API。
6. Nginx 配置必须解析真实有效配置，优先 `nginx -T`。
7. Nginx GUI 写入尽量创建独立 managed conf，避免大规模重写未知用户配置。
8. privileged operation 集中封装，不允许每模块自己偷偷 `sudo`。
9. secret 使用系统安全存储，SQLite 仅保存 reference。
10. 命令解析环境 `LC_ALL=C`。
11. 大文件/日志 streaming。
12. 所有后台刷新按 server capability 和页面可见性启停。
13. 端口释放必须 inspect -> terminate -> verify。
14. Docker 删除 volume、Nginx 配置覆盖、系统目录递归删除属于高风险。
15. Docker Desktop “80%”是功能覆盖目标，不是 UI 抄袭指标。

---

# 34. 官方技术参考（实现时优先参考这些文档）

以下链接用于明确技术行为；实现时应优先查官方文档而不是博客复制命令：

## Tauri

- Tauri 2 Architecture: https://v2.tauri.app/concept/architecture/
- Tauri Security: https://v2.tauri.app/security/
- Tauri Capabilities: https://v2.tauri.app/security/capabilities/
- Tauri Stronghold: https://v2.tauri.app/plugin/stronghold/

## Terminal

- xterm.js: https://xtermjs.org/
- xterm.js docs: https://xtermjs.org/docs/

## Docker

- Docker Desktop overview: https://docs.docker.com/desktop/
- Docker Desktop dashboard/features: https://docs.docker.com/desktop/use-desktop/
- Containers view: https://docs.docker.com/desktop/use-desktop/container/
- Images view: https://docs.docker.com/desktop/use-desktop/images/
- Volumes view: https://docs.docker.com/desktop/use-desktop/volumes/
- Builds view: https://docs.docker.com/desktop/use-desktop/builds/
- Docker Engine API: https://docs.docker.com/reference/api/engine/
- Runtime metrics: https://docs.docker.com/engine/containers/runmetrics/

## Nginx

- Proxy module: https://nginx.org/en/docs/http/ngx_http_proxy_module.html
- Upstream module: https://nginx.org/en/docs/http/ngx_http_upstream_module.html
- Server names: https://nginx.org/en/docs/http/server_names.html
- Load balancing: https://nginx.org/en/docs/http/load_balancing.html

## Rust SSH ecosystem reference

- russh: https://docs.rs/russh
- ssh2/libssh2 alternative reference: https://docs.rs/ssh2
- keyring: https://docs.rs/keyring

---

# 35. 最后执行指令

从现在开始：

1. 扫描当前 repository；如果为空，初始化上述技术栈。
2. 创建项目文档与 architecture skeleton。
3. 按 Milestone 0 -> 7 顺序实现。
4. 不要一次生成全部代码后再调试；每个纵向切片持续编译和测试。
5. 遇到远程系统差异时，优先增加 capability/adaptor，而不是散落 if/else。
6. 遇到权限问题，走统一 privileged runner。
7. 遇到 destructive action，统一 confirm + verify。
8. 遇到 secret，统一安全存储 + redaction。
9. 遇到 streaming 数据，使用 Tauri event/channel 或等价流式 IPC，不做高频 request polling 模拟 terminal/log stream。
10. 每完成一个 milestone，更新 `docs/ACCEPTANCE.md`，真实标记已通过/未通过。
11. P0 未通过前，不得以“后续完善”结束任务。
12. 最终必须给出：
    - 已完成 milestone；
    - build/test 命令与结果；
    - 已知限制；
    - 未完成的 P1；
    - 可运行安装包/构建产物路径（如果当前环境允许打包）。

**现在开始实现，不要把本文重新总结给用户，也不要反问已经在本文定义过的问题。**
