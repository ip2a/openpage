## Task Plan: CLI local dogfooding and optimization roadmap (2026-06-06)

### Goal
Use the locally installed `openpage` CLI as a real user would, collect evidence about what feels awkward, and identify the highest-value optimization projects rather than isolated cosmetic tweaks.

### Phases
- [x] Phase 1: Install local CLI and establish repeatable dogfooding loop
- [x] Phase 2: Audit startup / diagnostics / recovery friction
- [x] Phase 3: Audit first-run / happy-path / composition workflows
- [x] Phase 4: Rank optimization projects with evidence
- [ ] Phase 5: Land the next high-value UX fix and re-verify

### Success Criteria
1. Installed-binary evidence exists for first-run, normal-run, and failure/recovery workflows.
2. The top optimization projects are written down with concrete command evidence, not just intuition.
3. At least one additional high-value friction point is either fixed or explicitly deferred with a clear reason.

### Decisions Made
- Keep using the installed binary at `/tmp/openpage-cli-eval/bin/openpage` for evaluation, not `cargo run`.
- Prioritize shell-contract consistency and recovery semantics over adding more surface area.
- Treat `batch` / multi-command composition as the next likely workflow area after startup diagnostics.
- Treat default fixed browser debug port semantics as a first-class CLI UX problem, not just an environmental flake.
- Use `browser status` as the first place to close broken-session detection, while recognizing that `browser list` / `doctor` still need the same semantic upgrade.
- Use paired fresh-home experiments to decide whether launch policy or downstream diagnostics is the bigger lever.

### Errors Encountered
- The repository task-tracking files already contain older completed plans, so this objective needs its own section instead of reusing previous status blindly.
- A real dogfooding session showed that a daemon can stay alive and even look healthy from sidecars while page operations fail with `send failed because receiver is gone`.
- `browser list` and `doctor --quick` still classify that broken session as healthy because their inventory path only checks sidecars + TCP readiness today.
- A later real retest found an even noisier class: a second session can become `daemon_transient` / `os error 35`, while `browser list` and `doctor --quick` still read it as healthy and direct queries may hang.

### Current Status
Phase 5 in progress - the latest installed-binary experiments refine the top project further: not "default auto_port=true", but "default dynamic debug-port allocation for daemon-backed sessions without sacrificing persistent per-session profiles". The most credible implementation direction so far is the equivalent of `--port 0`, plus source-aware config handling for debugger address/port.

### Recovery-path follow-up (2026-06-06, later pass)

#### Additional evidence collected
- The asynchronous navigation follow-up path itself is valid:
  - `goto` returns a `navigation_token`
  - `wait-for-navigation` and `wait-for-ready` eventually succeed
  - so the fundamental issue is the busy in-flight window, not the eventual wait contract
- Busy-session recovery remains weak in real local usage:
  - `doctor --quick --fix` reports the busy session but leaves `fixed=[]`
  - `browser logs --tail 20` can return an empty log even when the fix text points users there
- New high-signal contract bug:
  - `browser start --replace` is documented in help and exposed in args
  - but `start_browser(...)` never reads `args.replace`
  - daemon `webpage.create` also has no replace path; existing targets simply return `existing=true`
  - real local run during a busy session showed `browser start --replace` waiting ~21s and then returning `already_running=true` on the same pid/port

#### What this changes in the roadmap
- The optimization backlog is now better grouped as:
  1. **Busy-session control plane and error semantics**
  2. **Recovery contract truthfulness**
  3. **Batch readability**
- "Recovery contract truthfulness" is now stronger than a docs polish item because it includes real behavioral mismatches:
  - `--replace` promised but not implemented
  - `doctor --fix` unable to act on the busy class it reports
  - forced stop can still orphan Chrome children

#### Next best implementation targets
- [ ] Busy-session project:
  - centralize a busy/unresponsive error contract for `rpc_webpage(...)` callers
  - stop surfacing generic `daemon_transient` for ordinary commands during in-flight navigation
  - keep `status` / `list` / `doctor` / normal commands aligned
- [ ] Recovery project:
  - either implement true `browser start --replace` behavior or remove the public contract until it exists
  - extend cleanup so forced stop can terminate the browser child as well as the daemon pid
  - decide whether `doctor --quick --fix` should recover busy sessions, and under what safety rules
- [ ] Batch project:
  - include command index and original argv/command text in each NDJSON line

#### Scope estimate from the current codebase
- Busy/error project scope is now better quantified:
  - `rust/src/cli/oneshot.rs` has **202** `rpc_webpage(...)` call sites
  - they converge through the shared chain:
    - `rpc_webpage(...)`
    - `rpc_request_existing(...)`
    - `send_request_existing(...)`
  - this confirms that busy-session semantics are a central project, not many small command fixes
- Recovery-contract pollution is also broader than a one-line fix:
  - `--replace` is present in args/help
  - recovery fix text in `connection.rs` and `protocol.rs` recommends it
  - skill/reference docs also teach it
  - so either implementation or rollback must be coordinated across code + docs + tests

#### Best next investigation after this pass
- [ ] Trace whether `send_request_existing(...)` can classify busy/unresponsive sessions before collapsing to generic `daemon_transient`
- [ ] Decide whether `--replace` should be:
  - implemented as a true stop/recreate path
  - or removed from public guidance until that path exists
- [ ] Check whether forced cleanup can reuse existing `browser_pid` state rather than inventing a second process-tracking store

#### Latest feasibility read
- Busy project:
  - current code makes a central fix plausible because `send_request_existing(...)` and `session_target_state(...)` already live together in `connection.rs`
  - likely no need for per-command surface edits across the 202 `rpc_webpage(...)` call sites
- Forced cleanup project:
  - browser child pid already exists in runtime state (`BrowserState` / `Browser::browser_pid()` / `WebPage::browser_pid()`)
  - the missing piece is persistence/externalization, not discovery
- Replace project:
  - almost certainly medium scope
  - not just request plumbing, because the daemon currently short-circuits on `existing=true`

#### Current project definitions
- [ ] Project 1: Busy-session control plane and error semantics
  - scope:
    - long navigation monopolizing control plane
    - generic `daemon_transient` for ordinary commands
    - inconsistent busy story across `status` / `list` / `doctor` / normal commands
  - key evidence:
    - real local slow-server reproductions
    - 202 `rpc_webpage(...)` call sites share the same request path
  - likely verification:
    - busy session emits one coherent structured state across all major command classes
- [ ] Project 2: Recovery-contract truthfulness
  - scope:
    - `--replace`
    - forced stop browser-child cleanup
    - empty-log recovery dead ends
    - deciding whether busy recovery should become an explicit `doctor --fix` capability
  - key evidence:
    - `--replace` is taught in help/docs/fix text but ignored in runtime
    - forced stop can return success while Chrome still lives
  - likely verification:
    - all publicly suggested recovery actions either work in real local runs or are no longer suggested
- [ ] Project 3: Batch readability and composition UX
  - scope:
    - add human-readable command correlation to NDJSON output
  - key evidence:
    - current `run_batch(...)` emits only raw per-command JSON lines with no index/argv echo
  - likely verification:
    - mixed-result batch runs are understandable from stdout alone

#### Latest distinction to preserve
- `doctor --quick --fix` is currently narrow **by documented design**.
- `browser start --replace` is currently a **broken public contract**.
- Those should stay separate in prioritization and implementation planning.

#### Phase-oriented read
- Project 1 can likely be split into:
  - Phase 1 semantic remapping in `connection.rs` / `protocol.rs`
  - Phase 2 daemon control-plane availability work in `serve.rs`
- Project 2 can likely be split into:
  - Phase 1 contract cleanup for `--replace`
  - Phase 2 persisted browser-child cleanup foundation

#### Current best implementation guesses
- Busy project:
  - a central remap after retry exhaustion looks plausible because:
    - `send_request_existing(...)`
    - `session_target_state(...)`
    - structured error helpers
    already exist in nearby code
  - but that only fixes the truthfulness layer, not the serial daemon bottleneck itself
- Replace project:
  - a oneshot-level stop-then-start implementation appears plausible without daemon protocol changes
  - but it is not a full trustable recovery story until browser-child cleanup is also handled

#### Latest semantic clarification
- `--replace` still needs a product decision even if implementation is straightforward:
  - stop+start same session **preserves localStorage/profile state**
  - so the minimal implementation would mean "restart process/page for this session name"
  - not "fresh state for this session name"
- That implies the recovery project is partly semantic, not only mechanical.

#### Latest protocol constraint
- Busy Phase 1 is not only a `connection.rs` change.
- To preserve a richer busy/unresponsive error through the existing local/daemon round-trip, `protocol.rs` also likely needs new reconstruction logic for:
  - `state="incomplete"`
  - `reasons=["daemon_unresponsive"]`
- Otherwise a new client-side remap would risk collapsing back into generic `browser_operation` or `daemon_transient` at the shell layer.

#### Current recommendations
- Busy Phase 1:
  - recommended first implementation:
    - keep public `error.kind` conservative
    - enrich structured `session/state/reasons/fix`
    - do not introduce a new public busy-specific kind yet
- Replace Phase 1:
  - recommended first implementation:
    - if implemented now, make `--replace` mean:
      - restart this named session runtime
      - preserve the session profile
    - then rewrite help/docs/fix text away from implying fresh-state reset
  - fallback:
    - remove `--replace` from public guidance until implementation exists

#### New follow-up item
- [ ] Clean up the session-state docs to match current runtime truth that named sessions now default to persistent profile dirs under `OPENPAGE_HOME/profiles/<session>`

#### Recommended first implementation slices
- [ ] Slice A: Busy truthfulness only
  - target files:
    - `rust/src/cli/connection.rs`
    - `rust/src/cli/protocol.rs`
    - possibly `rust/src/cli/oneshot.rs` tests
  - goal:
    - ordinary commands on busy sessions return structured session state instead of bare `daemon_transient`
  - first tests to add:
    - retry exhaustion + `SessionTargetState::Unresponsive` remap in `connection.rs`
    - structured busy-state round-trip in `protocol.rs`
- [ ] Slice B: Replace contract truthfulness only
  - target files:
    - `rust/src/cli/oneshot.rs`
    - `rust/src/cli/args.rs`
    - `README.md`
    - `skills/openpage-test/references/session-management.md`
    - `skills/openpage-test/references/cli-smoke.md`
  - goal:
    - make `--replace` either real or no longer publicly promised
  - first tests to add:
    - active-session `--replace` path changes runtime behavior instead of returning plain `already_running=true`
    - help/doc wording no longer implies fresh-state reset unless that behavior exists

#### First-pass implementation proposals
- [ ] Proposal A: Busy Slice A
  - implement in:
    - `send_request_existing(...)`
    - nearby retry/remap helper in `connection.rs`
    - structured round-trip support in `protocol.rs`
  - keep first public shape conservative:
    - `kind="browser_operation"`
    - `state="incomplete"`
    - `reasons=["daemon_unresponsive"]`
  - avoid new public error kind in the first pass
- [ ] Proposal B: Replace Slice B
  - implement in:
    - `start_browser(args)` in `oneshot.rs`
  - first behavior:
    - if `args.replace`, call quiet stop on the named session first
    - then run the existing start path
  - keep current persistent profile mapping
  - rewrite public wording so `--replace` means runtime restart, not state reset

#### Decision artifact
- Added:
  - `cli_optimization_projects.md`
  - `cli_optimization_checklists.md`
  - `cli_optimization_issue_cards.md`
- Purpose:
  - keep a short, decision-ready optimization shortlist separate from the long historical notes
  - make the current ranking and first implementation slices easier to reuse for issue/PR planning
  - keep exact edit points and first-test additions separate from the higher-level shortlist

#### Latest concrete progress
- Busy Slice A is no longer only a paper proposal.
- A narrow feasibility spike is now in the worktree:
  - `connection.rs` remap for exhausted transient existing-session failures
  - `protocol.rs` canonical busy-session reconstruction
- Verified by focused tests plus `cargo check`.
- Remaining gap before calling the slice proven in user terms:
  - re-dogfood the installed binary against the slow-server busy-session repro and compare command payloads before/after

## Task Plan: Unified config.toml module refactor (2026-06-01)

### Goal
Replace implicit ini-based runtime defaults with a single stable `config.toml` system, with strict precedence:
`CLI > ENV > workspace config > user config > built-in defaults`.

### Phases
- [x] Phase 1: Design + boundaries
- [x] Phase 2: Implement config loader (`config.toml`) and precedence
- [x] Phase 3: Wire CLI runtime (`serve`/`oneshot`/`doctor`) to unified config
- [x] Phase 4: Remove ini fallback from active CLI path
- [x] Phase 5: Tests + docs + verification

### Success Criteria
1. No active CLI runtime path depends on `rust/configs.ini` or `dp_configs.ini`.
2. Browser executable can be set and shown from:
   - `~/.openpage/config.toml` (user)
   - `<workspace>/.openpage/config.toml` (workspace)
   - `OPENPAGE_BROWSER_PATH` (env override)
3. Effective precedence is test-covered.
4. `openpage doctor --quick` reports effective browser config source without mentioning ini defaults.

### Errors Encountered
- `cargo test` currently fails due a pre-existing unrelated test compile issue in `rust/src/download.rs` (`Arc::clone` against a `Mutex` field in test code). This refactor was verified via `cargo check` and runtime smoke commands.

### Current Status
Completed - unified CLI config now resolves from TOML chain only in active runtime paths.

# Task Plan: OpenPage CLI 唯一 TCP 协议迁移与竞品设计借鉴

## Goal
把 OpenPage 当前 CLI 收敛到 **唯一稳定的 TCP daemon 协议路径**，删除或停用旧的废弃通信方式；保留 OpenPage 自己的元素定位、交互逻辑和 CDP 封装；同时尽量吸收 `agent-browser` 中对 OpenPage 友好的 **非-CDP 设计**（daemon 基础设施、sidecar 元数据、重试、优雅退出、AI 优先快照/输出等）。

## Phases
- [x] Phase 1: 审计当前 OpenPage CLI / daemon / protocol / session 现状
- [x] Phase 2: 建立长期任务跟踪文件与迁移清单
- [x] Phase 3: 统一协议设计，确定唯一 TCP daemon 路径与待删除旧路径
- [x] Phase 4: 引入 daemon 基础设施（connection、sidecar、重试、优雅退出）
- [x] Phase 5: 将 CLI 命令逐步收敛到唯一协议路径
- [ ] Phase 6: 引入可借鉴的非-CDP 设计（AI 快照/输出等）
- [ ] Phase 7: 清理文档、删除废弃入口、完成验证与 git 整理

## Key Questions
1. 当前本地代码里，真正同时存在的通信方式有哪些？
2. 哪些入口是“主路径”，哪些已经是废弃或临时路径？
3. 如果统一到 TCP，现有 `oneshot.rs`、`serve.rs`、`protocol.rs` 分别要承担什么职责？
4. 哪些 `agent-browser` 设计可以直接借，而不会污染 OpenPage 现有 CDP/元素交互实现？

## Decisions Made
- 以 **TCP daemon** 作为唯一协议方向，不再把 stdio 当成长期主路径。
- 不引入 `agent-browser` 的动作执行层、CDP 封装、元素定位实现。
- 优先借鉴外围基础设施：`connection.rs`、daemon sidecar、请求重试、优雅退出、AI 友好的输出/快照形态。
- 在迁移完成前，先做“现状审计 + 文件化追踪”，避免一边实现一边失去全局状态。
- 第一批代码先补 daemon 基础设施，不先碰 OpenPage 自己的 CDP/元素/交互内部逻辑。

## Evidence Collected
- `rust/src/cli/args.rs` 的 `ServeArgs` 已移除 `stdio`
- `rust/src/cli/serve.rs` 已只保留 TCP daemon 路径
- `README.md` 与 `skills/openpage-test/*` 已改为 TCP daemon 表述
- 历史 audit 起点里 `rust/src/cli/oneshot.rs` 曾大量命令直接走 `load_session()` / `open_page()` / `Browser::connect()`；当前这些旧直连执行路径已经从活跃 CLI 面移除
- `rust/src/cli/protocol.rs` 已经具备较清晰的 NDJSON request/response 结构

