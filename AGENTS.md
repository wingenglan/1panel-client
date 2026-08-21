# 1Panel Client 开发规范

## 开始工作前

1. 完整阅读 `HANDOFF.md`、`README.md`、`docs/FUNCTION_MATRIX.md` 和 `docs/SECURITY.md`。
2. 运行 `git status --short`；保护用户已有改动，不重置或覆盖无关内容。
3. 先复现 `HANDOFF.md` 中的质量门，再继续开发。

## 产品边界

- 这是多服务器桌面客户端，不在远端安装常驻管理服务。
- 1Panel v2.2.5 是功能与视觉基准；多服务器节点切换是客户端的核心差异。
- 未完成模块必须明确禁用或标记，不得使用静态 Mock 冒充真实能力。
- 不复制 1Panel 官方商标图形；保留 GPL-3.0 与 `NOTICE` 中的来源说明。

## 安全与实现

- React 不得构造裸远程 shell；SSH/SFTP/命令、权限与危险操作必须留在 Rust 边界。
- secret 不得进入源码、SQLite、LocalStorage、日志、命令参数或测试快照。
- Host Key 校验不得关闭；首次信任与 key changed 必须走产品流程。
- 远端安装、删除、服务变更与信号操作必须由用户在 UI 中明确触发，并验证结果。
- 只修改需求直接涉及的文件，不顺手重构无关代码。
- 新增或修改函数/方法时必须添加或同步用途注释；参数、返回值或副作用不直观时一并说明。

## 验证

前端变更至少运行：

```text
pnpm lint
pnpm typecheck
pnpm test --run
pnpm build
```

Rust 变更还需运行：

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

没有真实环境证据的远程功能只能标记为待验收，不能标记完成。
