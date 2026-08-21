# Security Model

## Secret 流向

```text
密码输入框（React 内存）
  -> Tauri IPC 参数（一次性）
  -> Rust SecretString / zeroize
  -> OS Keychain
  -> SQLite 仅记录随机 secret_ref
```

密码不会进入 LocalStorage、SQLite、日志或诊断包；1Panel Client 不会把终端历史写入磁盘，但用户在远程 shell 中主动提交的内容会在当前 shell 内存历史中短暂显示，因此不应把密码、私钥或 token 作为命令/输入提交。保存完成后 React 表单清空。Keychain 写入失败时整个保存操作失败，不做明文降级。

选择 SSH Agent / Pageant 时，Rust 只读取当前桌面 Agent 的公钥身份并委托签名；私钥不会复制到本地数据库、Keychain、远端命令参数或日志。Windows 优先使用 `SSH_AUTH_SOCK` OpenSSH named pipe，随后回退 Pageant；Unix 使用 `SSH_AUTH_SOCK`。

## Host Key

1. SSH 握手获取远端公钥。
2. 未知密钥返回算法和 SHA-256 fingerprint，拒绝继续认证。
3. 用户明确选择“信任并连接”后保存完整 OpenSSH public key。
4. 后续密钥不一致返回 `HOST_KEY_CHANGED` 并中止；此场景不能在普通连接弹窗中覆盖旧密钥。

## sudo

普通 runner 不接受 sudo 密码。privileged runner 使用 `sudo -S -p '' --` 并通过 stdin 发送密码，argv、错误与 tracing 字段均只含脱敏摘要。所有高权限写入先校验目标路径，并在操作后验证结果。

## 日志与诊断

日志过滤器对 password/passphrase/token/authorization/private key 等模式做统一 redaction。诊断导出仅含版本、能力、结构化错误和脱敏环境，不含命令 stdin、终端输入或远程敏感文件内容。

## 快捷指令、远程日志与任务记录

- 快捷指令只插入终端，不因匹配或 Tab 操作自动执行；带参数的命令必须先完成变量填写。
- 快捷指令、任务记录和审计事件不得持久化密码、私钥、sudo 凭据、完整终端输入或命令输出；界面提示用户使用变量代替敏感值。当前 shell 的短期命令历史也不应输入敏感值。
- 远程日志只按 tail/follow 流式读取，前端使用固定行数/字节上限，不默认把日志内容写入 SQLite。
- 日志来源使用固定命令和参数校验：Nginx 仅允许固定 access/error 路径，systemd/Docker/Compose 目标拒绝 shell 元字符和非法名称；不开放任意远程文件日志。
- 应用重启后 queued/running 任务只标记为 `interrupted`，任务中心只提供返回原模块的手动入口，不自动重放安装、删除、部署、移动等危险任务。
- `task_records` 的 `retry_payload_json` 仅允许非敏感重试元数据；不得把密码、私钥、sudo 凭据、完整终端输入、命令输出或 Docker 环境变量写入该字段。

## WebView

CSP 默认拒绝远程脚本与页面。当前主窗口 capability 只开启 Tauri core；前端没有系统浏览器、任意本地 shell、文件系统或数据库权限。需要外链时将按 URL scope 单独增加 opener 权限。