## Current Audit Summary
- **活跃公开协议入口已收敛到 TCP daemon**：stdio 已从代码和活跃文档中移除。
- **TCP 已成为 CLI 的唯一执行真相源**：`rust/src/cli/oneshot.rs` 中此前剩余的 `drag-in`、通用 `wait`、`click-to-*`、`tab *`、`frame *` 已全部迁到 daemon。
- **现有协议结构可保留**：`protocol.rs` 的 `Request/Response` 边界清晰，不需要照搬竞品的 `action` 风格。
- **最缺的是 connection 层**：当前缺少 daemon 发现、sidecar、自动拉起、版本校验、请求重试。
- **第一批基础设施已接入**：新增 `rust/src/cli/connection.rs`，并让 TCP `serve` 写入 `.port/.pid/.version` sidecar。
- **第一批命令已迁移**：`browser start/stop/status`、`goto`、`url`、`title`、`html`、`snapshot`、`screenshot`、`click`、`fill`、`focus`、`clear`、`submit`、`check`、`uncheck`、`text`、`attr`、`is-*`、`find/find_all/count`、`wait-visible/hidden/enabled/disabled/deleted/clickable` 已走 daemon 路径。
- **本轮又迁移一批 page/element/page-state 命令**：`scroll`、`back`、`forward`、`reload`、`stop-loading`、`js`、`download`、`intercept start/stop/status`、`alert accept/dismiss/text`、`hover`、`press`、`select`、`upload`、`drag`、`drag-to`、`drag-to-point`、`active-element`、`wait-for-url`、`wait-for-title`、`wait-for-function`、`wait-for-text`、`pdf`、`storage get/set`、`cookies get/set/delete/clear` 已走 daemon 路径。
- **connection 层已补强**：`ensure_daemon()` 现在会校验 `.version`，在版本不匹配时杀掉旧 daemon 并重启；daemon 启动 stderr 也会落到 `OPENPAGE_HOME/daemon/<session>.log`，启动失败时直接回传日志路径或内容。
- **AI-first ref 链路已打通**：`snapshot -> @e1 -> click` 已通过 daemon 实测通过。
- **旧直连辅助函数已移除**：`do_start_browser()`、`open_page()`、`load_session()`、`save_session()`、`Browser::connect()` 这一批旧 `oneshot` 执行辅助路径已从 `rust/src/cli/oneshot.rs` 清掉；本轮连最后那点旧 `session_file()` 清理残留也一起删掉了。
- **daemon 内部已接管 tab/frame 上下文**：`rust/src/cli/serve.rs` 新增 `ServeWebPage`，维护 active tab / active frame；CLI 不再依赖旧 session JSON 存储 target/frame 状态。
- **剩余主任务转向非-CDP 借鉴项**：下一步不再是协议收口，而是继续补 `agent-browser` 风格的外围设计，如 output 治理、daemon inventory、AI 友好输出等。
- **output 治理已真正接线到 CLI 输出出口**：`rust/src/cli/protocol.rs` 里的 `format_output_json()` 现在已由 `rust/src/cli/oneshot.rs::print_json()` 与 `rust/src/cli/mod.rs` 顶层错误打印共同使用；`OPENPAGE_CONTENT_BOUNDARIES` / `OPENPAGE_MAX_OUTPUT_CHARS` 已通过真实 CLI smoke 验证生效。
- **batch 批处理已完成第一版接入**：OpenPage 现在支持 `batch` 子命令，按顺序执行多条 CLI 指令；支持参数模式、stdin JSON 模式和 `--bail`，且不引入任何竞品 CDP / element / action 内核。
- **doctor 最小版已接入**：OpenPage 现在支持 `doctor` / `doctor --quick`，会用只读方式检查环境、daemon sidecars 和浏览器启动；浏览器 launch smoke 复用的是你自己的 `LaunchOptions` / `Browser::launch`。
- **daemon inventory 已真正接上 doctor**：`rust/src/cli/connection.rs` 里的 `daemon_inventory()` 现在不再是半成品；`rust/src/cli/doctor.rs` 已消费它，并区分 healthy session、incomplete sidecars、cleaned stale sidecars。
- **本机现状已重新核实**：本机 `~/.openpage/daemon` 的 healthy session 数量是运行时态；2026-05-30 当前这轮最新复核里 `browser list` 返回 8 个 healthy sessions、0 incomplete、0 cleaned。当前唯一明确失败点仍是 `browser.executable`，而在这份 dirty worktree 里失败对象是 `rust/configs.ini` 配置的 `browser_path=/tmp/dp-browser`。
- **doctor 的本机修复提示已补强**：当当前配置里的 browser executable 在 PATH 或绝对路径上不可解析时，`doctor` 现在会额外探测本机常见浏览器落点；当前机器会明确提示 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 可直接作为 `OPENPAGE_BROWSER_PATH` override 或写回配置。
- **inventory 行为已通过合成 sidecar 验证**：用临时 `OPENPAGE_HOME` 成功验证了两类行为：
  - dead + invalid sidecars 会被 `doctor --quick` 标记为 `daemon.cleaned.*`
  - alive + missing version sidecar 会被 `doctor --quick` 标记为 `daemon.incomplete.*`
- **活跃用户面已开始统一到唯一 TCP 心智**：README 和 repo-local `skills/openpage-test/*` 已删除 one-shot / 旧 `page get` 类表述，改为 raw TCP daemon 与 named-session CLI 这两个共享同一 TCP 执行路径的用户面。
- **repo-local smoke 已重新跑通**：当前机器虽然 `chrome` 不在 PATH，但 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 可用；repo-local smoke scripts 已自动探测该路径，并成功跑通：
  - named-session CLI smoke
  - raw TCP daemon smoke
  - 两张 Baidu 截图都已生成，且至少一张已视觉确认不是白屏
- **daemon inventory 已继续暴露到直接用户面**：`browser list` 不再只返回 healthy sessions，而是直接返回：
  - `sessions`
  - `incomplete`
  - `cleaned`
  这样用户不必先跑 `doctor` 才能看见 sidecar 异常状态。
- **AI-first snapshot contract 已增强**：`snapshot` 现在不再只返回交互元素数组，还会额外返回：
  - `text`
  - `refs`
  - `origin`
  - `title`（可用时）
  - `interactive_count`
  且 `text` 会自动走现有 `OPENPAGE_CONTENT_BOUNDARIES` / `OPENPAGE_MAX_OUTPUT_CHARS` 输出治理链路。
- **历史迁移文档已降级标注**：`rust_progress_report.md` 和 `协议迁移审计-v1.md` 顶部都已加“历史文档说明”，避免后续会话把旧的 `serve --stdio` / one-shot 事实误当成当前真相。
- **高风险历史协议文档已进一步强降级**：`rust_progress_report.md` 和 `协议迁移审计-v1.md` 现在都已改成 `[ARCHIVED]` 标题，并补了“当前覆盖事实（2026-05-30）”块，明确说明这些旧命令不可再作为当前仓库执行手册。
- **竞品借鉴文档已落盘**：根目录 `竞品文档-考虑借鉴的部分v1.md` 现在明确列出了三类内容：
  - OpenPage 当前本地现状
  - 可直接 copy 的非-CDP 借鉴点
  - 明确禁止借用的竞品内核范围
- **`protocol.rs` 的 CSPRNG boundary nonce 已落地**：当前 `rust/src/cli/protocol.rs` 不再使用弱 `pid+timestamp` 方案，而是改为 `getrandom` 生成进程内稳定 nonce，并已通过 `cargo check` 重新验证。
- **`protocol.rs` 的 trust boundary 又向竞品靠了一步**：当前 boundary 不再只有 `nonce + keys`；如果结果里存在 `origin`，`_boundary.origin` 和 wrapped page-content marker 都会带上该 origin。这个改动已经通过单测和真实 `snapshot` CLI smoke 验证。
- **`doctor.rs` 的 `Check.fix` 已扩展覆盖面**：当前结构化 `fix` 已不只在 `browser.executable`，还覆盖到了 environment / daemon inventory / browser launch 相关检查；同时 launch smoke 的临时目录清理已收成 Drop guard，开始向竞品 `LaunchGuard` 思路靠拢。
- **`doctor.rs` 的 daemon warning 修复建议已补上**：当前 `daemon.session.*` 这类 warning 不再只有状态描述；如果 version mismatch 或 session not ready，doctor 现在会返回明确的 stop/status/log 检查建议。
- **`doctor.rs` 的 launch lifecycle 已继续收紧**：当前 launch smoke 不再只靠显式 close；已补 `BrowserLaunchGuard` 做 best-effort browser close，并继续保留 Drop 清理临时目录，进一步向竞品 `LaunchGuard` 模式靠近。
- **`doctor.rs` 现在也开始审计旧协议本地残留物**：当前会额外检查 `OPENPAGE_HOME/sessions/*.json` 这类 legacy session JSON 文件；在本机上已经实测发现 `~/.openpage/sessions` 下仍有 4 个旧文件，并会以 `env.legacy_sessions` warning + fix 的形式暴露出来。
- **`doctor.rs` 的 summary 已继续结构化**：当前 summary 不再只有 `pass/warn/fail`，还直接返回 `info/fixable/total`，便于 agent 和脚本快速判断当前还有多少纯提示项、多少可修复项。
- **根目录文档误导面已再做一轮定点审计**：当前没有发现新的“活跃 OpenPage CLI 执行手册级误导面”；剩余旧协议措辞主要还在归档历史报告、比较研究和跟踪文件里。
- **活跃用户文档面已跟进当前真相**：README、`skills/openpage-test/references/cli-smoke.md`、`trust-boundaries.md`、`snapshot-refs.md` 已同步写入 legacy session JSON warning 与 origin-aware boundary 行为，避免代码和 skill docs 继续漂移。
- **origin-aware boundary 已从 snapshot 扩到更多读操作**：当前 `webpage.html`、`page.run_js`、`page.selected_text`、`element.text/html/attr` 这类结果也会在可用时携带 `origin`，所以 `_boundary.origin` 与 wrapped marker 的 origin 现在不再只服务于 snapshot。
- **`connection.rs` 的唯一 daemon 约束又收紧了一步**：`ensure_daemon()` 现在不再只处理“ready 但 version mismatch”的旧进程；如果发现同 session 旧进程仍存活但 TCP 端口迟迟不可用，会先给启动中的 daemon 一个短暂 ready 宽限期，宽限后仍不可用就杀掉旧进程再拉起新的 daemon，避免同 session 留下孤儿进程或并存 daemon。
- **旧 session JSON 残留已彻底从活跃 CLI 面移除**：`rust/src/cli/oneshot.rs` 里的 `session_file()` / 本地 `openpage_home()` 以及 `browser stop` 时顺手删旧 session JSON 的逻辑都已删除，当前 CLI stop 只围绕 TCP sidecar 与 daemon shutdown。
- **本轮运行态再次核实**：2026-05-30 最新一次 `browser list` 实测返回 5 个 healthy sessions（`cli-more-states-2`、`cli-state-queries`、`human-flow`、`smoke-history2`、`smoke-shot`）；`doctor --quick` 当前仍只有 1 个 fail（`browser.executable`）；这个 session 数量是运行时态，不应写死成仓库事实。
- **活跃代码面再次 grep 核实**：当前 `rust/src/cli`、`README.md` 与 `skills/openpage-test/*` 里没有重新出现活跃 `serve --stdio`、`open_page()`、`load_session()`、`save_session()`、CLI-side `Browser::connect()` 或旧 `page get/page url/page title/page screenshot` 用户面；剩余命中集中在归档历史报告与跟踪文件。
- **`serve.rs` 的 origin 透传已收成纯 helper 并补单测**：当前 `payload_with_origin(...)` / `payload_with_origin_and_title(...)` / `payload_object(...)` 已把 `webpage.html`、`webpage.run_js`、`page.selected_text`、`element.text/html/attr` 以及 snapshot 根 payload 的 origin/title 注入逻辑集中到一处，继续停留在外壳/合约层，不碰浏览器、CDP 或元素交互真相源。
- **本轮外壳层测试已补证据**：新增 `serve.rs` 的 payload helper 单测 3 个，并重新通过了 `protocol.rs`、`connection.rs`、`doctor.rs` 与 snapshot 文本/ref 的定向单测；`cargo check`、`browser list`、`doctor --quick` 也已再次复核当前本机状态。
- **`doctor.rs` 的 machine-friendly summary 又向竞品靠了一步**：当前 `summary` 不只返回计数，还会稳定返回 `warn_ids` / `fail_ids` / `info_ids` / `fixable_ids`，这样脚本和 agent 不必再重扫全部 `checks` 才知道当前本机究竟卡在哪些检查项上。
- **活跃 smoke 文档已同步当前本机真相**：`skills/openpage-test/references/cli-smoke.md` 已从 4 个 healthy sessions 更新到 5 个，并写明 `doctor --quick` 的 actionable summary id 列表；`README.md` 的 doctor 段也已补上这一点。
- **唯一协议的 parser 回归护栏已补上**：当前 `rust/src/cli/oneshot.rs` 已新增拒绝测试，明确保证这些废弃入口继续 parse 失败：
  - `serve --stdio`
  - `page get`
  - `page url`
  - `page title`
  - `page screenshot`
- **活跃文档也已明确“这些旧入口是有意拒绝”的当前真相**：`README.md` 与 `skills/openpage-test/references/cli-smoke.md` 现在都写明旧 `page *` 和 `serve --stdio` 不是遗漏，而是被明确移除并由测试守住的废弃面。
- **顶层 JSON error 外壳已继续收紧**：当前 `protocol.rs` 已新增稳定 `openpage_error_kind(...)` 映射，以及 `simple_openpage_error(...)` / `response_openpage_error(...)` helper；CLI 顶层、batch 错误输出与 raw TCP daemon runtime error 现在都不再笼统返回 `openpage`，而是给出稳定 `error.kind`，例如 `unsupported_operation`、`browser_operation`、`timeout`、`io`、`serialization`。
- **顶层 parse/input 拒绝也已开始走统一 JSON 外壳**：当前 `rust/src/cli/mod.rs` 不再把 clap parse 失败直接裸打印成人类文本；除了 help/version 仍保持 clap text 之外，像 `page url`、`--set-browser-path` 这类被拒绝的旧/无效入口现在都会返回 `{"ok":false,"error":{"kind":"invalid_input",...}}`。
- **本轮还顺手修掉了一个当前工作树的最小 compile blocker**：`rust/src/webpage.rs` 里 session timeout wrapper 与当前 `SessionPage` 真接口存在命名漂移；已做最小对齐（`timeout_secs` / `set_timeout` 与 `HashMap` import），目的是恢复验证链路，不是改产品语义。
- **`doctor --quick --fix` 已落地并在本机执行过**：当前 `doctor` 新增了一个只做外壳层清理的修复入口，会删除 `OPENPAGE_HOME/sessions/*.json` 这类已经不再驱动活跃 TCP CLI 路径的 legacy session JSON 文件；本机 `/Users/yuuu/.openpage/sessions` 里的 4 个旧 JSON 已被清掉。
- **本机当前剩余问题已进一步收口**：2026-05-30 最新复核里，`browser list` 现在返回 6 个 healthy sessions、0 incomplete、0 cleaned；`doctor --quick` 已不再有 `env.legacy_sessions` warning，唯一 fail 只剩 `browser.executable`，即 `rust/configs.ini` 里的 `browser_path=/tmp/dp-browser` 在本机不可解析，而 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 是可用候选。
- **本机 browser_path 问题已有不污染仓库默认值的外壳层解法**：当前 `Browser::launch` 与 `doctor` 都已开始识别 `OPENPAGE_BROWSER_PATH`。在本机上设置 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 后：
  - `doctor --quick` 转为全 pass
  - full `doctor` 转为全 pass，并成功完成 live headless launch smoke
  - `browser start --headless https://example.com -> title -> browser stop` 也已实测通过
  这让 machine-local 浏览器路径不再需要写死进仓库 `rust/configs.ini`。
- **`doctor` 的失败提示也已跟进新的外壳层真相**：当前 `missing_browser_message(...)` 和 `browser_executable_fix(...)` 不再只让用户改 `rust/configs.ini` 或传 `--browser-path`；它们也会明确提示可以设置 `OPENPAGE_BROWSER_PATH` 做 process-local override。活跃 smoke 文档里的常见失败说明也已经同步。
- **运行时 browser create 现在也开始和 `doctor` 用同一条 launch-config 链**：`rust/src/cli/serve.rs` 里的 `webpage.create` 不再从裸 `LaunchOptions::default()` 起步，而是改成 `LaunchOptions::from_ini(None)` 起步，再用 daemon 请求参数覆盖。这样 raw TCP daemon、named-session CLI 和 `doctor` 对 browser_path/config 的理解开始一致。
- **这条 runtime/doctor 对齐已经在本机被正反两面验证过**：
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 时，`browser start --headless https://example.com -> title -> browser stop` 通过
  - 不带 `OPENPAGE_BROWSER_PATH` 时，同一条 `browser start` 会像 `doctor` 一样因为当前配置里的 `/tmp/dp-browser` 在本机不可解析而失败
- **2026-05-30 本机 truth 又刷新了一次**：当前工作树里的 `rust/configs.ini` 已不是此前的 `browser_path=chrome`，而是本地脏改后的 `browser_path=/tmp/dp-browser`；因此今天重新实测时：
  - `browser list` 返回 6 个 healthy sessions、0 incomplete、0 cleaned
  - `doctor --quick` 的唯一 fail 仍是 `browser.executable`，但失败对象已经变成 `/tmp/dp-browser`
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 时，`doctor --quick` 重新转为全 pass
- **`webpage.create` 的 session 侧默认值也已收口到同一份 ini truth**：当前 `rust/src/cli/serve.rs` 不再用 `SessionOptions::default()` 硬编码出一套 session 默认值，而是改成 `SessionOptions::from_ini(None)` 起步，仅在请求显式提供 `timeout_secs` / `user_agent` 时覆盖。这样 runtime 的 launch/session 两侧都开始共用同一份配置真相。
- **这轮 session-config 收口已经补了直接证据**：
  - 新增 `serve.rs` 定向单测 2 个，验证 `session_options_from_request(...)` 会保留 ini 默认值，并在请求显式给出参数时正确覆盖
  - `cargo check --manifest-path rust/Cargo.toml` 通过
  - `browser start --session session-config-noenv --headless https://example.com` 现在会像 `doctor` 一样因为 `/tmp/dp-browser` 失败
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 的 `browser start --session session-config-seq --headless https://example.com -> title -> browser stop` 通过
- **daemon 请求重试路径又向竞品外壳层靠了一步**：当前 `rust/src/cli/connection.rs` 的 `send_request()` 不再只在第一次发送前 `ensure_daemon()` 一次；现在每次重试前都会重新 ensure。这样如果 daemon 在第一次 ensure 之后、真正读写 socket 之前崩掉，下一次重试会重新走 sidecar 检查 / stale kill / respawn，而不是盲重试同一个坏状态。
- **这轮 request-retry 收口已经补了直接证据**：
  - 新增 `connection.rs` 定向单测 2 个，验证 transient error 后会重新 ensure，而 non-transient error 会立即停下
  - `cargo test --manifest-path rust/Cargo.toml send_request_with_retry_ -- --nocapture` 通过
  - `cargo check --manifest-path rust/Cargo.toml` 通过
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 的 `browser start --session retry-shell-check --headless https://example.com -> title -> browser stop` 通过
- **活跃 smoke 文档不再写死 daemon session 数量**：`skills/openpage-test/references/cli-smoke.md` 现在不再把某个 healthy session 数字写成固定事实，而是明确说明 exact count 是 runtime-local，会随 named-session smoke daemon 的创建和遗留而漂移。
- **`browser stop` 的 daemon 生命周期也已收口**：当前 `rust/src/cli/oneshot.rs` 不再只是“发一个 `daemon.shutdown` 然后盲删 sidecars”；它现在会走 `connection.rs::shutdown_daemon(...)`：
  - ready daemon 先尝试优雅 shutdown
  - 然后轮询确认 daemon 已退出
  - 如果 daemon 仍活着，则回退到 forced stale kill
  - 最终再清 sidecars
