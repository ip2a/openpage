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
- `rust/src/cli/oneshot.rs` 大量命令仍直接走 `load_session()` / `open_page()` / `Browser::connect()`
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
- **旧直连辅助函数已移除**：`do_start_browser()`、`open_page()`、`load_session()`、`save_session()`、`Browser::connect()` 这一批旧 `oneshot` 执行辅助路径已从 `rust/src/cli/oneshot.rs` 清掉；仅保留 `session_file()` 作为 stop 时顺手清理旧遗留文件。
- **daemon 内部已接管 tab/frame 上下文**：`rust/src/cli/serve.rs` 新增 `ServeWebPage`，维护 active tab / active frame；CLI 不再依赖旧 session JSON 存储 target/frame 状态。
- **剩余主任务转向非-CDP 借鉴项**：下一步不再是协议收口，而是继续补 `agent-browser` 风格的外围设计，如 output 治理、daemon inventory、AI 友好输出等。

## Errors Encountered
- 当前工作树已存在大量未提交变更，因此迁移时必须逐文件审计，避免覆盖已有工作。
- `daemon.shutdown` 初始只修改了 runtime 状态，没有真正退出 TCP accept 循环；现已修正并验证 sidecar 会随优雅退出清理。
- `back/forward` 在第一版 RPC 包装里存在导航完成前就读取 URL 的竞态；现已通过在包装层补 `wait.doc_loaded` 修正并重新验证。
- `click-for-new-tab` smoke 第一轮失败并非协议问题，而是测试脚本仍停留在新 tab 上就去点旧页的上传控件；调整为先 `tab switch` 回原页后，后续 `click-to-upload` / `click-to-download` / `drag-in` 全部验证通过。

## Status
**Currently in Phase 6** - 唯一 TCP daemon 路径已经落地并通过 compile + smoke；`oneshot.rs` 旧直连执行路径已移除。下一步继续补非-CDP 借鉴项，并收尾文档、验证与 git checkpoint。
