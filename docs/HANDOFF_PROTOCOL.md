# 持续交接维护规范

这份文档用于保证项目在不同电脑、窗口、AI Agent 或模型之间多次转接后仍可继续。任何接手 Agent 在用户要求“暂停”“收尾”“交接”“换环境”时都必须执行本规范，而不是只口头总结。

## 1. 稳定入口与文档职责

保持以下结构：

| 文档 | 职责 | 每次交接是否更新 |
|---|---|---|
| `HANDOFF.md` | 唯一入口、当前阶段、凭据/环境、最近证据、第一条任务 | 必须 |
| `docs/CURRENT_STATE.md` | 代码事实、模块地图、完成/未完成/技术债 | 必须 |
| `docs/NEXT_STEPS.md` | 按依赖排序的下一步和完成条件 | 必须 |
| `docs/ACCEPTANCE.md` | 证据驱动的里程碑/场景状态 | 必须 |
| `docs/HANDOFF_PROTOCOL.md` | 本规范 | 规则变化时 |
| `docs/ARCHITECTURE.md` | 当前真实架构 | 架构变化时 |
| `docs/DECISIONS.md` | 不可逆/重要 ADR | 有新决定时 |
| `docs/SECURITY.md` | secret/host key/sudo/security boundary | 安全流变化时 |
| `docs/REMOTE_COMPATIBILITY.md` | distro/capability/fallback | 兼容实现变化时 |

不得为每次交接创建 `HANDOFF-v2-final-new.md` 之类分叉入口。历史由 Git 保存；始终原地更新 `HANDOFF.md`。

## 2. 收尾前冻结工作范围

1. 停止开始新 feature。
2. 等待或安全终止正在运行的 dev server、Cargo/Vite/build/automation。
3. 对被中断的命令明确记录：成功、失败、被暂停、是否有可用产物。不能把无输出的中断构建写成成功。
4. `git status --short`，区分本次改动与用户已有改动；禁止 reset 用户工作。
5. 检查大目录、临时文件、测试 secret、数据库、known_hosts、日志、installer 是否意外 untracked/staged。

推荐检查：

```bash
git status --short
git diff --check
git diff --stat
git diff --cached --stat
git ls-files | grep -E '(target/|node_modules/|\.env|\.db$|known-hosts)'
```

Windows PowerShell 没有 grep 时用 `Select-String`。

## 3. 记录事实，不记录愿望

每个能力使用以下状态语言：

- **已实现且已验证**：有自动化和/或真实环境证据，可在 `ACCEPTANCE.md` 标 `[x]`。
- **已实现，待真实验证**：代码/compile/tests 已有，但真实 DoD 未闭环，标 `[~]`。
- **部分实现**：明确列出已有与缺口，标 `[~]`。
- **未实现**：标 `[ ]`，不能写“基本完成”。
- **阻塞**：写清阻塞条件、已尝试内容和解除方法，不等同于未完成。

每次更新 `CURRENT_STATE.md` 至少包含：

- 当前模块/文件路径；
- 已完成 UI、backend、error/loading/empty、tests；
- 未验证的权限/断网/取消/危险路径；
- 已知 bug、技术债和兼容风险；
- 最后 build/test 命令与精确结果；
- 实际安装包路径/哈希，若没有则明确“无”。

## 4. 维护测试环境记录

`HANDOFF.md` 可以按用户授权记录测试机凭据，但必须：

- 标记用途与授权范围；
- secret 不进入源码、fixture、shell history、日志或 CI；
- 记录 Host Key 算法/fingerprint，禁止通过关闭校验来省事；
- 写清远端执行过哪些变更、创建了什么测试资源、是否清理；
- 若凭据轮换，更新入口并确认旧值不再散落在其他文档；
- 生产凭据默认不得写入仓库，除非用户像本项目测试机一样明确授权。

## 5. 运行交接质量门

按改动风险运行检查。正式阶段交接至少运行：

```bash
pnpm lint
pnpm typecheck
pnpm test --run
pnpm build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

若环境不允许某项：

- 不要跳过不报；
- 在 `HANDOFF.md` 和 `CURRENT_STATE.md` 写明未运行项、原因、上一次通过证据；
- 将其列为下一位 Agent 的第一项。

涉及 UI/remote/package 的结果分别记录：

- desktop app 是否实际启动；
- 使用了哪个 server/distro/auth/sudo mode；
- 是否执行变更、变更对象、验证与 cleanup；
- bundle/installer 是否真实存在。

## 6. 更新下一步，保证可立即执行

`NEXT_STEPS.md` 第一项必须是下一位 Agent 能直接执行的具体任务，包含：

- 前置条件；
- 涉及模块；
- 操作顺序；
- 安全边界；
- 测试/验收完成条件。

删除已经完成的短期步骤或将其移入 `CURRENT_STATE.md`；不要让下一位 Agent从过期 checklist 猜状态。仍需遵守主需求的 Milestone 顺序。

## 7. Commit 与 push

只有用户已授权时才 push。流程：

```bash
git diff --check
git status --short
git add <明确范围>
git diff --cached --stat
git commit -m "docs: update project handoff"  # 按实际改动命名
git push
git status --short
git log --oneline --decorate -3
```

必须确认：

- 当前 branch 是预期 branch；
- remote URL 正确；
- push 返回成功，远程追踪分支已设置；
- 最终 worktree clean，或明确列出保留的用户改动；
- 将最终 commit id 写入 `HANDOFF.md`。如果先写文档再产生 commit，可在最终回复中给出 commit id；下一次交接再刷新入口里的“最近 commit”。

不得 force push、重写共享历史或删除 tag，除非用户明确要求并确认范围。

## 8. 给用户的最终交接回执

最终消息必须简洁包含：

- 入口文档路径；
- commit/branch/remote/push 结果；
- 最后测试结果；
- 未完成或未运行的重要项；
- 下一位 Agent 的第一条任务。

不要只说“已完成交接”。让用户可以在另一台电脑上用 `git clone`/`git pull` 后，把 `HANDOFF.md` 直接交给下一位 Agent。

## 交接文档自身的 Definition of Done

- 新 Agent 只看 `HANDOFF.md` 就知道还需读什么、怎么装环境、怎么验证、先做什么。
- `CURRENT_STATE.md` 不把实现等同于验收。
- `NEXT_STEPS.md` 与主需求里程碑一致，没有跳过 P0。
- 凭据使用范围、Host Key 与真实远端变更记录清楚。
- 所有测试/构建陈述可由命令输出或产物证明。
- 文档链接有效，Git 状态与 push 状态清楚。
