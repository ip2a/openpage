# OpenPage v2 Wait / Navigation Plan

## Goal
把 OpenPage CLI 的等待能力收敛成 agent 日常浏览可依赖的稳定语义：等待页面加载、等待导航、等待 DOM/元素状态，行为尽量吸收 DrissionPage 的 wait 设计，但不引入兼容层和多套协议。

## Assumptions
- 当前 CLI 的执行真相源是 TCP daemon；新增/修复 wait 命令应优先落在 `serve.rs` op + `oneshot.rs` wrapper。
- DP 文档可作为语义参考，不要求命令名完全兼容。
- 本轮只改 wait/navigation 相关文件，避免触碰已有大量未提交变更。

## Phases
- [x] Phase 1: 对齐 DP wait 板块与 OpenPage 现状
- [x] Phase 2: 修复 `wait-for-navigation` 不能区分旧页面 ready 的问题
- [x] Phase 3: 修复 `wait-disabled-or-deleted` 在元素已删除/查找失败时直接报错的问题
- [x] Phase 4: 编译、单测、真实 CLI smoke
- [ ] Phase 5: 输出最终状态和 v2 后续优先级

## Success Criteria
1. `wait-for-ready` 只表达“当前页面 ready”。
2. `wait-for-navigation` 必须观察到 URL 变化或加载状态变化，不能在旧页面已经 ready 时立即成功。
3. `wait-disabled-or-deleted` 符合 DP 语义：元素不可用或不存在/已删除都可成功。
4. `cargo check`、CLI parser tests、serve tests 通过。
5. 本地 HTML smoke 能证明点击后导航、延迟元素、删除元素等待链路可用。

## Status
Ready for review - 当前 wait/navigation 第三轮语义收紧已完成：
- `wait-for-navigation` 对主页面优先使用 `Page` 级导航事件快照（`started_seq` / `settled_seq`）
- frame 场景继续保留 URL + `readyState` 轮询兜底
- 新增显式 `navigation_token` 机制：点击 / submit / press-key 等交互动作会返回 token，`wait-for-navigation --token ...` 和 `wait navigation --token ...` 可显式消费该动作的导航票据，覆盖“导航已经发生，wait 稍后才调用”的场景
- `frame switch` 的 daemon 内部状态已改为持久化真实 `frame_id`，`frame list` 的 `active` 标记已与之对齐；`press` CLI wrapper 现在也会透传底层 `navigation_token`

已通过 `cargo check`、`cli::oneshot::tests`、`cli::serve::tests`，并通过本地静态页 smoke 验证：
- `wait-for-ready`
- `wait-for-navigation`
- `wait-for-elements-loaded`
- `wait-disabled-or-deleted`
- `wait-deleted`
- generic `wait loaded` / `wait ready` / `wait navigation`
- `wait-for-navigation` 不再只依赖 daemon 侧 URL baseline；主页面走事件化导航跟踪，same-document 导航也会推进 `started_seq` / `settled_seq`
- `wait-for-navigation --token nav-*` 可在隐式 baseline 已失效后仍准确等待对应导航

## Verification
- `cargo check -q`
- `cargo test -q cli::oneshot::tests`
- `cargo test -q cli::serve::tests`
- Headless CLI smoke on local HTML via `cargo run -q --bin openpage -- ...`:
  - start at `index.html`
  - click `#go`
  - `wait-for-navigation` reached `next.html`
  - `wait-for-elements-loaded "#late"` succeeded
  - `wait-disabled-or-deleted "#toggle"` succeeded after disable
  - `wait-deleted "#gone"` succeeded after removal
- Same-document smoke via `cargo run -q --bin openpage -- ...`:
  - click `#hashgo` changed `location.hash` to `#done`
  - plain `wait-for-navigation` succeeded
  - final URL resolved `hash.html#done`
- Token smoke via `cargo run -q --bin openpage -- ...`:
  - click `#go` returned `navigation_token`
  - after `sleep 1` and a plain `url` query, plain `wait-for-navigation --timeout 400` timed out
  - `wait-for-navigation --token <navigation_token>` still succeeded and resolved `next.html`
- History smoke via `cargo run -q --bin openpage -- ...`:
  - after `goto page2.html`, `history go 1/2` returned `navigation_token`
  - `wait-for-navigation --token ...` succeeded for history traversal
- Form submit smoke via `cargo run -q --bin openpage -- ...`:
  - `submit '#f'` returned `navigation_token`
  - `wait-for-navigation --token ...` reached `submitted.html?q=openpage`
- Frame state smoke via `cargo run -q --bin openpage -- ...`:
  - `frame switch 1` persisted active frame state
  - `frame list` now reports that frame as `active: true`
  - `press '#innerlink' Enter` now returns `navigation_token`
- Negative stale-baseline smoke via `cargo run -q --bin openpage -- ...`:
  - click a non-navigating button on `index.html`
  - subsequent `wait-for-navigation --timeout 400` timed out as expected instead of false-positive success

## DP Coverage
- DrissionPage 文档确认有完整 wait 板块：页面级 `load_start` / `doc_loaded` / `eles_loaded` / `upload_paths_inputted` / `title_change` / `url_change` / `alert_closed`，元素级 `displayed` / `hidden` / `deleted` / `has_rect` / `covered` / `not_covered` / `enabled` / `disabled` / `clickable` / `disabled_or_deleted` / `stop_moving`，以及浏览器级 `new_tab` / `download_begin` / `downloads_done`。
- OpenPage CLI 现在已覆盖上述主干能力，并额外提供 agent 侧友好的 `wait-for-ready`、`wait-for-navigation`、`wait-for-function`、`wait-for-text`。通用 `wait` 入口也已不再把 `load/doc-loaded/ready/navigation` 混成同一个 op。

## Remaining Gaps
- `wait-for-navigation` 现在已有显式 `navigation_token`，但默认的“无 token 隐式等待”仍然更适合 agent 在交互后尽快调用。如果后续想把默认路径也做得更强，需要再设计“最近一次可等待导航”的失效规则，而不是无限记住最后一次导航。
- iframe 场景还暴露了一个更底层的问题：在 `frame switch` 后，`click '#innerlink'` / `press '#innerlink' Enter` 能返回成功和 `navigation_token`，但本地 smoke 中并没有触发 iframe 内实际导航，`frame.url` 仍停留在原页面。这更像 frame 内交互链路问题，不是 wait/navigation 判定问题。
- `history go` 的索引语义目前会把初始 `about:blank` 也算进历史项；对 agent 来说这有点反直觉，后续可以考虑是否需要更明确的 CLI 文档或更友好的选择器。
- 还没有把 DP 的“sleep/random wait”抽成明确 CLI 能力；目前只能通过 shell 或上层 agent 控制。
- 还没有把 wait 集成矩阵补到更完整，例如 back/forward 命令路径、`pushState` / `replaceState`、多 iframe、跨 frame 切换后的交互恢复。