- **这轮 stop-lifecycle 收口已经补了直接证据**：
  - 新增 `connection.rs` 定向单测 1 个，验证 dead/stale sidecars 会被 `shutdown_daemon(...)` 清掉
  - `cargo test --manifest-path rust/Cargo.toml cli::connection::tests:: -- --nocapture` 通过
  - `cargo check --manifest-path rust/Cargo.toml` 通过
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 的顺序 smoke：
    - `browser start --session stop-shell-check --headless https://example.com`
    - `browser stop --session stop-shell-check`
    - `browser list`
    - 结果里已不再包含 `stop-shell-check`
  - `browser stop` 当前 JSON 结果也会额外返回：
    - `had_daemon`
    - `forced`
- **竞品借鉴文档已补到“本地落位”粒度**：根目录 `竞品文档-考虑借鉴的部分v1.md` 现已额外写清：
  - 2026-05-30 当前本地真实情况
  - 哪些残留项只是 archived/compat，不应误判成协议分叉
  - 竞品外壳层代码分别该落到 OpenPage 哪个本地文件
  - 哪些本地文件该承担什么职责、哪些边界不要越界
- **2026-05-30 最新本机复核已再次落证据**：当前工作树和本机运行态重新确认：
  - `browser list` 返回 8 个 healthy sessions、0 incomplete、0 cleaned
  - 8 个 healthy sessions 分别是 `cli-more-states-2`、`cli-state-queries`、`definitely-missing`、`human-flow`、`human-gap-check`、`smoke-history2`、`smoke-shot`、`smoke_eval_5554`
  - `definitely-missing` 名字虽然可疑，但当前运行时证据显示它 `alive=true`、`ready=true`，不能按名字误删
  - `doctor --quick` 当前唯一 fail 仍是 `browser.executable`，失败对象是 `rust/configs.ini` 里的 `/tmp/dp-browser`
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 时，`doctor --quick` 转为全 pass
  - 编译后的 `openpage --help` 已直接写明 active CLI protocol 是 TCP-backed daemon only；`openpage serve --help` 已直接写明 removed `serve --stdio` 继续 rejected
  - 活跃面 grep 现在仍只在 reject tests、活跃文档里的“removed on purpose”说明、以及 archived/跟踪材料里命中旧 `serve --stdio` / `page *` / one-shot attach 表述
  - 顺序 `browser start -> title -> browser stop` 的 `latest-local-audit` smoke 通过，且 stop 后 `browser list` 不再包含该 session
- **`dp` compat surface 的“只作兼容、不作协议”已经从口头结论变成活跃护栏**：
  - `rust/src/cli/args.rs::CompatCli` help 已显式写明 compat only
  - `README.md` 与 `skills/openpage-test/references/cli-smoke.md` 已同步这个约束
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture` 已通过
- **`openpage` 活跃入口已经不再偷偷接住 compat flags**：
  - `rust/src/cli/mod.rs::should_use_dp_compat_mode(...)` 现在只会在 `dp` binary 下触发 compat 模式
  - `cargo test --manifest-path rust/Cargo.toml detects_dp_compat_mode_only_for_dp_binary -- --nocapture` 已通过
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --set-browser-path /tmp/chrome` 现在会直接报 `unexpected argument`，不会再伪装成第二条 active CLI surface

## Errors Encountered
- 当前工作树已存在大量未提交变更，因此迁移时必须逐文件审计，避免覆盖已有工作。
- `daemon.shutdown` 初始只修改了 runtime 状态，没有真正退出 TCP accept 循环；现已修正并验证 sidecar 会随优雅退出清理。
- `back/forward` 在第一版 RPC 包装里存在导航完成前就读取 URL 的竞态；现已通过在包装层补 `wait.doc_loaded` 修正并重新验证。
- `click-for-new-tab` smoke 第一轮失败并非协议问题，而是测试脚本仍停留在新 tab 上就去点旧页的上传控件；调整为先 `tab switch` 回原页后，后续 `click-to-upload` / `click-to-download` / `drag-in` 全部验证通过。
- 本轮 `cargo check` 首次被 `rust/src/page.rs` 中一个现有工作树编译错误挡住：`CaptureSnapshot` 返回值从借用结果中 move 出 `String`。已做最小修复为 `.clone()`，恢复可验证状态。
- `batch` 接入为了避免“每条子命令报错后再多打一层总错误 JSON”，把 `rust/src/cli/oneshot.rs` 的返回语义调整为显式 exit code；这是外层 CLI 行为调整，不涉及浏览器/元素/CDP 内核。
- 当前本机 `doctor` 实测表明：`OPENPAGE_HOME=/Users/yuuu/.openpage`，现存多个活跃 daemon session；当前加载到的配置里 `browser_path=/tmp/dp-browser`，但本机并不能解析这个可执行文件，因此 `doctor --quick` 已经会直接失败，full `doctor` 也会跳过 live launch 并给出显式修复提示。
- 一次补充的“live incomplete sidecar”脚本验证命令因为 `cargo run` 并发编译锁导致后台 `serve` 尚未写 sidecar 时就触发了 `doctor`；该次输出已判定为脚本竞态，不作为结论证据。
- repo-local smoke scripts 原先还残留旧命令语法：
  - `page get`
  - `page url`
  - `page title`
  - `page screenshot`
  现已统一到当前真实 CLI 语法：
  - `goto`
  - `url`
  - `title`
  - `screenshot`
- `browser list` 原先只暴露 healthy sessions，无法直接把 `daemon_inventory()` 的 `incomplete` / `cleaned` 带给用户；现已做加法式修正。
- `snapshot` 原先只返回 `snapshot` 数组，缺少面向 agent 的文本摘要和 ref 索引；现已在不修改内部元素/CDP 实现的前提下补到 CLI/daemon 合约层。
- 当前历史文档仍然保留大量旧事实正文，这是有意保留的回溯材料；本轮只做了显式降级标注，没有重写全文。
- 给 `doctor.rs` 新增清理测试时，第一次把 `remove_legacy_session_files()` 直接写成了裸调用；由于测试在子模块里，需要改成 `super::remove_legacy_session_files()`，修正后定向单测通过。
- 给 `browser.rs` 新增 env override 测试时，第一次把常量写成了未导入名字；已改成 `super::OPENPAGE_BROWSER_PATH_ENV`，定向单测随后通过。
- `serve.rs` 这轮为了让 runtime 和 `doctor` 对齐，改成了从 ini 配置起步；因此在本机不带 `OPENPAGE_BROWSER_PATH` 时，`browser start` 现在会和 `doctor` 一样受当前配置里的 `/tmp/dp-browser` 影响，这不是回归，而是刻意去掉两套不一致的 browser-path 真相。
- 2026-05-30 这轮重新取证时又发现一个“旧结论过期”问题：当前工作树里的 `rust/configs.ini` 已被本地脏改成 `browser_path=/tmp/dp-browser`，所以先前跟踪文件里写死的 `browser_path=chrome` 已不再代表今天这台机器的真实失败对象；本轮已改为把新 truth 追加记录，而不是继续复用旧说法。
- 一次并发 smoke 把 `title --session session-config-env` 和 `browser stop --session session-config-env` 同时跑了，导致 `title` 命中了 `unknown target`；后续顺序重跑 `browser start -> title -> browser stop` 已拿到有效通过证据。
- 本轮验证链路又被当前工作树里的其它 compile blockers 挡了一次：
  - `rust/src/settings.rs` 这个未跟踪新文件里有一个 `FnMut` 捕获值 move 和一个测试导入问题
  - `rust/src/page.rs` 测试里有两个 `matches!` 模式借用问题
  - 这三处都只做了最小修复，用来恢复 `cargo check` / 定向 `cargo test` 证据链，不属于本轮外壳层设计变更本身
- 2026-05-30 这轮再次取证时，当前工作树里的 `rust/src/browser.rs` 也暂时挡住了验证链路：
  - `tab_infos()` 里的 `Ok(...)` 泛型返回值无法推断
  - `wait_for_new_tab()` 里把 `Option<String>` 和新的 `explicit_current_tab: bool` 签名混用，导致编译失败
  - 已只做最小修复以恢复 `cargo check` 和本地协议面取证，不借此改动任何 CLI/daemon 协议边界
- healthy session count 不是稳定仓库事实，也不适合作为活跃 smoke 文档里的固定数字；以每次顺序执行后的 `browser list` 实测输出为准，不要把某一次运行态数量写死成仓库真相。
- 上一条旧结论在本轮已被更强证据推翻：顺序 `browser start -> browser stop -> browser list` smoke 现在已经确认，当前 `browser stop` 收口后，刚停掉的 `stop-shell-check` session 不会继续留在活跃 `browser list` 里。此前的“session 数量漂移”问题主要来自并发取证与历史遗留 daemon，不再是当前 stop 生命周期本身的直接证据。

## Status
**Currently between Phase 6 and Phase 7** - 唯一 TCP daemon 路径仍然保持稳定，但当前重点已经转到“继续补外壳层借鉴 + 持续把最新本机 truth 写回文档”。2026-05-31 这轮最新本机复核再次确认：`browser list` 当前返回 18 个 healthy sessions、0 incomplete、0 cleaned；默认 `doctor --quick` 的 summary 是 `pass=22 / warn=0 / fail=1 / info=1 / total=24`，唯一 fail 仍是 `browser.executable=/tmp/dp-browser`；带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 时 `doctor --quick` 转为 `pass=23 / warn=0 / fail=0 / info=1 / total=24`；编译后的 `openpage --help` 与 `openpage serve --help` 继续直接把 TCP-only / removed `serve --stdio` 真相写进活跃 help 文本，而 `dp` 继续被钉死为 compat-only helper。本轮没有继续动 `rust/src/cli/oneshot.rs`，因为本地仍有活跃 `git add -p rust/src/cli/oneshot.rs` 交互进程；因此这轮先把最新本机 truth 与竞品借鉴边界同步回文档，避免干扰用户当前的 staging。下一步继续沿 README / skill docs / 竞品底稿收敛最新本机 truth，并在 staging 风险解除后优先挑这种只动 `connection.rs` / `doctor.rs` / `oneshot.rs` 的外壳点小步接入。

## 2026-05-30 增量进展：active-session 外壳边界同步

- 已把最新落地的 session 边界同步到活跃 help / README / skill docs：

## 2026-05-31 文档同步：竞品借鉴清单刷新

- 已把 `竞品文档-考虑借鉴的部分v1.md` 补成可直接复用的借鉴底稿。
- 这次只补文档，不改实现层代码。
- 文档里现在多了一张前置速查表，明确了：
  - 哪些竞品文件适合直接按骨架 copy 后微调
  - 哪些只适合借思路
  - 哪些明确禁止碰
- 这能降低后续继续借竞品时“抄过界”的风险，尤其是避免误抄 `cli/src/native/*`。

## 2026-05-31 增量进展：AI-first snapshot 外壳增强

- 这轮继续只借竞品的 agent-facing snapshot contract，不借它的 native snapshot / CDP / element 内核。
- 已在 `rust/src/cli/serve.rs` 落地两个 outer-shell 方向的增强：
  - `snapshot` 现在会补充 `label` / `checked` / `selected` / `disabled` 这类 agent 友好的状态元数据
  - 每次新 `snapshot` 开始前会先清掉现有 DOM 上残留的 `data-op-ref`，再重新分配新 ref 集，降低动态页面上的 stale ref 误点风险
- 这轮仍然不改：
  - `rust/src/browser.rs`
  - `rust/src/page.rs`
  - `rust/src/element.rs`
  - `rust/src/webpage.rs`
- 也就是说，这一步仍然是 shell / contract 层增强，不是浏览器或 CDP 内核替换。

## 2026-05-31 增量进展：version-mismatch daemon 护栏收紧

- 这轮继续只动 `rust/src/cli/connection.rs` 这一层，不碰浏览器/CDP/定位/交互内核。
- 新护栏解决的是一个更核心的协议稳定性问题：
  - 之前 follow-up `--session` 命令只检查 daemon 是否 `ready`
  - 没有检查该 daemon 的 `.version` 是否和当前 CLI 匹配
  - 这会让旧版本 live daemon 混进当前 TCP 协议面
- 现在已经改成：
  - `ensure_existing_daemon()` 对 version mismatch 直接 fail-fast
  - `browser status` / `browser list` / `browser logs` / `doctor inventory` 现在会显式暴露：
    - `state="incompatible"`
    - `reasons=["version_mismatch"]`
    - `version_matches_current_cli=false`
- 这一步的意义很直接：
  - 唯一 TCP daemon 协议不只是“入口唯一”
  - 还变成“follow-up 命令不会继续和旧版本 daemon 混跑”

## 2026-05-31 增量进展：`doctor --quick --fix` 也纳入 incompatible daemon 清理

- 这轮继续只动 `doctor.rs` / `connection.rs` 外壳层。
- 当前已经把上一轮的 version-mismatch fail-fast 补成统一修复闭环：
  - `doctor --quick` 能报告 `state="incompatible"`
  - follow-up `--session` 命令会拒绝旧版本 daemon
  - `doctor --quick --fix` 现在也会 stop incompatible live daemon session
- 这意味着“唯一 TCP 协议”现在不只是拒绝旧入口、拒绝旧 daemon，还具备统一 cleanup path。

## 2026-05-31 增量进展：active docs 与 `browser logs` incompatible 护栏同步

- 这轮是小步收口：
  - `skills/openpage-test/SKILL.md`
  - `skills/openpage-test/references/install.md`
  - `skills/openpage-test/references/cli-smoke.md`
  现在都已经把 `doctor --quick --fix` 的适用范围更新到：
  - legacy session JSON residue
  - incompatible daemon sessions
  - incomplete unready daemon sessions
- 同时补了 `rust/src/cli/oneshot.rs` 的测试护栏，确认 `browser logs` 在 incompatible session 上也会保留：
  - `state="incompatible"`
  - `reasons=["version_mismatch"]`
  - `version_matches_current_cli=false`

## 2026-05-31 增量进展：session-level `fix` guidance 真相源开始收口

- 这轮继续只动 `connection.rs` / `doctor.rs` / `oneshot.rs` 这一层。
- 当前新增的是一个更像竞品 outer-shell 的诊断增强：
  - 不再只给 `state` / `reasons`
  - 而是开始把 session-level `fix` guidance 下沉到 connection/control-plane 真相源
- 现在以下几处会开始共享同一套 session 修复建议：
  - `browser status`
  - `browser logs`
  - `browser list`
  - `doctor` 的 session 相关 check
- 这意味着后续 agent 不必再自己根据 `state` / `reasons` 反推“下一步该 stop、restart 还是 start”。
  - `browser start` 和 `goto` 是仅存的 bootstrap 入口
  - `title` / `snapshot` / `click` / `js` / `screenshot` 这类 follow-up 命令现在要求 session 已经 active
  - inactive session 会 fail fast，而不是静默拉起新 daemon/browser
- 已把这条外壳借鉴写回 `竞品文档-考虑借鉴的部分v1.md`，并明确落点：
  - `rust/src/cli/oneshot.rs`
  - `rust/src/cli/connection.rs`
- 待本轮验证补齐后，再继续看是否还有别的命令入口会意外绕回 auto-start 路径

## 2026-05-30 增量进展：compile gate 恢复 + 本机实情同步

- 为了继续做本地协议面核实，先做了 `rust/src/browser.rs` 的最小编译修复：
  - `tab_infos()` 的返回值显式标注为 `OpenPageError`
  - `wait_for_new_tab()` 按当前 `find_new_tab_id(..., explicit_current_tab: bool)` 签名收口
