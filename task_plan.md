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
- **本机现状已重新核实**：本机 `~/.openpage/daemon` 的 healthy session 数量是运行时态；2026-05-30 最新一次 `browser list` 实测为 4 个 healthy session（`cli-more-states-2`、`cli-state-queries`、`human-flow`、`smoke-history2`）；当前唯一明确失败点仍是 `rust/configs.ini` 配置的 `browser_path=chrome` 在本机不可解析。
- **doctor 的本机修复提示已补强**：当 `browser_path=chrome` 这类别名在 PATH 中找不到时，`doctor` 现在会额外探测本机常见浏览器落点；当前机器会明确提示 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` 可直接写回 `rust/configs.ini`。
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
- **`connection.rs` 的唯一 daemon 约束又收紧了一步**：`ensure_daemon()` 现在不再只处理“ready 但 version mismatch”的旧进程；如果发现同 session 旧进程仍存活但 TCP 端口迟迟不可用，会先给启动中的 daemon 一个短暂 ready 宽限期，宽限后仍不可用就杀掉旧进程再拉起新的 daemon，避免同 session 留下孤儿进程或并存 daemon。
- **旧 session JSON 残留已彻底从活跃 CLI 面移除**：`rust/src/cli/oneshot.rs` 里的 `session_file()` / 本地 `openpage_home()` 以及 `browser stop` 时顺手删旧 session JSON 的逻辑都已删除，当前 CLI stop 只围绕 TCP sidecar 与 daemon shutdown。
- **本轮运行态再次核实**：2026-05-30 最新一次 `browser list` 实测返回 5 个 healthy sessions（`cli-more-states-2`、`cli-state-queries`、`human-flow`、`smoke-alert`、`smoke-history2`）；`doctor --quick` 当前仍只有 1 个 fail（`browser.executable`）；这个 session 数量是运行时态，不应写死成仓库事实。

## Errors Encountered
- 当前工作树已存在大量未提交变更，因此迁移时必须逐文件审计，避免覆盖已有工作。
- `daemon.shutdown` 初始只修改了 runtime 状态，没有真正退出 TCP accept 循环；现已修正并验证 sidecar 会随优雅退出清理。
- `back/forward` 在第一版 RPC 包装里存在导航完成前就读取 URL 的竞态；现已通过在包装层补 `wait.doc_loaded` 修正并重新验证。
- `click-for-new-tab` smoke 第一轮失败并非协议问题，而是测试脚本仍停留在新 tab 上就去点旧页的上传控件；调整为先 `tab switch` 回原页后，后续 `click-to-upload` / `click-to-download` / `drag-in` 全部验证通过。
- 本轮 `cargo check` 首次被 `rust/src/page.rs` 中一个现有工作树编译错误挡住：`CaptureSnapshot` 返回值从借用结果中 move 出 `String`。已做最小修复为 `.clone()`，恢复可验证状态。
- `batch` 接入为了避免“每条子命令报错后再多打一层总错误 JSON”，把 `rust/src/cli/oneshot.rs` 的返回语义调整为显式 exit code；这是外层 CLI 行为调整，不涉及浏览器/元素/CDP 内核。
- 当前本机 `doctor` 实测表明：`OPENPAGE_HOME=/Users/yuuu/.openpage`，现存多个活跃 daemon session；当前加载到的配置里 `browser_path=chrome`，但本机并不能解析这个可执行文件，因此 `doctor --quick` 已经会直接失败，full `doctor` 也会跳过 live launch 并给出显式修复提示。
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

## Status
**Currently in Phase 7** - 唯一 TCP daemon 路径仍然保持稳定，但这不代表可以停手。当前还在继续收两类尾巴：一类是把会误导后续会话的旧协议残留继续删到只剩归档材料，另一类是继续把竞品里真正有用的非-CDP 外壳设计往 `connection.rs` / `doctor.rs` / 文档层收。2026-05-30 本轮最新核验里：`cargo test --manifest-path rust/Cargo.toml existing_daemon_action_ -- --nocapture` 通过，`cargo check` 通过，`browser list` 最新实测返回 5 个 healthy sessions，`doctor --quick` 当前仍只有 1 个 fail（`browser.executable`）。本轮还额外修掉了一个会伤害“唯一 daemon”约束的点：旧进程活着但 TCP 不可用时，`ensure_daemon()` 现在会在短暂 ready 宽限期后杀掉旧进程再拉起新 daemon，而不是留下同 session 孤儿进程。与此同时，`oneshot.rs` 里最后那点旧 session JSON 清理残留也已经删除。下一步继续沿 `doctor.rs` 的 summary/fix 结构和根目录历史文档误导面做收敛。