- 修复后重新跑通的本地核实命令：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --help`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
- 这轮更强的新证据：
  - 当前 healthy session 数量已从之前记录的 7 个变成 8 个，新增可见 healthy session 是 `human-gap-check`
  - `doctor --quick` 默认仍只 fail 在 `/tmp/dp-browser`
  - override 到 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 后，`doctor --quick` 转为全 pass
  - 当前工作树的活跃 help、单测、runtime inventory 和 reject surface 仍然共同指向唯一 TCP daemon 真相

## 2026-05-31 增量进展：本地真相复核 + shell-only 最小接入确认

- 先重新核实了当前工作树，而不是沿用上轮 handoff 里的 compile blocker 结论：
  - `cargo check --manifest-path rust/Cargo.toml` 已通过
  - 说明之前提到的 `rust/src/cli/oneshot.rs` 对 `Command::Clipboard` / `Command::Permissions` 非穷尽匹配问题，当前工作树里已经被修掉
- 继续验证唯一 TCP 协议护栏：
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
  - 三个都通过
- 对“最小接入但不借竞品 native 内核”这件事，本轮确认 clipboard / permissions 已经是一个现成样板：
  - `rust/src/cli/args.rs` 已公开 `Clipboard` / `Permissions` 子命令
  - `rust/src/cli/oneshot.rs` 已只通过 `rpc_webpage(...)` 走 daemon：`clipboard.read/write`、`permissions.set/reset`
  - `rust/src/cli/serve.rs` 已只在 daemon dispatch 层调用 `page.clipboard_*` / `page.set_permission` / `page.reset_permissions`
  - `rust/src/webpage.rs` 已有 driver-mode wrapper
  - `rust/src/page.rs` 已有内部实现与 clipboard runtime regression test
  - 这正符合“只借外壳层，不借 `agent-browser/cli/src/native/*`”的边界
- 2026-05-31 本机运行态重新取证：
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - 当前返回 **15 个 healthy sessions、0 incomplete、0 cleaned**
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - 当前唯一 fail 仍是 `browser.executable`
    - 失败对象仍是 `rust/configs.ini` 里的 `/tmp/dp-browser`
  - `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - 转为 **0 fail**
  - `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start https://example.com --session latest-local-audit-20260531 --headless`
    - 后续 `title --session latest-local-audit-20260531` 返回 `Example Domain`
    - `browser stop --session latest-local-audit-20260531` 成功，随后 `browser list` 已确认这个临时 session 不再残留
- 活跃面 grep 继续成立：
  - `serve --stdio`、旧 `page *`、`open_page()`、`load_session()`、`save_session()` 当前只剩：
    - reject tests / help / README 里的明确 rejected 说明
    - archived 历史文档里的旧事实
    - Rust 库内部仍然存在但不等于 CLI 分叉的 `Browser::connect()`
  - 没有重新回流到活跃 CLI 执行路径

## 2026-05-31 增量进展：browser logs 外壳接入 + 文档 truth 刷新

- 新确认并验证了一个继续可借的非-CDP 外壳点：
  - `rust/src/cli/args.rs` 新增 `browser logs`
  - `rust/src/cli/oneshot.rs` 新增 `run_browser_logs(...)`
  - 只复用现有 `daemon_status().log_path` 和 sidecar truth，不接触浏览器/CDP/locator/interaction 内核
- 已重新通过的定向验证：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml parses_browser_logs_tail -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_keeps_last_lines -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_handles_zero_and_large_limits -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
- 2026-05-31 本机运行态新证据：
  - `rust/target/debug/openpage browser list`
    - 当前返回 **17 个 healthy sessions、0 incomplete、0 cleaned**
  - `rust/target/debug/openpage doctor --quick`
    - summary 为 `pass=21 / warn=0 / fail=1 / info=1 / total=23`
    - `fail_ids=["browser.executable"]`
    - `fixable_ids=["browser.executable","browser.launch"]`
  - `rust/target/debug/openpage browser logs --session human-flow --tail 20`
    - 返回 `exists=false`、`content=null`
    - 说明该 session 当前没有可读的持久化 stderr log
  - `rust/target/debug/openpage browser logs --session clipboard-probe-20260531 --tail 20`
    - 返回 `exists=true`
    - 已读到 tailed stderr：`WebSocket protocol error: Connection reset without closing handshake`
- 这条能力的意义：
  - 很符合 `agent-browser` 可借的 outer-shell 方向
  - 能补强 daemon/doctor 之后的现场排障路径
  - 仍然不需要引入竞品 `native/*`

## 2026-05-31 增量进展：当前工作树 compile gate 再确认 + tracking files 对齐

- 先重新以当前工作树为准复核，而不是沿用上一轮记忆：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml parses_browser_logs_tail -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_keeps_last_lines -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_handles_zero_and_large_limits -- --nocapture`
  - `rust/target/debug/openpage browser list`
  - `rust/target/debug/openpage doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' rust/target/debug/openpage doctor --quick`
  - `rust/target/debug/openpage browser logs --session human-flow --tail 20`
  - `rust/target/debug/openpage browser logs --session clipboard-probe-20260531 --tail 20`
  - `rust/target/debug/openpage --help`
  - `rust/target/debug/openpage serve --help`
- 这轮还需要记住一个“不是协议变更”的本地事实：
  - 当前 dirty worktree 里 `rust/src/element.rs` 为了恢复编译，`Frame::new(...)` 现在显式传入了 `Arc::clone(self.none_element_runtime_config_handle())`
  - 这是现有 `Frame::new` 签名对齐所需的最小 compile-recovery 修复，不是新的协议设计，也不是借竞品 `native/*`
- 2026-05-31 这轮复核拿到的当前真相：
  - `browser list` 仍返回 **17 个 healthy sessions、0 incomplete、0 cleaned**
  - plain `doctor --quick` 仍是 `pass=21 / warn=0 / fail=1 / info=1 / total=23`
  - 其唯一 fail 仍是 `browser.executable=/tmp/dp-browser`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'` 下，`doctor --quick` 当前变为 `pass=22 / warn=0 / fail=0 / info=1 / total=23`
  - `browser logs --session human-flow --tail 20` 仍返回 `exists=false` / `content=null`
  - `browser logs --session clipboard-probe-20260531 --tail 20` 仍返回 `exists=true`，且 tailed content 里能看到 `Connection reset without closing handshake`
  - 编译后的 `openpage --help` 与 `openpage serve --help` 仍把 TCP-only / removed `serve --stdio` 写成活跃 help 护栏
- 这一轮文档同步的结论不变：
  - 继续借 `connection.rs` / `doctor.rs` / `protocol.rs` / agent docs 这种 outer-shell 设计
  - 不借 `agent-browser-main/cli/src/native/*`
  - 当前更需要的是继续把最新本机 truth 写回文档，而不是重做浏览器/CDP/元素交互内核

## 2026-05-31 增量进展：`doctor --quick --fix` 外壳层借鉴完成验证

- 这轮不再只凭代码阅读判断 `doctor --fix` 是否站得住，而是把当前实现重新跑成了完整证据链：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_reports_stale_daemon_sidecar_cleanup -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incomplete_unready_daemon_session -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor`
  - synthetic smoke under `OPENPAGE_HOME=/tmp/openpage-doctor-fix-*` with:
    - one legacy session JSON file
    - one stale dead daemon sidecar set
    - one incomplete unready daemon session backed by `/bin/sleep 30`
    - followed by `openpage doctor --quick --fix`
- 这轮拿到的新事实：
  - `openpage page url` 当前直接返回统一 JSON 壳里的 `error.kind="invalid_input"`
  - `openpage serve --stdio` 当前也直接返回统一 JSON 壳里的 `error.kind="invalid_input"`
  - 当前本机运行态仍是 **17 个 healthy sessions、0 incomplete、0 cleaned**
  - plain `doctor --quick` 仍是 `pass=21 / warn=0 / fail=1 / info=1 / total=23`，唯一 fail 仍然是 `browser.executable=/tmp/dp-browser`
  - override `doctor --quick` 当前为 **0 fail**
  - override full `doctor` 当前也为 **23/23 pass**，说明 launch smoke 和 browser-path 对齐这条链是闭合的
  - synthetic `doctor --quick --fix` 当前实测会：
    - 删除 legacy session JSON
    - 报告 stale dead daemon sidecars cleanup
    - 停掉 incomplete unready daemon session
    - 修复后临时 `OPENPAGE_HOME` 目录下不再残留 sidecar 文件
    - 被标记为 incomplete 的那个 `sleep` 子进程在修复后已不再存活
- 这一轮最重要的判断：
  - `doctor --quick --fix` 已经成为一个**被验证过的外壳层借鉴样板**
  - 它借的是竞品 `connection.rs` / `doctor/*` 的控制流和状态治理思路
  - 它没有引入任何竞品 `native/*`、CDP、locator 或 interaction 内核

## 2026-05-31 增量进展：`browser stop --all` 外壳层借鉴接入并验证

- 这轮继续沿“只借 shell/control-plane，不借 native 内核”的边界前进，新增的是竞品 `close --all` 对应的 OpenPage 版：
  - `rust/src/cli/args.rs`：`browser stop` 现在支持 `--all`
  - `rust/src/cli/oneshot.rs`：新增 stop-all session 聚合与逐 session shutdown 控制流
  - `README.md` / `skills/openpage-test/references/session-management.md`：同步写入新的 shell-level cleanup 用法
- 这轮验证链路：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo build --manifest-path rust/Cargo.toml --bin openpage`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser stop --help`
  - `cargo test --manifest-path rust/Cargo.toml parses_browser_stop_all -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_stop_all_sessions_deduplicates_and_keeps_alive_incomplete_sessions -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml parses_batch_with_commands -- --nocapture`
  - synthetic smoke under `OPENPAGE_HOME=/tmp/openpage-stop-all-*`:
    - 起两个原始 `openpage serve --session alpha/beta --port 0`
    - `browser list`
    - `browser stop --all`
    - 再次 `browser list`
    - 检查两个 daemon pid 是否已退出
  - 回归单 session stop：
    - synthetic `OPENPAGE_HOME=/tmp/openpage-stop-one-*`
    - `serve --session review --port 0`
    - `browser stop --session review`
    - 再次 `browser list`
- 这轮拿到的新事实：
  - `openpage browser stop --help` 现在已明确出现 `--all`
  - synthetic stop-all smoke 当前返回：
    - `{"stopped":2,"sessions":["alpha","beta"],"all_stopped":true,"failed":[]}`
  - stop-all 后，同一个临时 `OPENPAGE_HOME` 下的 `browser list` 已为空
  - 两个 raw daemon pid 在 stop-all 后都已不再存活
  - 单 session `browser stop --session review` 回归仍然通过，且返回 `had_daemon=true`、`forced=false`
- 这轮最重要的判断：
  - `browser stop --all` 是另一个已经被实证过的 outer-shell borrow point
  - 它复用的是现有 daemon inventory / shutdown 真相源
  - 它没有引入任何竞品 browser/CDP/locator/interaction 内核

## 2026-05-31 增量进展：`browser list` machine-friendly summary 接入并验证

- 这轮继续保持只动外壳层，给 `browser list` 增加了更适合 agent/script 直接消费的摘要：
  - `rust/src/cli/oneshot.rs`：`browser list` 现在额外返回 `summary { healthy, incomplete, cleaned, total }`
  - `README.md` / `skills/openpage-test/references/session-management.md` / `skills/openpage-test/references/cli-smoke.md`：同步写入新输出形态
- 这轮验证链路：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_summary_counts_all_categories -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - synthetic smoke under `OPENPAGE_HOME=/tmp/openpage-list-summary-*`:
    - `openpage serve --session summary-check --port 0`
    - `openpage browser list`
- 这轮拿到的新事实：
  - 当前机器上的 `browser list` 现在返回：
    - `summary.healthy=17`
    - `summary.incomplete=0`
    - `summary.cleaned=0`
    - `summary.total=17`
  - synthetic 单 session smoke 下，`browser list` 返回：
    - `summary.healthy=1`
    - `summary.incomplete=0`
    - `summary.cleaned=0`
    - `summary.total=1`
- 这轮最重要的判断：
  - 这是一条纯 machine-friendly 的外壳层增强
  - 它进一步强化了“先调研清楚本地当前真相，再让 agent/脚本消费”的目标

## 2026-05-31 增量进展：`browser status` state/reasons 接入并验证

- 这轮继续补 shell-level 状态可观测性，没有触碰任何浏览器/CDP/元素内核：
  - `rust/src/cli/oneshot.rs`：`browser status` 现在额外返回 `state`
  - 当 `state="incomplete"` 时，还会额外返回：
    - `incomplete`
    - `reasons`
- 这轮验证链路：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml incomplete_session_reasons_report_missing_version_and_not_ready -- --nocapture`
  - `cargo build --manifest-path rust/Cargo.toml --bin openpage`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser status --help`
  - synthetic smoke under `OPENPAGE_HOME=/tmp/openpage-status-shapes-*`:
    - `serve --session healthy --port 0`
    - one live incomplete session using `/bin/sleep 30` + `.pid/.port` but no `.version`
    - `browser status --session healthy`
    - `browser status --session incomplete`
    - `browser status --session missing`
- 这轮拿到的新事实：
  - healthy session 当前返回 `state="healthy"`
  - incomplete session 当前返回：
    - `state="incomplete"`
    - `reasons=["missing_version","daemon_not_ready"]`
  - missing session 当前返回 `state="inactive"`
- 这轮最重要的判断：
  - 这也是纯 outer-shell / machine-friendly 增强
  - 它进一步帮助 agent 和脚本区分“healthy / incomplete / inactive”三种本地 session 真相

## 2026-05-31 增量进展：`browser list` 条目级 state/reasons 接入并验证

- 这轮继续保持只动 shell/output 层，不碰任何浏览器/CDP/locator/interaction 内核：
  - `rust/src/cli/oneshot.rs`：`browser list` 现在不只返回 `summary`
  - `sessions[]` 条目会显式带 `state="healthy"`
  - `incomplete[]` 条目会显式带 `state="incomplete"` 和稳定 `reasons[]`
  - `cleaned[]` 条目会显式带 `state="cleaned"`
  - 顺手修掉了当前工作树里一个 shell-layer compile blocker：`DownloadsCommand::Open/Reveal` 现在在 `run_downloads(...)` 中有最小实现，不再因为非穷尽匹配卡住后续协议验证
- 这轮验证链路：
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_ -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml incomplete_session_reasons_ -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- 这轮拿到的新事实：
  - 当前机器上的 `browser list` healthy entries 现在都带 `state="healthy"`
  - 当前机器仍是 `healthy=17 / incomplete=0 / cleaned=0 / total=17`
  - `openpage_help_marks_tcp_daemon_as_only_active_protocol` 再次通过
  - `page url` 和 `serve --stdio` 当前都继续返回统一 JSON 壳里的 `error.kind="invalid_input"`
- 这轮最重要的判断：
  - 这是对 daemon inventory 输出形态的继续结构化，不是新协议
  - 它继续服务于“先把本地真相暴露清楚，再让 agent/script 消费”

## 2026-05-31 增量进展：`browser logs` state/reasons 对齐接入并验证

- 这轮继续保持只动 shell/control-plane 层：
  - `rust/src/cli/oneshot.rs`：`browser logs` 现在不再只返回 log path / tail / content
  - 它会复用 `browser_status_payload(...)`，因此和 `browser status` 对齐同一份 `state`
  - 如果 session 是 incomplete，`browser logs` 也会继续带 `reasons[]`
- 这轮验证链路：
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_keeps_last_lines -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_handles_zero_and_large_limits -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser logs --session human-flow --tail 5`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- 这轮拿到的新事实：
  - `browser logs --session human-flow --tail 5` 当前返回：
    - `state="healthy"`
    - `exists=false`
    - `content=null`
    - `path=/Users/yuuu/.openpage/daemon/human-flow.log`
  - 说明当前 `human-flow` session 是 healthy，但还没有 persisted stderr log 文件
  - TCP-only help 护栏和两个 removed surface runtime reject 继续成立
- 这轮最重要的判断：
  - 这继续沿着竞品的诊断/日志外壳借鉴推进
  - 但只复用了现有 daemon lifecycle 真相，没有引入任何浏览器/CDP/定位/交互内核

## 2026-05-31 增量进展：`doctor --quick` 直接暴露 inventory 真相并验证

- 这轮继续保持只动 shell/control-plane 层：
  - `rust/src/cli/doctor.rs` 现在不再只返回 `summary / checks / fixed`
  - 它会直接把当前 daemon runtime truth 作为 `inventory` 一并返回
  - `inventory` 的形态继续和现有 `browser list` 收口：
    - `summary { healthy, incomplete, cleaned, total }`
    - `sessions[]` with `state="healthy"`
    - `incomplete[]` with `state="incomplete"` + `reasons[]`
    - `cleaned[]` with `state="cleaned"`
- 这轮验证链路：
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- 这轮拿到的新事实：
  - 当前 `doctor --quick` 现在直接返回 `inventory`
  - 在当前机器上它报告：
    - `inventory.summary.healthy=17`
    - `inventory.summary.incomplete=0`
    - `inventory.summary.cleaned=0`
    - `inventory.summary.total=17`
  - 同一次 `doctor --quick` 里，唯一 fail 仍然只是 `browser.executable=/tmp/dp-browser`
  - removed surfaces runtime reject 和 TCP-only help 护栏继续成立
- 这轮最重要的判断：
  - 这一步把“调研本地当前真相”的入口进一步收口到 `doctor`
  - 但复用的仍然只是 sidecar / inventory / status 真相，不是浏览器/CDP/定位/交互内核

## 2026-05-31 增量进展：共享 incomplete-session `reasons[]` taxonomy 收口并验证

- 这轮继续保持只动 shell/control-plane 层：
  - `rust/src/cli/connection.rs` 现在开始承载共享的 incomplete-session `reasons[]` 真相
  - `browser list/status/logs` 和 `doctor` 不再各自维护一份同名但独立的 reason 计算逻辑
  - 共享 helper 还同时承载了 inventory `summary` / payload 的统一 JSON shaping
- 这轮验证链路：
  - `cargo test --manifest-path rust/Cargo.toml incomplete_daemon_reasons_report_missing_version_and_not_ready -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `rust/target/debug/openpage browser list`
  - `rust/target/debug/openpage doctor --quick`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- 这轮拿到的新事实：
  - 共享 reason helper 已经在 `connection.rs` 单测通过
  - `browser list` 与 `doctor --quick` 当前都继续报告同一份 runtime summary：
    - `healthy=17`
    - `incomplete=0`
    - `cleaned=0`
    - `total=17`
  - `doctor --quick` 当前唯一 fail 仍是 `browser.executable`
  - removed surfaces runtime reject 和 TCP-only help 护栏继续成立
- 这轮最重要的判断：
  - 这是纯外壳层“真相源收口”，可以减少后续会话把 reason taxonomy 写漂
  - 仍然完全没有触碰浏览器/CDP/元素交互真相源

## 2026-05-31 增量进展：`browser logs.content` 接入统一 boundary / truncate 输出并验证

- 这轮继续保持只动 `protocol/output` 外壳层：
  - `rust/src/cli/protocol.rs` 的统一输出过滤现在不只覆盖 `html / text / value`
  - `browser logs` 的 `result.content` 现在也会走同一条 boundary / truncate 链
  - 这让 agent 在消费 daemon log 文本时，也能用同一套 trust-boundary 心智
- 这轮验证链路：
  - `cargo test --manifest-path rust/Cargo.toml format_output_json_wraps_content_field_with_boundaries -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml format_output_json_truncates_content_field -- --nocapture`
  - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=40 cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser logs --session clipboard-probe-20260531 --tail 20`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- 这轮拿到的新事实：
  - 两个 `protocol.rs` 新单测都通过
  - 真实 CLI 验证里，`browser logs` 的 `content` 现在已经被包成：
    - `_boundary.keys=["content"]`
    - wrapped marker `OPENPAGE_PAGE_CONTENT ... key=content`
    - 截断提示 `showing 40 of 92 chars`
  - removed `page url` / `serve --stdio` runtime reject 与 TCP-only help 护栏继续成立
- 这轮最重要的判断：
  - 这是纯 agent-facing output borrow point
  - 它只动输出壳，不动浏览器/CDP/定位/交互真相源

## 2026-05-31 增量进展：竞品借鉴文档改写为 copy-ready 版本

- 这轮没有改代码，只更新后续可执行的竞品借鉴文档。
- 根目录 `竞品文档-考虑借鉴的部分v1.md` 已重写成更适合后续直接参考的版本，重点补强了：
  - 借鉴边界表
  - P0/P1/P3 优先级表
  - 竞品文件 → OpenPage 落点映射表
  - `可直接 copy` / `只借思路` / `明确不要借` 三层分类
  - TCP-only 前提下 copy 时必须删改的点
- 这轮最重要的判断：
  - `agent-browser` 对 OpenPage 的核心价值，仍然是 outer shell / control-plane / agent docs
  - 后续如果真要抄，优先顺序仍然应是 `connection.rs` → `doctor/*` → `output.rs` → top-level error shell → skill docs
  - `cli/src/native/*` 继续保持禁区
- 这轮验证：
  - 已重新核对竞品关键文件函数清单：`connection.rs`、`doctor/mod.rs`、`doctor/launch.rs`、`output.rs`、`main.rs`、`commands.rs`
  - 已重新核对 OpenPage 当前对应落点：`rust/src/cli/connection.rs`、`doctor.rs`、`protocol.rs`、`serve.rs`、`oneshot.rs`
- 当前阶段判断不变：
  - Phase 6 仍在进行中，但这份文档已经足够作为后续借鉴时的准入检查单。

## 2026-05-31 增量进展：direct/daemon 控制面错误开始暴露 machine-readable `error.fix`

- 这轮继续只动 shell/protocol 层，不碰浏览器/CDP/定位/交互内核。
- `rust/src/cli/protocol.rs` 现在做了两件事：
  - direct CLI 错误在已知控制面恢复路径下会额外暴露 `error.fix`
  - daemon `Response::error` 现在会带 raw detail + optional `fix`，避免本地 CLI 再包装时出现 message 双重前缀漂移
- `rust/src/cli/oneshot.rs::response_result(...)` 现在会保留 structured `fix`，而不是把它丢掉。
- 活跃文档已同步：
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
- 这轮最重要的判断：
  - 这是非常典型的 outer-shell borrow point：稳定错误壳 + 下一步指引 + agent/script 更少解析自由文本
  - 它继续强化唯一 TCP daemon 协议的可消费性，但没有把任何竞品 browser/CDP/runtime 内核带进来
- 这轮验证：
  - 定向测试通过：
    - `cargo test --manifest-path rust/Cargo.toml structured_fix -- --nocapture`
    - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
    - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
    - `cargo check --manifest-path rust/Cargo.toml`
  - runtime 复核通过：
    - `openpage title --session missing` 现在返回 top-level `error.fix`
    - synthetic mismatch smoke 中，version-mismatch follow-up 命令现在也返回完整 `error.fix`
    - `openpage serve --stdio` 和 `openpage page url` 继续保持 `invalid_input` rejected

## 2026-05-31 增量进展：top-level error shell 省略空 `error.fix`

- 这轮继续只动 shell/protocol 层一致性，不碰浏览器/CDP/定位/交互内核。
- 当前确认到的一个小缺口是：
  - direct CLI 错误已经开始支持 `error.fix`
  - 但在没有恢复建议时，top-level `simple_error(...)` 还会发 `fix: null`
  - 这和 daemon `ResponseError` 的 `skip_serializing_if = Option::is_none` 不一致
- 本轮已把这个缺口收掉：
  - `rust/src/cli/protocol.rs::simple_error_with_fix(...)` 现在会在 `fix` 缺失时直接省略字段
- 这轮最重要的判断：
  - 这是一条小但正确的 outer-shell 收口
  - 目标是让 direct CLI error 和 daemon error 的 JSON shape 更一致，避免脚本/agent 还要单独处理 `null`
- 这轮验证：
  - `cargo test --manifest-path rust/Cargo.toml simple_error_omits_fix_when_absent -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml structured_fix -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime 复核：
    - `openpage title --session missing` 继续带完整 `error.fix`
    - `openpage serve --stdio` 继续 rejected，且不再带 `fix: null`
    - `openpage page url` 继续 rejected，且不再带 `fix: null`

## 2026-05-31 增量进展：top-level direct error 开始暴露 `error.state` / `error.reasons`

- 这轮继续只动 shell/protocol 层，不碰浏览器/CDP/定位/交互内核。
- 当前确认到的高价值缺口是：
  - `browser status` / `browser logs` / `browser list` / `doctor.inventory` 已经有 machine-readable `state/reasons`
  - 但 direct follow-up command failure 之前只有 `error.kind/message/fix`
- 本轮已把这个缺口补上：
  - 对已知 session/control-plane 失败，top-level JSON error 现在开始暴露：
    - `error.state`
    - `error.reasons`（适用时）
  - raw daemon `ResponseError` 也同步支持同样字段
- 当前覆盖到的已知 session-control failures：
  - inactive session → `state="inactive"`
  - version mismatch → `state="incompatible"`, `reasons=["version_mismatch"]`
  - daemon not ready → `state="incomplete"`, `reasons=["daemon_not_ready"]`
- 这轮最重要的判断：
  - 这是把 shared control-plane truth 从 read-only 状态面进一步扩到 direct failure 面
  - 对 agent/script 的价值比继续堆 message 文本更高
- 这轮验证：
  - `cargo test --manifest-path rust/Cargo.toml 'state_and_reasons' -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml structured_fix -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime 复核：
    - `openpage title --session missing` 现在返回 `error.state="inactive"`
    - synthetic mismatch smoke 里 `title --session mismatch-state-smoke` 现在返回：
      - `error.state="incompatible"`
      - `error.reasons=["version_mismatch"]`
      - 完整 `error.fix`
    - `openpage serve --stdio` 继续 rejected，仍只有 `invalid_input`，不会误带 session-state 字段

## 2026-05-31 增量进展：`doctor.checks[]` 的 daemon 项开始暴露 `state/reasons`

- 这轮继续只动 shell/control-plane 层，不碰浏览器/CDP/定位/交互内核。
- 当前确认到的差异是：
  - `doctor.inventory` 已经有 machine-readable `state/reasons`
  - 但 `doctor.checks[]` 里的 daemon 相关项之前主要还靠 `message` 文本
- 本轮已把这个缺口补上：
  - `rust/src/cli/doctor.rs::Check` 现在支持可选 `state` / `reasons`
  - 只对 daemon 相关 checks 生效：
    - `daemon.session.*`
    - `daemon.incomplete.*`
    - `daemon.cleaned.*`
- 当前覆盖到的 daemon check 语义：
  - healthy session → `state="healthy"`
  - incompatible session → `state="incompatible"`, `reasons=["version_mismatch"]`
  - incomplete session → `state="incomplete"`, `reasons` 复用 `incomplete_daemon_reasons(...)`
  - cleaned sidecars → `state="cleaned"`
- 这轮最重要的判断：
  - 这一步把 shared control-plane truth 又向 doctor 的 check-oriented 视图收口了一层
  - 这样 scripts/agents 不需要一边解析 doctor message，一边再去 inventory/status 找状态真相
- 这轮验证：
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_state_and_reasons_when_present -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime 复核：
    - synthetic mismatch session 下 `openpage doctor --quick` 现在会在 `checks[]` 里直接返回：
      - `state="incompatible"`
      - `reasons=["version_mismatch"]`
      - 完整 `fix`

## 2026-05-31 增量进展：`error.session` 与 `doctor.checks[].session` 已开始 machine-readable 化

- 这轮继续只动 shell/control-plane 层，不碰浏览器/CDP/定位/交互内核。
- 当前确认到的高价值缺口是：
  - direct error 虽然已有 `state/reasons/fix`，但 session 名称仍然常常只在 `message` 里
  - `doctor.checks[]` 的 daemon 项虽然已有 `state/reasons`，但 session 名称仍然常常只在 `id` 里
- 本轮已把这个缺口补上：
  - top-level known session-control error 现在开始暴露 `error.session`
  - `doctor.checks[]` 的 daemon 项现在开始暴露 `session`
- 当前覆盖范围：
  - direct error：仅 known session-control failures
  - doctor checks：仅 `daemon.session.*` / `daemon.incomplete.*` / `daemon.cleaned.*`
- 这轮最重要的判断：
  - 这是 shared control-plane truth 的又一层收口
  - 现在 session 名称不必再从 `message` 或 `daemon.session.<name>` 手工切出来
- 这轮验证：
  - 定向测试通过：
    - `simple_openpage_error_exposes_structured_fix_for_session_guidance`
    - `simple_openpage_error_exposes_state_and_reasons_for_version_mismatch`
    - `response_openpage_error_uses_raw_detail_and_structured_fix`
    - `response_result_preserves_structured_fix_without_double_prefix`
    - `response_result_reconstructed_error_keeps_state_and_reasons_for_incompatible_session`
    - `check_serializes_state_and_reasons_when_present`
    - `daemon_checks_include_machine_readable_state_and_reasons`
    - `openpage_help_marks_tcp_daemon_as_only_active_protocol`
    - `cargo check --manifest-path rust/Cargo.toml`
  - runtime 复核：
    - `openpage title --session missing` 现在返回 `error.session="missing"`
    - synthetic mismatch doctor smoke 现在在 `daemon.session.*` check 上直接返回 `session="session-field-smoke"`
- 附加记录：
  - 本轮一度误用了 `cargo test ... 'session' ...` 这种过宽过滤器，命中了多处仓库内既有 session 测试噪音；这些失败不是本轮变更引入的协议/外壳层回归，后续继续坚持 exact test names 即可。

## 2026-05-31 增量进展：`doctor.inventory` 在无 daemon 目录时也保持稳定对象 shape

- 这轮继续只动 shell/control-plane 层，不碰浏览器/CDP/定位/交互内核。
- 当前确认到的缺口是：
  - `browser list` 一直会返回稳定的 inventory 对象
  - 但 `doctor --quick` 在没有 daemon 目录时此前会返回 `inventory: null`
- 本轮已把这个缺口收掉：
  - `rust/src/cli/doctor.rs::daemon_checks(...)` 在 no-daemon-dir 场景下现在返回 `DaemonInventory::default()`
  - 因此 `doctor --quick` 的 `result.inventory` 现在会稳定保留对象形态和零值 summary
- 这轮最重要的判断：
  - 这是典型的 machine-friendly shape 收口
  - 可以减少 agent/script 对 `null` 分支的额外处理
- 这轮验证：
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_return_empty_inventory_when_daemon_dir_is_missing -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime 复核：
    - `OPENPAGE_HOME=/tmp/openpage-doctor-empty-shape openpage doctor --quick` 现在返回：
      - `inventory.summary.total=0`
      - `sessions=[]`
      - `incomplete=[]`
      - `cleaned=[]`
      - 不再是 `inventory: null`

## 2026-05-31 增量进展：重写竞品借鉴文档，明确可 copy 的外壳层边界

- 本轮目标不是继续改代码，而是把竞品分析文档收成一份后续可直接执行的借鉴清单。
- 当前重新核实后的判断：
  - `agent-browser` 最值得借的是 `connection`、`doctor`、`output`、top-level error shell、`skill-data/core` 文档组织。
  - 明确不要借 `cli/src/native/*`、`chat.rs`、`packages/dashboard/*`。
- 额外核实到一个方法论事实：
  - 本地 `.codegraph/codegraph.db` 只覆盖当前 OpenPage 的部分文件，不覆盖参考项目本身。
  - 且当前 codegraph 对部分 OpenPage 文件存在索引滞后迹象，因此这轮竞品结论以真实源码文件为准，codegraph 只作为当前仓库落点辅助。
- 产物：
  - 根目录 `竞品文档-考虑借鉴的部分v1.md` 已重写，补齐：
    - 借鉴边界表
    - 直接可 copy 候选表
    - 竞品文件 → OpenPage 文件映射表
    - copy 顺序与硬约束
- 这轮没有引入任何运行时代码变更，只更新文档与跟踪文件。

## 2026-06-01 增量进展：收尾 `doctor.rs` 的 browser-path 机器可读字段

- 这轮继续只动 shell/control-plane 层，不碰浏览器/CDP/定位/交互内核。
- 当前收掉的是上一轮被中断的半成品 patch：
  - `rust/src/cli/doctor.rs` 的 `Check` 已补完 `browser_path / resolved_path / suggested_path` 字段初始化
  - browser 相关 checks 现在可以稳定序列化这些字段，而不会让当前源码树卡在 compile error
- 当前已接入的 browser-path 字段语义：
  - `browser.config` / `browser.executable` → `browser_path`
  - `browser.executable` → `resolved_path`（当配置的可执行路径成功解析时）
  - `browser.executable` / `browser.executable.hint` → `suggested_path`（当 doctor 为缺失别名例如 `chrome` 找到本机可用候选时）
- 这轮最重要的判断：
  - 这是对竞品 `doctor` 外壳层的继续借鉴，但仍然没有引入任何浏览器 runtime / CDP / locator / interaction 内核
  - 机器和 agent 现在不必只靠 message 文本去反解“当前配置的 browser_path 是什么”以及“doctor 建议改成哪条本机绝对路径”
- 这轮验证：
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - 临时 project-local `dp_configs.ini`：
    - `[chromium_options]`
    - `browser_path = chrome`
    - 然后执行 `/Volumes/data0/data4work/2026_5/openpage/rust/target/debug/openpage doctor --quick`
- 运行态证据：
  - 默认 dirty worktree 下：
    - `browser.config.browser_path="/tmp/dp-browser"`
    - `browser.executable.browser_path="/tmp/dp-browser"`
  - 带 `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 时：
    - `browser.executable.resolved_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
  - 临时 `browser_path=chrome` 场景下：
    - `browser.executable.suggested_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
    - `browser.executable.hint.suggested_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
- git-safety 约束仍然成立：
  - `git add -p rust/src/cli/oneshot.rs` 仍活跃
  - 本轮仍未触碰 `rust/src/cli/oneshot.rs`

## 2026-06-04 增量进展：direct CLI daemon error 的结构化上下文保真继续收紧

- 这轮继续只动 shell/control-plane 层，重点落点是：
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/oneshot.rs`
- 当前收掉的缺口是：
  - `response_result(...)` 之前只用 `kind / message / fix` 重建 `OpenPageError`
  - daemon 已经给出的 `session / state / reasons / retryable / suggested_action` 在 direct CLI error round trip 上仍有丢失风险
- 本轮已完成：
  - 新增 `openpage_error_from_response_error(...)`
  - 新增 `openpage_error_from_structured_context(...)`
  - `oneshot.rs::response_result(...)` 现在直接消费完整 `ResponseError`
  - 对已知 session-control / daemon-transient 场景，会优先用结构化字段合成 canonical message，而不是只依赖自由文本
  - `openpage_error_fix(...)` 现在也覆盖 canonical session-state message，因此 synthetic `inactive / incomplete / incompatible` 场景的 `fix` 不会在 round trip 中丢掉
- 这轮最重要的判断：
  - direct CLI error 现在对 daemon 结构化字段的保真更强了
  - 这一步依然没有引入竞品 browser/CDP/locator/interaction 内核
- 这轮验证：
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_session_state_when_message_is_generic -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_transient_fields_when_message_is_generic -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_incompatible_state_when_message_is_generic -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_transient_retry -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_generic_incompatible_state -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - `rust/target/debug/openpage serve --stdio`
  - `rust/target/debug/openpage page url`
- 运行态 / guardrail 结论：
  - `cargo check` 当前通过
  - removed `serve --stdio` 继续返回 `error.kind="invalid_input"`
  - removed `page url` 继续返回 `error.kind="invalid_input"`

## Competitor-doc resync (2026-06-04, machine-truth refresh only)

- Intent:
  - finish the competitor-borrow document as a durable reference, without touching runtime code
  - replace stale 2026-05-31 machine snapshots with current 2026-06-04 runtime truth
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `rust/target/debug/openpage serve --stdio`
  - `rust/target/debug/openpage page url`
  - `rust/target/debug/openpage doctor --quick`
  - `rust/target/debug/openpage browser list`
- Observed truth:
  - TCP daemon remains the only active protocol
  - `serve --stdio` still fails with top-level `error.kind="invalid_input"`
  - `page` is no longer an active top-level subcommand; `page url` now fails at command parsing with `error.kind="invalid_input"`
  - `browser list` currently reports `healthy=0 / incompatible=0 / incomplete=0 / cleaned=18 / total=18`
  - `doctor --quick` currently reports `pass=5 / warn=18 / fail=0 / info=2 / total=25`
  - current browser config resolves to `browser_path=<default>` and `browser.executable` is now informational, not failing
- Files synced:
  - `竞品文档-考虑借鉴的部分v1.md`
  - `task_plan.md`
  - `notes.md`
  - `claude-progress.txt`
- Interpretation:
  - the borrow boundary is unchanged: borrow shell/control-plane patterns, do not borrow competitor runtime internals
  - the document is now aligned to current local truth instead of the older `/tmp/dp-browser` machine snapshot

## Competitor-doc deepening (2026-06-04, function-level migration matrix)

- Intent:
  - turn the competitor-borrow document from a file-level recommendation into a function-level migration checklist
  - make future copy/micro-tuning work executable instead of interpretive
- Evidence inspected:
  - `参考项目/agent-browser-main/cli/src/connection.rs`
  - `参考项目/agent-browser-main/cli/src/doctor/mod.rs`
  - `参考项目/agent-browser-main/cli/src/doctor/launch.rs`
  - `参考项目/agent-browser-main/cli/src/output.rs`
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/doctor.rs`
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/mod.rs`
  - `rust/src/cli/oneshot.rs`
- Main conclusions:
  - `connection.rs` is still the highest-value borrow point, but OpenPage must not regress from its richer `sessions/incomplete/cleaned` inventory model
  - `doctor.rs` is no longer the main gap; OpenPage already exceeds the competitor in machine-readable payload richness
  - `output.rs` and top-level error shell are already being systemically absorbed via `protocol.rs`
  - one worthwhile future tightening point is converting `cleaned.reason: String` toward a more stable taxonomy without reintroducing competitor runtime internals
- Files synced:
  - `竞品文档-考虑借鉴的部分v1.md`
  - `task_plan.md`
  - `notes.md`
  - `claude-progress.txt`

## Cleaned sidecar reason taxonomy (2026-06-04)

- Intent:
  - turn cleaned stale-sidecar reporting into a stable machine-readable taxonomy without breaking existing human-readable summaries
  - keep the change strictly in the TCP daemon control plane
- Code changes:
  - `rust/src/cli/connection.rs`
    - `CleanedDaemonSession` now carries both:
      - `reason` — human-readable summary such as `invalid pid, missing version`
      - `reasons` — stable taxonomy such as `["invalid_pid", "missing_version"]`
    - inventory payload `cleaned[]` now emits `reasons[]`
    - added `CleanedReason` internal enum plus summary/taxonomy helpers
  - `rust/src/cli/doctor.rs`
    - `daemon.cleaned.*` checks now also carry machine-readable `reasons[]`
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml cleaned_reason_taxonomy_is_stable_and_keeps_human_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo build --manifest-path rust/Cargo.toml`
  - synthetic runtime audit with fresh `OPENPAGE_HOME`:
    - `openpage browser list`
    - `openpage doctor --quick`
- Observed truth:
  - `browser list` cleaned entries now expose both `reason` and stable `reasons[]`
  - `doctor --quick` cleaned checks now expose `state="cleaned"` plus stable `reasons[]`
  - no runtime/kernel surfaces were touched; the change stays in shell/control-plane only

## Batch invalid-input shell alignment (2026-06-04)

- Intent:
  - align malformed `batch` input with the same machine-readable JSON shell used by top-level CLI parse failures
  - keep semantic workflow restrictions under `unsupported_operation`
- Code changes:
  - `rust/src/cli/oneshot.rs`
    - added `batch_error_payload(...)`
    - malformed nested batch parse errors now return `error.kind="invalid_input"`
    - invalid stdin JSON for `batch` now also returns `error.kind="invalid_input"`
    - batch workflow restrictions such as `batch cannot execute serve` still return `error.kind="unsupported_operation"`
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_uses_invalid_input_for_nested_parse_errors -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_uses_invalid_input_for_invalid_stdin_json -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_keeps_unsupported_operation_for_batch_restrictions -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage batch "page url"`
    - `printf 'not-json' | rust/target/debug/openpage batch`
- Observed truth:
  - malformed batch parse errors now return `error.kind="invalid_input"`
  - invalid batch stdin JSON now returns `error.kind="invalid_input"`
  - semantic batch restrictions still return `error.kind="unsupported_operation"`

## Invalid-value shell alignment (2026-06-04)

- Intent:
  - keep shrinking places where obvious user-input validation errors accidentally surfaced as `unsupported_operation`
  - preserve `unsupported_operation` only for real workflow/platform restrictions
- Code changes:
  - `rust/src/cli/protocol.rs`
    - a narrow subset of `UnsupportedOperation` details now maps to `error.kind="invalid_input"`
    - top-level JSON shell now uses raw detail text for those `invalid_input` cases instead of the `unsupported operation: ...` prefix
    - `openpage_error_from_kind("invalid_input", ...)` now reconstructs through `UnsupportedOperation` without losing the shell kind on re-serialization
  - `rust/src/cli/oneshot.rs`
    - added round-trip test coverage for daemon `invalid_input` responses
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_invalid_value_unsupported_operation_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_uses_invalid_input_kind_for_invalid_snapshot_mode -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_from_daemon_response -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage zoom in --step 0 --session doc-agent`
- Observed truth:
  - direct invalid value checks such as `zoom in --step 0` now return `error.kind="invalid_input"`
  - daemon-side invalid-input-like `UnsupportedOperation` can now round-trip through the JSON shell without degrading back to `unsupported_operation`
  - real semantic restrictions such as `batch cannot execute serve` still stay `error.kind="unsupported_operation"`

## Daemon-side param validation shell alignment (2026-06-04)

- Intent:
  - continue shrinking obvious input-validation cases that still surfaced as `browser_operation` or `unsupported_operation`
  - keep the scope narrow to clearly-invalid schema/value checks
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `error.kind="invalid_input"` now also covers a narrow set of daemon-side parameter validation details, including:
      - `history index must be >= 1`
      - `select requires one of: ...`
      - `select-range/select-text requires end >= start`
      - `missing param:` / `missing numeric param:` / `missing headers param:`
      - `... must be ...` schema-shape errors from request parsing helpers
    - the JSON shell now emits raw detail text for those `invalid_input` cases rather than `browser operation failed: ...`
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for daemon `invalid_input` response preservation
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_browser_operation_schema_validation_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_uses_invalid_input_kind_for_browser_operation_param_validation -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_for_param_validation_detail -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage history go 0 --session doc-agent`
- Observed truth:
  - daemon-side parameter validation such as `history go 0` now returns `error.kind="invalid_input"`
  - these invalid-input-like daemon errors now round-trip through the shell without degrading back to `browser_operation`
  - true workflow restrictions still stay under their existing kinds

## Range/empty/missing-param invalid-input alignment (2026-06-04)

- Intent:
  - continue shrinking daemon-side validation cases that still looked like runtime failures even though the user input itself was invalid
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `error.kind="invalid_input"` now also covers a slightly broader but still narrow set of daemon-side validation details, including:
      - `history index out of range: ...`
      - `find-in-page text must not be empty`
      - `missing target`
      - `missing string/number/array param: ...`
    - these errors now render as raw invalid-input messages instead of `browser operation failed: ...`
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for `invalid_input` details such as missing required string params
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_find_in_page_empty_text_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_missing_string_param_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_for_missing_string_param_detail -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage history go 999999 --session doc-agent`
    - `rust/target/debug/openpage find-in-page "" --session doc-agent`
- Observed truth:
  - out-of-range history selection now returns `error.kind="invalid_input"`
  - empty find-in-page text now returns `error.kind="invalid_input"`
  - missing-param-like daemon validation can now round-trip through the shell without degrading back to `browser_operation`

## Navigation token invalid-input alignment (2026-06-04)

- Intent:
  - continue shrinking stateful-but-user-supplied invalid token cases that still surfaced as `browser_operation`
- Code changes:
  - `rust/src/cli/protocol.rs`
    - bad navigation token details such as `unknown navigation token: ...` now map to `error.kind="invalid_input"`
    - token/frame mismatch details are now grouped into the same invalid-input bucket
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for daemon `invalid_input` preservation on bad navigation tokens
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_unknown_navigation_token_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_for_unknown_navigation_token -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage wait-for-navigation --token definitely-bad --timeout 1 --session doc-agent`
- Observed truth:
  - bad navigation tokens now return `error.kind="invalid_input"`
  - these token-validation errors no longer degrade into `browser_operation` in the JSON shell

## Invalid-input contract hardening (2026-06-04)

- Intent:
  - harden the current invalid-input shell boundary so later edits do not silently drift kinds back toward `browser_operation` or `unsupported_operation`
- Code changes:
  - `rust/src/cli/protocol.rs`
    - added table-driven contract coverage for the known invalid-input detail taxonomy
    - added explicit negative coverage proving that runtime/state cases such as `unknown target: ...` and real restrictions such as `downloads open is unsupported on this platform` stay out of the invalid-input bucket
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml invalid_input_contract_covers_known_detail_taxonomy -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml invalid_input_contract_keeps_runtime_and_restriction_cases_outside_bucket -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the current invalid-input bucket is now protected by a contract test instead of only one-off case tests
  - negative cases still prove the bucket is not swallowing genuine runtime or platform-restriction errors

## Error semantics map artifact (2026-06-04)

- Intent:
  - stop relying on scattered memory of the recent shell/control-plane error-kind tightening work
  - publish a durable map of the current classification boundary
- Artifact created:
  - `错误语义地图-v1.md`
- What it captures:
  - current meaning of `invalid_input`, `unsupported_operation`, `browser_operation`, `daemon_transient`, `invalid_json`, `tcp_error`
  - positive examples already verified at runtime
  - negative examples that should stay outside the invalid-input bucket
  - current contract tests that protect the boundary from drift
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - the shell/control-plane error tightening line now has both implementation/tests and a stable human-readable map

## Control-plane map artifact (2026-06-04)

- Intent:
  - complement the error-kind map with a durable module/ownership map for the active TCP CLI shell
  - make later borrow work stay constrained to control-plane files instead of drifting back into runtime internals
- Artifact created:
  - `控制面地图-v1.md`
- What it captures:
  - current roles of `connection.rs`, `doctor.rs`, `protocol.rs`, `oneshot.rs`, and `mod.rs`
  - current control-plane data flow and ownership boundaries
  - which parts are good borrow targets from `agent-browser`, and which parts should stay out of scope
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `ls -l 控制面地图-v1.md`
  - `rg -n "控制面地图-v1.md" README.md skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - the shell/control-plane line now has both an error-semantics map and a module-ownership map

## Borrow migration checklist artifact (2026-06-05)

- Intent:
  - turn the competitor-borrow guidance into an executable migration checklist instead of a narrative recommendation
  - keep future borrowing constrained to `rust/src/cli/*` and out of runtime internals
- Artifact created:
  - `借鉴迁移清单-v1.md`
- What it captures:
  - priority order for borrow targets
  - per-file/per-function migration suggestions
  - what can be copied directly vs only referenced
  - mandatory edits after copy
  - minimal verification expectations for each borrow step
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `ls -l 借鉴迁移清单-v1.md`
  - `rg -n "借鉴迁移清单-v1.md" README.md skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - the competitor-borrow line now has a concrete execution checklist, not just a direction memo

## Daemon transient retry classifier tightening (2026-06-05)

- Intent:
  - borrow one small but high-value control-plane behavior from the competitor without touching runtime internals
  - make daemon retry classification tolerate more startup/restart-adjacent malformed-response cases
- Code changes:
  - `rust/src/cli/connection.rs`
    - `is_transient_error(...)` now also treats these as transient:
      - `EOF while parsing a value`
      - `expected value at line 1 column 0`
      - `line 1 column 0`
      - `Connection aborted`
      - `os error 2`
    - added retry coverage for EOF-like and empty-JSON-like serialization failures
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml send_request_with_retry_retries_after_eof_like_serialization_error -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml send_request_with_retry_retries_after_empty_json_like_serialization_error -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - retry classification is now closer to the competitor's mature shell behavior
  - the change stays fully inside `rust/src/cli/connection.rs`
  - no TCP/runtime/protocol boundary changed

## Existing daemon reuse stability recheck (2026-06-05)

- Intent:
  - borrow the competitor's ready-recheck discipline before reusing an existing daemon
  - reduce the chance of reusing a daemon that is already in the middle of shutting down
- Code changes:
  - `rust/src/cli/connection.rs`
    - `existing_daemon_action_with_retry(...)` now waits briefly and re-checks readiness before returning `Reuse`
    - added `READY_RECHECK_DELAY_MS`
    - if the daemon disappears during that short window, the flow falls back to the normal alive/unready handling path instead of reusing it
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_does_not_reuse_daemon_that_drops_during_recheck_window -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_reuses_ready_matching_daemon -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - stable ready daemons are still reused
  - daemons that vanish during the short recheck window are no longer eagerly reused
  - the change stays inside the TCP control plane and does not touch runtime internals

## Failed startup sidecar cleanup tightening (2026-06-05)

- Intent:
  - tighten the daemon startup failure path so an early child exit does not leave stale sidecars behind until a later inventory sweep
  - keep persisted daemon logs readable while immediately cleaning `.port/.pid/.version`
- Code changes:
  - `rust/src/cli/connection.rs`
    - extracted `startup_exit_error(...)`
    - early daemon startup exits now route through that helper
    - the helper immediately cleans sidecars before building the returned IO error
    - if the daemon exits right after the polling loop ends, the final `try_wait()` now still takes the same cleanup path
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml startup_exit_error_cleans_sidecars_and_surfaces_log_content -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml startup_exit_error_cleans_sidecars_without_log_content -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - failed startup now eagerly removes stale `.port/.pid/.version` sidecars
  - startup error messages still preserve log content when available
  - `.log` files are intentionally left intact for later `browser logs` inspection

## Startup-timeout cleanup tightening (2026-06-05)

- Intent:
  - make the startup-timeout path behave more like a failed bootstrap cleanup path instead of leaving a detached startup daemon around after timeout
  - keep the persisted log file as the surviving diagnostic artifact
- Code changes:
  - `rust/src/cli/connection.rs`
    - on startup timeout, `ensure_daemon(...)` now best-effort kills the still-running child handle and waits for it
    - extracted `startup_timeout_error(...)`
    - timeout errors now also clean `.port/.pid/.version` immediately before returning
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml startup_timeout_error_cleans_sidecars_and_preserves_log_path_in_message -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml startup_exit_error_cleans_sidecars_and_surfaces_log_content -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - timeout failures now stop behaving like sidecar-leaking startup attempts
  - the `.log` path is still preserved in the returned error message
  - the surviving diagnostic artifact after timeout is the log file, not stale sidecars

## Startup failure direct-error context preservation (2026-06-05)

- Intent:
  - close a shell-level gap where startup failures kept `error.kind="io"` but lost machine-readable recovery fields
  - keep the kind stable while surfacing `session` and `fix`
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `openpage_error_context(...)` now recognizes startup failure IO details of the form:
      - `daemon for session '...' exited during startup`
      - `daemon for session '...' failed to become ready during startup`
    - those payloads now surface:
      - `error.session`
      - `error.fix`
    - fix points callers at `openpage browser logs --session ... --tail 20`
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_session_and_fix_for_startup_timeout_io -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_exposes_session_and_fix_for_startup_exit_io -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - startup failures still keep `error.kind="io"`
  - callers now also get machine-readable recovery context instead of only a free-form message

## Generic startup-io round-trip preservation (2026-06-05)

- Intent:
  - close the remaining round-trip gap where a daemon could send a generic startup `io` message plus structured `session/fix`, and `response_result(...)` would otherwise degrade that back to a plain free-form IO string
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `openpage_error_from_structured_context(...)` now canonicalizes generic startup `io` errors into a session-tagged startup-failure form when the structured fix matches the startup-log recovery action
    - `startup_failure_session_from_detail(...)` now also recognizes the canonical `startup failure:` form
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for generic startup `io` daemon responses carrying structured `session/fix`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_generic_startup_failure_io -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_session_and_fix_for_generic_startup_failure_io -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - generic startup `io` daemon responses no longer lose `session/fix` when reconstructed through `response_result(...)`
  - the shell kind still remains `io`

## Generic session-io round-trip preservation (2026-06-05)

- Intent:
  - extend the same round-trip discipline beyond startup-specific IO failures
  - ensure a daemon response carrying `kind="io"` plus structured `session` does not degrade back into an unstructured plain IO string
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `openpage_error_from_structured_context(...)` now canonicalizes generic structured session-scoped IO into `daemon for session '...': ...`
    - `openpage_error_context(...)` now extracts `session` from that generic canonical IO form as well
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for generic `io` daemon responses carrying `session` and no `fix`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_generic_session_io -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_session_for_generic_io_without_fix -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - generic `io` daemon responses now preserve `error.session` across reconstruction
  - this stays a shell/control-plane change only; `error.kind` remains `io`

## Cleaned inventory log diagnostics (2026-06-05)

- Intent:
  - make stale-sidecar cleanup more diagnosable without turning cleaned residue back into active state
  - surface whether a cleaned session still has a persisted daemon log worth inspecting
- Code changes:
  - `rust/src/cli/connection.rs`
    - `CleanedDaemonSession` now also carries:
      - `log_path`
      - `log_exists`
    - `daemon_inventory_payload_json(...)` now emits those fields under `cleaned[]`
  - `rust/src/cli/doctor.rs`
    - cleaned daemon checks now also retain `log_path`
  - `rust/src/cli/oneshot.rs`
    - browser inventory payload tests updated to assert the new cleaned log diagnostics
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `cleaned[]` now tells callers whether a stale cleaned session still has a persisted log
  - the session remains `state="cleaned"`; this is extra diagnostics, not a state reclassification

## Cleaned inventory machine-readable fix (2026-06-05)

- Intent:
  - make cleaned residue actionable for automation instead of only descriptive
  - align cleaned entries with the rest of the control plane, where payloads usually carry a next-step hint
- Code changes:
  - `rust/src/cli/connection.rs`
    - added `cleaned_daemon_fix(...)`
    - `cleaned[]` payload entries now include `fix`
    - when a cleaned log still exists, the fix points to `browser logs --session ... --tail 20`
    - when no log exists, the fix falls back to restarting the session if needed
  - `rust/src/cli/doctor.rs`
    - cleaned daemon checks now also carry `fix`
  - `rust/src/cli/oneshot.rs`
    - browser inventory payload tests updated to assert cleaned fixes
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - cleaned residue is now both diagnosable and actionable
  - this stays a control-plane guidance change; cleaned sessions are still not treated as active

## Doctor cleaned-check log contract alignment (2026-06-05)

- Intent:
  - finish aligning doctor cleaned checks with the cleaned inventory payload shape
  - make stale-log existence machine-readable in doctor output, not just in inventory
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `Check` now also supports `log_exists`
    - cleaned daemon checks now emit both `log_path` and `log_exists`
    - daemon-check shape test now includes a real cleaned fixture and asserts the cleaned fix/log fields
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - doctor cleaned checks now expose the same stale-log existence signal as inventory
  - this improves automation/debuggability without changing any session state classification

## Active/incomplete daemon log contract alignment (2026-06-05)

- Intent:
  - finish the daemon log diagnostics alignment across `browser list`, `browser status`, and `doctor`
  - stop treating `log_exists` as a cleaned-only signal when active/incomplete sessions also have a persisted log truth
- Code changes:
  - `rust/src/cli/connection.rs`
    - added `log_exists` to `DaemonSessionInfo` and `IncompleteDaemonSession`
    - `daemon_status(...)` and `daemon_inventory(...)` now capture log presence for active/incomplete sessions
    - `daemon_inventory_payload_json(...)` now emits `log_exists` for `sessions[]` and `incomplete[]`
  - `rust/src/cli/doctor.rs`
    - daemon-session and incomplete-session checks now emit `log_exists`
  - `rust/src/cli/oneshot.rs`
    - browser inventory tests updated to assert the aligned payload shape
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_incomplete_with_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_daemon_runtime_fields_when_present -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - active, incomplete, and cleaned daemon surfaces now all expose log availability explicitly
  - this is a control-plane shape alignment only; it does not change daemon lifecycle behavior

## Browser logs log-existence contract alignment (2026-06-05)

- Intent:
  - finish the daemon log-shape alignment by making `browser logs` expose the same `log_exists` signal as `browser status` / `browser list` / `doctor`
  - keep backward compatibility for existing callers that still read `exists`
- Code changes:
  - `rust/src/cli/oneshot.rs`
    - `run_browser_logs(...)` now prefers structured `log_exists` from the status payload and only falls back to `Path::exists()` when needed
    - `browser_logs_payload(...)` now emits `log_exists` and keeps `exists` as a compatibility alias
    - added a false-case test so the inactive/no-log shape stays explicit and machine-readable
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_incompatible_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_false_log_exists -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `browser logs` now speaks the same log-availability contract as the rest of the daemon control plane
  - the legacy `exists` field still works, but it is now clearly just an alias for `log_exists`

## Doctor auto-fix contract tightening (2026-06-05)

- Intent:
  - make `summary.fixable_ids` reflect the real scope of `doctor --quick --fix` instead of every check that merely carries manual guidance
  - separate machine-readable auto-fixability from human-readable `fix` text
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added `auto_fixable=true` for checks that `apply_fixes()` can actually repair automatically
    - `summary.fixable` / `summary.fixable_ids` now count only those checks
    - legacy-session residue, incompatible daemon sessions, and incomplete unready daemon sessions are now the explicit auto-fix bucket
    - manual guidance checks such as browser executable / browser launch keep `fix` text but are no longer misclassified as auto-fixable
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_auto_fixable_only_when_present -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `checks[].fix` now clearly means “guidance exists”, while `fixable_ids` means “doctor can do it for you”
  - this removes a shell-level ambiguity that would otherwise mislead automation

## Doctor fixed[] structure alignment (2026-06-05)

- Intent:
  - make `doctor --quick --fix` results machine-readable end-to-end instead of leaving `fixed[]` as free-form strings
  - align applied-fix reporting with `checks[].id` and `summary.fixable_ids`
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added structured `FixedAction` entries with `check_id`, `message`, `auto_fixable`, and optional `session` / `path`
    - `apply_fixes()` now returns structured entries for legacy JSON cleanup, incompatible daemon cleanup, incomplete unready daemon cleanup, and opportunistic stale-sidecar cleanup
    - stale-sidecar cleanup is explicitly represented as `auto_fixable=false` because it happens during inventory scan, not through a directly fixable check
  - tests now assert the new structure instead of only string containment
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml remove_legacy_session_files_deletes_only_json_entries -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_reports_stale_daemon_sidecar_cleanup -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incomplete_unready_daemon_session -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incompatible_daemon_session -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml fixed_action_serializes_machine_fields -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `fixed[]` can now be consumed by scripts without scraping human text
  - `check_id` now closes the loop between checks, fixable summary, and applied-fix reporting

## Doctor --fix post-fix view contract (2026-06-05)

- Intent:
  - remove ambiguity around whether `doctor --quick --fix` returns a pre-fix snapshot or a post-fix snapshot
  - verify that applied-fix reporting and current-state reporting can be consumed together safely
- Code changes:
  - `rust/src/cli/doctor.rs`
    - extracted `doctor_payload(&DoctorArgs)` so the JSON report can be tested directly
    - added a regression test that sets up legacy residue, stale sidecars, an incomplete unready daemon, and an incompatible daemon, then verifies:
      - `fixed[]` reports all applied actions
      - `summary.fixable_ids` is empty after cleanup
      - `inventory.summary` is the post-fix zero-residue view
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `doctor --quick --fix` now has an explicit tested contract: applied fixes are listed in `fixed[]`, while `summary` / `checks` / `inventory` describe the resulting post-fix state

## Doctor fixed[] source/reason taxonomy (2026-06-05)

- Intent:
  - remove the last ambiguity in structured `fixed[]` output so callers no longer infer cleanup provenance from `auto_fixable` plus free-form text
  - make opportunistic inventory cleanup and direct `--fix` actions distinguishable by stable fields
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `FixedAction` now also carries stable `source` and `reason`
    - `source` currently distinguishes `direct_fix` vs `inventory_scan`
    - `reason` currently distinguishes `legacy_session_json`, `incompatible_daemon`, `incomplete_unready_daemon`, and `stale_sidecars`
    - tests updated so the structured applied-fix contract is asserted directly
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml fixed_action_serializes_machine_fields -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml remove_legacy_session_files_deletes_only_json_entries -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_reports_stale_daemon_sidecar_cleanup -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - callers can now tell whether a `fixed[]` entry came from explicit `--fix` work or opportunistic daemon inventory cleanup without scraping `message`

## Doctor checks[] kind field for daemon sessions (2026-06-05)

- Intent:
  - remove one more place where callers had to infer semantics from `category + id`
  - give concrete daemon-session checks a stable, directly filterable shape marker
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `Check` now supports optional `kind`
    - concrete daemon-session checks now emit `kind="daemon_session"` for healthy/incompatible, incomplete, and cleaned daemon session entries
  - tests updated to assert the new field on serialized checks and daemon-check output
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_state_and_reasons_when_present -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - consumers can now filter concrete daemon-session checks directly by `kind` instead of recovering that meaning from string prefixes

## Doctor check kinds for core non-daemon checks (2026-06-05)

- Intent:
  - keep pushing `doctor checks[]` away from string-prefix parsing
  - cover the highest-value non-daemon checks with stable kinds before broadening further
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `env.legacy_sessions` now emits `kind="legacy_sessions"`
    - `browser.config` now emits `kind="browser_config"`
    - `browser.executable` and `browser.executable.hint` now emit `kind="browser_executable"`
    - `browser.launch` now emits `kind="browser_launch"`
  - added focused tests for `environment_checks(...)` and `browser_checks(...)` to assert the new kinds on real generated checks
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml environment_checks_include_legacy_sessions_kind -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_checks_include_stable_kinds_for_core_browser_checks -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_auto_fixable_only_when_present -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - callers can now filter the most important non-daemon checks directly by `kind` instead of depending on `id` naming conventions

## Doctor kind coverage for foundational env/daemon checks (2026-06-05)

- Intent:
  - finish the highest-value baseline kinds so automation can classify the doctor control-plane entry points without parsing `id`
  - keep the change scoped to foundational env/daemon checks only
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `env.openpage_home` now emits `kind="openpage_home"`
    - `env.daemon_dir` and `daemon.dir` now emit `kind="daemon_dir"`
    - `daemon.sessions` now emits `kind="daemon_sessions"`
  - focused tests now assert these kinds on real generated checks
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml environment_checks_include_legacy_sessions_kind -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_return_empty_inventory_when_daemon_dir_is_missing -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the doctor shell contract now covers the main env/daemon/browser entry checks with explicit stable `kind` values instead of relying on `id` naming patterns

## Doctor contract inventory doc (2026-06-05)

- Intent:
  - freeze the current doctor JSON shell contract into one place so later tightening work has a stable baseline
  - reduce future context rebuilding when iterating on `doctor` machine fields
- Docs added:
  - `doctor-契约盘点-v1.md`
    - top-level shape
    - `summary` semantics
    - `checks[]` fields and stable `kind` taxonomy
    - `fixed[]` fields plus `source` / `reason`
    - `inventory` shape
    - post-fix view semantics
    - recommended parse order for automation
  - `README.md` now links to the contract inventory doc
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the current doctor shell contract is now documented as an explicit artifact instead of being spread across code, tests, and changelog notes

## Doctor kind coverage source-level guard (2026-06-05)

- Intent:
  - turn the current manual kind-coverage audit into an enforceable regression guard
  - prevent future production `doctor` checks from silently landing without `kind`
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added a source-level test that scans the production segment of `doctor.rs` and asserts every `Check::new(...)` block includes `.with_kind(...)`
- Docs synced:
  - `doctor-契约盘点-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml production_check_builders_all_include_kind -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - doctor kind coverage is no longer just convention; it is now guarded by a regression test

## Doctor contract closure conclusion + documented kind baseline (2026-06-05)

- Intent:
  - finish the current doctor-contract tightening stage with an explicit stabilized-vs-unpromised conclusion
  - guard not only that production checks have `kind`, but that the current stable kind baseline matches documentation
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added `production_check_kinds_match_documented_stable_set`
    - this test now guards the current production kind baseline:
      - `openpage_home`
      - `daemon_dir`
      - `legacy_sessions`
      - `daemon_sessions`
      - `daemon_session`
      - `browser_config`
      - `browser_executable`
      - `browser_launch`
- Docs added/updated:
  - `doctor-契约收口结论-v1.md`
    - stable fields
    - stable kind baseline
    - stable fixed/source/reason semantics
    - post-fix view semantics
    - explicitly unpromised areas
  - `doctor-契约盘点-v1.md`
    - now notes that production `Check::new(...)` coverage and kind baseline are source-level guarded
  - `README.md`
    - now links to the closure/conclusion doc
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml production_check_kinds_match_documented_stable_set -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the current doctor shell contract stage now has both a baseline contract inventory and a closure document stating what is stable vs explicitly unpromised

## Browser daemon payload kind alignment (2026-06-05)

- Intent:
  - align `browser status`, `browser logs`, and `browser list` with the doctor daemon-session kind taxonomy
  - let callers filter daemon payloads across these browser surfaces without inferring from field combinations
- Code changes:
  - `rust/src/cli/connection.rs`
    - `daemon_inventory_payload_json(...)` now emits `kind="daemon_session"` for `sessions[]`, `incomplete[]`, and `cleaned[]`
    - `daemon_status_payload_json(...)` now emits `kind="daemon_session"` on the top-level payload and nested `incomplete` payloads
  - `rust/src/cli/oneshot.rs`
    - `browser_logs_payload(...)` now backfills `kind="daemon_session"` when older callers/tests pass a status payload without it
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_incomplete_with_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_incompatible_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the daemon-session kind taxonomy now spans both doctor checks and browser daemon control payloads

## Browser daemon contract inventory doc (2026-06-05)

- Intent:
  - give `browser status` / `browser logs` / `browser list` the same explicit contract treatment that `doctor` now has
  - freeze the current daemon-session field alignment in one place for upper-layer consumers
- Docs added/updated:
  - `browser-daemon-契约盘点-v1.md`
    - stable fields for list/status/logs
    - stable state set
    - stable reasons
    - `kind="daemon_session"` alignment
    - compatible alias notes for `path` / `exists`
    - unpromised ranges
  - `README.md` now links to the browser daemon contract doc
- Code changes:
  - `rust/src/cli/oneshot.rs`
    - added `browser_logs_payload_backfills_daemon_session_kind_when_missing`
    - this locks in backward-compatible `kind` backfill for old status shapes passed into browser-logs payload composition
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_backfills_daemon_session_kind_when_missing -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - browser daemon shell outputs now have a dedicated contract artifact instead of being implied through scattered tests and README bullets

## Control-plane contract consistency sweep (2026-06-05)

- Intent:
  - do a closeout audit across the newly added control-plane docs instead of continuing random field additions
  - confirm that README, doctor contract docs, and browser daemon contract docs describe the same stable shell contract
- Scope checked:
  - `README.md`
  - `doctor-契约盘点-v1.md`
  - `doctor-契约收口结论-v1.md`
  - `browser-daemon-契约盘点-v1.md`
  - `控制面总览-契约关系-v1.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- Observed consistency:
  - `fixable_ids` is consistently documented as narrower than `checks[].fix`
  - `fixed[]` is consistently documented as applied-actions history, while `summary / checks / inventory` are post-fix view
  - `kind=\"daemon_session\"` is consistently documented across `doctor`, `browser list`, `browser status`, and `browser logs`
  - `log_path / log_exists` are consistently preferred over compatibility aliases `path / exists`
  - `source / reason` taxonomies for `doctor.fixed[]` are documented consistently
- Minimal doc changes:
  - refreshed `竞品文档-考虑借鉴的部分v1.md` date to `2026-06-05`
  - added an “已吸收进 OpenPage 的借鉴项” table so future borrowing work starts from the current baseline instead of the original gap analysis
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml production_check_kinds_match_documented_stable_set -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_backfills_daemon_session_kind_when_missing -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions -- --nocapture`
- Observed truth:
  - at this stage the control-plane shell contract is not just implemented, but also documented consistently enough to be treated as the current baseline

## Removed-surface parse-error migration fixes (2026-06-05)

- Intent:
  - tighten one remaining direct CLI / batch shell inconsistency instead of only documenting the current state
  - ensure removed legacy surfaces return not just `error.kind=\"invalid_input\"`, but also a stable `error.fix` that points callers at the active TCP-only workflow
- Code changes:
  - `rust/src/cli/protocol.rs`
    - added `known_invalid_input_fix(...)`
    - centralizes migration guidance for currently removed surfaces:
      - removed `page ...`
      - removed `serve --stdio`
  - `rust/src/cli/mod.rs`
    - `clap_error_payload(...)` now emits `simple_error_with_fix(...)` for parse failures
    - known removed-surface parse failures now carry `error.fix`
  - `rust/src/cli/oneshot.rs`
    - `batch_error_payload(...)` now reuses the same removed-surface migration helper for nested parse failures
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml parse_errors_render_machine_friendly_json_shell -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml removed_stdio_parse_errors_expose_migration_fix -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_ -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - removed-surface direct parse errors and nested batch parse errors now share the same machine-readable migration guidance instead of making callers scrape legacy-surface wording from free-form clap text

## Batch workflow-restriction fixes (2026-06-05)

- Intent:
  - continue shell/control-plane error convergence by covering a remaining `unsupported_operation` gap
  - expose stable `error.fix` for known batch workflow restrictions instead of returning kind+message only
- Code changes:
  - `rust/src/cli/protocol.rs`
    - added `known_unsupported_operation_fix(...)`
    - wired `UnsupportedOperation` into `openpage_error_context(...)`
    - known workflow restrictions now emit `fix` without affecting unrelated platform unsupported cases
  - `rust/src/cli/oneshot.rs`
    - batch restriction payload tests now assert structured `fix`
- Covered restriction cases:
  - `batch cannot execute \`serve\`; use top-level \`serve\` separately`
  - `batch cannot execute \`doctor\`; run \`openpage doctor\` separately`
  - `batch cannot execute nested batch commands`
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
  - `控制面地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_fix_for_batch_workflow_restriction -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_keeps_fix_absent_for_platform_unsupported_case -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_ -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- batch "serve --session smoke-bad"`
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- batch "batch \"title\""`
- Observed truth:
  - batch workflow restrictions now participate in the same machine-readable `error.fix` discipline as removed legacy surfaces and daemon control-plane recovery errors

## Session-local unsupported-operation fix: tab reopen prereq (2026-06-05)

- Intent:
  - keep extending the structured `error.fix` discipline only where there is a clear, concrete recovery path
  - cover one remaining session-local restriction: `tab reopen` with no recorded recently closed tab
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `known_unsupported_operation_fix(...)` now also covers:
      - `no recently closed tab recorded for this session`
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_fix_for_missing_recently_closed_tab_stack -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_keeps_fix_absent_for_platform_unsupported_case -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Evidence boundary:
  - this pass intentionally uses protocol-level verification, not runtime smoke, because the error depends on session-local recently-closed-tab history and would require a live browser/session setup to reproduce deterministically
- Observed truth:
  - `unsupported_operation` now covers not only top-level batch workflow restrictions, but also one session-local prerequisite failure with a stable next-step `fix`

## drag-in missing-payload fix alignment (2026-06-05)

- Intent:
  - extend the structured `error.fix` discipline to one clearer `invalid_input` case that exists in both direct CLI validation and daemon-side validation
  - keep direct CLI and daemon round-trip behavior aligned for `drag-in` payload validation
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `known_invalid_input_fix(...)` now covers both:
      - `drag-in requires --text or --files`
      - `drag-in requires text or files`
    - `UnsupportedOperation` error-context fix lookup now also falls back to `known_invalid_input_fix(...)`
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_fix_for_drag_in_missing_payload -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_uses_invalid_input_fix_for_daemon_drag_in_validation -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- drag-in '#dropzone' --session smoke-drag-fix`
- Observed truth:
  - both the direct CLI validation string and the daemon-side validation string for `drag-in` now converge on the same `error.kind=\"invalid_input\"` plus the same structured `error.fix`

## Enum-style invalid-input fixes: snapshot/select (2026-06-05)

- Intent:
  - keep extending the `error.fix` allowlist to invalid-input cases where the valid choices are explicit and finite
  - cover one `BrowserOperation` variant and two `UnsupportedOperation` variants with the same strategy
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `known_invalid_input_fix(...)` now covers:
      - `select requires one of: text, value, index`
      - `unsupported snapshot mode: ...`
      - `unsupported snapshot format: ...`
    - `BrowserOperation` context lookup now also falls back to `known_invalid_input_fix(...)` when no session-state fix already applies
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_uses_invalid_input_kind_for_invalid_snapshot_mode -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_browser_operation_schema_validation_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_fix_for_invalid_snapshot_format -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Evidence boundary:
  - no runtime smoke for this family in this pass because these cases are daemon-side/session-backed validation paths rather than top-level parse-only failures
- Observed truth:
  - enum-style invalid-input cases now expose machine-readable valid-choice guidance even when they originate from different OpenPage error variants

## Missing/shape invalid-input fixes (2026-06-05)

- Intent:
  - close one more clear `invalid_input` family where the missing field or payload shape already implies a concrete recovery action
  - keep this pass limited to shell/control-plane validation semantics, without touching browser/runtime internals
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `known_invalid_input_fix(...)` now also covers:
      - `missing target`
      - `missing string param: ...`
      - `missing number param: ...`
      - `missing numeric param: ...`
      - `missing array param: ...`
      - `missing headers param: ...`
      - `missing param: ...`
      - `... must be a string or string array`
      - `... must be an integer or integer array`
      - `... must be an object`
      - `array param must contain only strings: ...`
      - `array param must contain only integers: ...`
      - `header values must be strings: ...`
    - existing `BrowserOperation` invalid-input fallback now reuses those fixes automatically during shell payload shaping
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_missing_string_param_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_fix_for_missing_headers_param -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_exposes_fix_for_object_shape_validation -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - missing-param and payload-shape validation now return not only `error.kind="invalid_input"` but also stable recovery guidance for the covered family

## CLI dogfooding follow-through (2026-06-06)

- Goal:
  - validate the highest-value CLI dogfooding recommendation with real installed-binary usage
- Completed:
  - added `debugger_source` coverage in `rust/src/config.rs`
  - added a narrow runtime launch policy in `rust/src/cli/serve.rs`:
    - built-in default debugger endpoint
    - no explicit CLI `--port`
    - switch launch to dynamic local debugging port allocation
    - keep session-scoped persistent user-data-dir behavior
  - rebuilt and reinstalled the local CLI to `/tmp/openpage-cli-eval/bin/openpage`
  - reran real dogfooding against the installed binary
- Verification:
  - two default `browser start` commands in the same fresh `OPENPAGE_HOME` both succeeded and stayed healthy
  - both sessions were usable via `title`
  - `browser list` and `doctor --quick` both reported `healthy=2`, `incomplete=0`
  - same-session `localStorage` survived `browser stop` plus `browser start` without passing `--port 0`
- Updated ranking:
  1. startup / dynamic debug-port policy with persistent profiles
     - now validated and implemented on the daemon-backed default path
  2. runtime health truthfulness
     - still required because it is the guardrail when launches or targets degrade
  3. batch readability / human scanning
     - still worthwhile, but lower leverage than fixing launch semantics first
- Remaining UX work:
  - explain this default behavior more explicitly in help/docs/doctor output so users are not forced to infer why starts now land on different ports

## CLI dogfooding: next project ranking after longer interactive use (2026-06-06)

- New evidence from installed-binary interactive flows:
  - `browser start --session flow --headless https://www.wikipedia.org` hung while `browser list` / `browser status` classified the session as `daemon_unresponsive`
  - `browser start --session nav --headless` succeeded, but `goto --session nav https://www.wikipedia.org` hung the same way
  - while that navigation was in flight, `browser stop --session nav` also hung
  - `goto --help` advertises a non-blocking path without `--wait`, but `run_goto(...)` currently blocks on synchronous `webpage.get`
  - `batch` ran correctly for a short flow (`snapshot` -> `click @e1` -> `wait-for-navigation` -> `title`), so its main problem is transcript readability, not correctness
- Updated ranking:
  1. navigation request lifecycle / daemon responsiveness
     - decouple long-running navigation from daemon liveness
     - distinguish busy vs unhealthy in session health reporting
     - reconcile `goto`/`browser start <url>` behavior with their advertised async wait contract
     - expose bounded timeout/retry control at the CLI surface instead of forcing users into the built-in ~2 minute retry window
  2. unresponsive-session recovery UX
     - add an explicit force-stop / kill / cleanup path when a session cannot service RPCs
  3. batch transcript readability
     - keep NDJSON if needed, but add command indexing/labels or a friendlier summary shape for human scanning
  4. error rendering hygiene
     - dedupe repeated `error.fix` strings in overlapping session-target failure paths
- Current recommendation:
  - treat project (1) as the next major optimization stream
  - treat project (2) as the operational escape hatch that should likely ship with or immediately after (1)

## CLI dogfooding: implementation-stream refinement from root-cause analysis (2026-06-06)

- Root cause now verified:
  - the daemon currently serves TCP clients serially in one accept loop and runs each request synchronously
  - a long `goto` / `webpage.get` request blocks fresh status/list/stop connections from being serviced
- Refined implementation streams:
  1. daemon request isolation / concurrency model
     - separate connection acceptance from request execution
     - ensure status/list/shutdown can still be served while one navigation request is in flight
     - likely touchpoints:
       - `rust/src/cli/serve.rs`
       - request/response ownership model around `ServeRuntime`
  2. busy-vs-broken health semantics
     - add an explicit "busy" or equivalent runtime state instead of collapsing everything into `daemon_unresponsive`
     - likely touchpoints:
       - `rust/src/cli/connection.rs`
       - `rust/src/cli/doctor.rs`
  3. explicit operator force-stop path
     - expose the already-existing forced shutdown fallback as a first-class CLI choice
     - likely touchpoints:
       - `rust/src/cli/args.rs`
       - `rust/src/cli/oneshot.rs`
       - `rust/src/cli/connection.rs`
  4. async navigation contract cleanup
     - make `goto` / `browser start <url>` either truly non-blocking without `--wait`, or rewrite help/output to stop promising that behavior
     - likely touchpoints:
       - `rust/src/cli/oneshot.rs`
       - `rust/src/cli/args.rs`
  5. timeout/retry ergonomics
     - expose per-command timeout/retry control so users can bound long navigations without editing config
     - this is lower priority than request isolation because timeout tuning alone does not fix the frozen control plane

## CLI dogfooding: refined sequencing after type-feasibility check (2026-06-06)

- New feasibility evidence:
  - compile-time scratch check proved:
    - `openpage_rs::browser::Browser: Send + Sync`
    - `openpage_rs::page::Page: Send + Sync`
    - `openpage_rs::webpage::WebPage: Send + Sync`
  - so a background-worker design is not blocked by the core wrapper types
- Sequencing recommendation:
  1. busy-state instrumentation + state taxonomy
     - smallest high-value change
     - fixes misleading `daemon_unresponsive` reports first
  2. operator force-stop / immediate kill surface
     - smallest operational safety improvement
     - should not depend on the larger async-navigation rewrite
  3. true non-blocking navigation jobs
     - align `goto` / `browser start <url>` behavior with advertised follow-up waits
     - likely implemented as background navigation work plus per-session/job state
  4. broader daemon accept-loop / request scheduling refactor
     - still potentially valuable, but should come after the narrower session-job model is proven or rejected
- Why this order changed:
  - the current highest pain is not merely "one request at a time"; it is "the CLI cannot explain, classify, or interrupt that state cleanly"
  - those operator-facing failures can be improved before committing to the largest possible daemon rewrite

- Scope reduction found for project (3):
  - existing `navigation_token` + `wait-for-navigation` plumbing means async navigation does not need a new public protocol
  - the work is mainly:
    - background execution of navigation
    - busy-state ownership while that navigation is in flight
    - aligning `goto` / `browser start <url>` to return early with the existing follow-up command shape

- Additional narrowing found for project (1):
  - the first valuable slice does not need full daemon concurrency
  - a sidecar-backed busy/activity state can make status truthfulness much better before broader scheduling changes

- Revised effort sizing:
  1. busy/activity sidecar + state taxonomy
     - size: small/medium
     - value: very high
  2. explicit force-stop / immediate kill surface
     - size: medium
     - value: high
     - reason: current forced cleanup kills the daemon pid but can leave the Chrome child orphaned
  3. async navigation for `goto` and `browser start <url>`
     - size: medium
     - value: very high
  4. broader daemon accept-loop / request scheduling refactor
     - size: large
     - value: high, but may become less urgent if (1)-(3) land well

- New operational evidence:
  - busy-session `browser stop` currently takes about `32s` before returning `forced=true`
  - this makes project (2) high urgency even after its size estimate was raised

## CLI dogfooding: PR-shaped roadmap (2026-06-06)

- Immediate track: highest ROI, bounded blast radius
  1. busy/activity sidecar
     - add `{session}.activity` or equivalent
     - surface `busy` state in `browser list`, `browser status`, and `doctor`
     - stop telling users the daemon is unhealthy when it is just occupied
  2. help/output contract cleanup for navigation commands
     - stop promising non-blocking `goto` behavior until the implementation actually returns early
     - same for `browser start <url>` follow-up language
  3. error rendering hygiene
     - dedupe repeated `error.fix` strings in overlapping failure paths

- Next track: operational recovery
  4. explicit force-stop path
     - add a user-visible `--force` or equivalent
     - but only together with browser-child cleanup, otherwise it will leak orphan Chrome processes
  5. browser-child pid visibility
     - likely sidecar or equivalent daemon-visible state
     - required to make forced cleanup complete instead of daemon-only

- Medium track: behavior correction
  6. real async navigation for `goto` and `browser start <url>`
     - return existing `navigation_token`
     - reuse `wait-for-navigation`
     - keep session in `busy` state while work is in flight

- Large track: architecture simplification if earlier tracks are insufficient
  7. daemon accept-loop / request scheduling refactor
     - only needed if sidecar busy-state plus async navigation still leave unacceptable control-plane starvation

## CLI dogfooding: concrete work packages and touchpoints (2026-06-06)

- Work package A: busy/activity state
  - goal:
    - stop misclassifying occupied sessions as `daemon_unresponsive`
  - likely files:
    - `rust/src/cli/serve.rs`
      - write/clear activity sidecar around long-running request execution
    - `rust/src/cli/connection.rs`
      - read activity sidecar
      - introduce `busy` state/reason in inventory + status payloads
    - `rust/src/cli/doctor.rs`
      - report busy separately from incomplete/unhealthy
  - docs/help surfaces that must change together:
    - `browser list`
    - `browser status`
    - `browser logs`
    - `doctor --quick`
    - ordinary command error mapping for session-backed commands such as `title`, `snapshot`, `click`, etc.
  - size:
    - small/medium
  - hidden dependency:
    - likely needs a new explicit `session_busy` or equivalent error contract instead of falling through to generic `daemon_transient`

- Work package B: navigation contract cleanup
  - goal:
    - make help/output stop promising behavior that current code does not implement
  - likely files:
    - `rust/src/cli/args.rs`
      - `browser start` help
      - `goto` help
    - `rust/src/cli/oneshot.rs`
      - follow-up payload wording
    - `README.md`
  - size:
    - small

- Work package C: force-stop foundation
  - goal:
    - make explicit force-stop safe and complete
  - hidden dependency:
    - must be able to terminate Chrome child, not only daemon pid
  - likely files:
    - `rust/src/browser.rs`
      - validate where browser child pid is known and stable
    - `rust/src/cli/connection.rs`
      - extend sidecars / cleanup semantics
    - `rust/src/cli/args.rs`
      - `browser stop --force` or equivalent
    - `rust/src/cli/oneshot.rs`
      - user-visible stop path
  - size:
    - medium

- Work package D: async navigation
  - goal:
    - make `goto` and `browser start <url>` return early with existing navigation tokens
  - hidden dependencies:
    - session busy ownership
    - background job lifecycle
    - state visibility while job is in flight
  - likely files:
    - `rust/src/cli/oneshot.rs`
    - `rust/src/cli/serve.rs`
    - maybe `rust/src/cli/connection.rs` for surfaced state
  - size:
    - medium

- Work package E: broader daemon scheduling refactor
  - goal:
    - allow independent control-plane requests while long work is active
  - likely files:
    - `rust/src/cli/serve.rs`
  - size:
    - large
  - note:
    - defer unless A-D prove insufficient

### Latest progress (2026-06-06)

- [x] Busy Slice A implemented in `connection.rs` and `protocol.rs`
- [x] Focused tests added for:
  - retry exhaustion remap
  - structured busy-state reconstruction
  - shell-layer `response_result(...)` reconstruction
- [x] Installed-binary slow-server repro confirmed:
  - `title` and `snapshot` now return structured busy/incomplete errors
- [x] `--replace` truthfulness slice implemented in `oneshot.rs`
- [x] Installed-binary healthy-session repro confirmed:
  - `browser start --replace` now performs a real restart instead of returning `already_running=true`
  - same-session localStorage/profile continuity is preserved
- [x] Installed-binary busy-session repro narrowed the remaining gap:
  - `--replace` now attempts a real restart
  - but orphan Chrome/profile-lock fallout still blocks recovery under busy forced-stop scenarios
- [x] Installed-binary replace-interruption repro clarified remaining semantic debt:
  - displaced read commands fail as `inactive`
  - the original in-flight navigation can still leak out as generic `daemon_transient`
- [x] Installed-binary profile-lock repro clarified a docs/fix mismatch:
  - `browser_launch` fix text still points at browser-path validation even when the real failure is orphan-Chrome profile locking
- [x] Installed-binary stop-path comparison narrowed Project 2 further:
  - normal `browser stop` is clean
  - orphan Chrome is concentrated in the forced-stop path
- [x] Installed-binary startup-observation recheck did not reveal a stronger replacement for Project 3
- [x] Installed-binary mixed-result batch runs reconfirmed Project 3:
  - no command correlation metadata
  - no explicit `--bail` stopping marker
- [x] Code inspection narrowed Project 2 implementation ownership:
  - graceful stop already reaches `browser.close()`
  - forced path only kills daemon pid sidecars
  - browser child pid exists in runtime objects but is not persisted for forced cleanup
- [x] Code inspection also validated the current first-pass model:
  - daemon-backed CLI sessions plausibly map to one browser process per session for cleanup purposes
- [x] Code inspection narrowed Project 3 implementation ownership:
  - current batch loop just prints each native payload
  - first fix can stay in output shaping/help/docs without changing execution semantics
- [ ] Remaining highest-value work in Phase 5:
  - complete forced-stop browser-child cleanup
  - decide whether busy-session fix text should keep recommending `--replace` alone before that cleanup lands
  - tighten interruption/fix-text semantics around replace-displaced commands
  - only after that, reconsider larger daemon scheduling work

### Project 1 next-slice read

- The current branch closed the biggest truthfulness gap for ordinary busy reads.
- The next contained slice for Project 1 is now clearer:
  - reuse the same central request-remap path to converge displaced in-flight commands on structured inactive state after `--replace` / forced recovery
- This is still smaller and lower risk than jumping straight to:
  - true async navigation jobs
  - daemon scheduling refactor

### Project 3 next-slice read

- The first valuable batch improvement does not need:
  - daemon protocol changes
  - different execution order
  - summary-only output replacing NDJSON
- The smallest credible slice is:
  - add correlation metadata to each emitted line
  - make `--bail` stopping output explicit
