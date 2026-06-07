## 执行状态

更新时间: 2026-06-01

| 阶段 | 范围 | 状态 | 验证 |
|---|---|---|---|
| 1 | Snapshot v2: 默认短输出、mode/format/raw/depth/selector 参数 | 已完成 | `cargo check`; `cargo test cli::serve::tests`; runtime smoke: `snapshot`, `snapshot --format json --mode semantic --depth 4` |
| 2 | Element Summary: find/find-all 默认不返回 HTML，HTML 显式读取 | 已完成 | `cargo test cli::oneshot::tests`; runtime smoke: `find a` 不返回 `html`，`element-html a` 返回显式 HTML |
| 3 | LocatorChain: 暴露 DP 风格 parent/child/next/prev/before/after 链式定位 | 第一版已完成 | `cargo test locator::tests`; runtime smoke: `locate 'text:"Learn more"'`, `locate '@e2 >> parent'`, `locate '@e2 >> parent >> child a'` |
| 4 | RefRegistry: 从 DOM ref 标记迁移到内部 registry | 已完成 | `cargo check`; runtime smoke: DOM 无 `data-op-ref`、stale ref 语义重解析成功、frame 内 `snapshot/click` 正常 |
| 5 | 错误与连接稳定性: transient 错误结构化、session/daemon 稳定性 | 已完成 | `cargo check`; `cargo test cli::protocol::tests`; `cargo test shutdown_daemon_cleans_stale_sidecars_when_process_is_gone`; runtime smoke: auto-start, `browser stop --all`, 50× `url/title/snapshot` burst |

当前执行备注:
- 已确认当前文档是方案正文，先补充状态区用于持续跟踪。
- 已确认现有协议层有 `OPENPAGE_MAX_OUTPUT_CHARS` 兜底，但默认 `snapshot/find` 的结构仍需要重做。
- 本轮先推进阶段 1 和阶段 2，因为它们直接解决 agent 使用时内容过大、元素信息噪声高的问题。
- 已新增 `snapshot --mode interactive|semantic|all --format text|json --raw --depth --selector` 参数。
- 已将 `find/find-all` 默认返回改为 element summary，不再默认返回 `html`。
- 已新增显式 `element-html <locator>` 命令用于读取元素 outer HTML。
- runtime smoke 暴露 `browser start` 没有把 `--port/--incognito/--mute` 传入 launch 覆盖层；已补齐，避免测试/用户显式端口时仍接管默认 `127.0.0.1:9222`。
- 已新增 `locate <chain...>` 命令，第一版支持 `@ref/css/xpath/text:` 根定位与 `parent/child/prev/next/before/after` 链式步骤。
- Stage 3 smoke 暴露底层 `Element::collect_relative_elements()` 对相对元素数组返回值处理不稳定，`@e2 >> parent` 会报 `relative element markers script did not return an array: null`；已改为 JS 侧返回 JSON 字符串再反序列化，避免 CDP 对数组 remote object 的序列化差异。
- Stage 3 smoke 还暴露 `text:` 根定位会因 `contains(.)` 命中 `<html>` 这类祖先节点；已把 `text=` 语义收敛为“包含该文本的最小元素”，更适合作为 agent 的人类文本定位入口。
- 已完成验证：`cargo check`、`cargo test locator::tests`、`cargo test cli::serve::tests`、`cargo test cli::oneshot::tests` 均通过；隔离 `OPENPAGE_HOME=/tmp/openpage-v1-smoke-stage3b` 的 CLI smoke 覆盖了 snapshot、summary、显式 HTML、text locator、parent/child chain 和 stop。
- Stage 4 第一版已完成：`ServeWebPage` 新增 session 级 `RefRegistry`，`find/find_all/active_element/locate/snapshot` 都会注册或消费内部 ref；`@eN` 解析优先走 registry，而不是把 ref 转成 CSS `[data-op-ref=...]`。
- `snapshot` 不再给页面元素写入 `data-op-ref`；快照中改为输出内部 `ref + _cssPath + _xpath`，服务端注册后再去掉内部字段。runtime smoke 已验证 `element-html a` 返回的真实 DOM 不再带 `data-op-ref`。
- Stage 4 当前验证场景：`snapshot` 生成 `@e1`，随后 `click @e1` 成功跳转到 `https://www.iana.org/help/example-domains`。这证明用户可见 ref 已不依赖 DOM 属性。
- Stage 4 收尾已补齐：
  - `find_ref()` 现在是两阶段解析：先走旧 `css/xpath` hints；失效后再用 `name/text/tag/role` 做语义重解析。重解析成功后会回写新的 locator hints；仍失败时返回明确错误：`ref @eN is stale and could not be re-resolved; run openpage snapshot again`
  - runtime smoke（正向）：本地页面先 `snapshot` 生成 `@e1` 指向 `<button id="old">Do it</button>`，`600ms` 后 DOM 替换成 `<button id="new">Do it</button>`；随后 `click @e1` 仍成功，`html` 返回 `data-clicked="new"`，证明 stale ref 已按语义重绑定到新节点
  - runtime smoke（负向）：`example.com` 上 `snapshot` 出来的 `@e1` 在跳转到 `https://www.iana.org/help/example-domains` 后再次点击，不再暴露底层 `Invalid search result range`，而是返回明确 stale-ref 提示
  - frame 场景 bug 已修：`page::frame_find_all_script()` 之前错误使用 `XPathResult.ORDERED_NODE_ITERATOR_TYPE`，在迭代中 `setAttribute()` 会触发 `InvalidStateError: document has mutated since the result was returned`；现已改成 `ORDERED_NODE_SNAPSHOT_TYPE`
  - runtime smoke（frame）：最小 `iframe srcdoc` 页面上 `frame list` 正常返回 iframe 元信息，`frame switch 1` 后 `snapshot` 能拿到 frame 内 `@e1 button "Inside Button"`，随后 `click @e1` 成功
- Stage 5 已落一部分：`send_request_with_retry()` 在 transient 错误重试耗尽后，不再只返回模糊的 `io`/`browser_operation`，而是统一包装成 `daemon_transient` 语义。协议层现在会输出 `retryable: true` 和 `suggested_action: "retry_same_command"`。
- Stage 5 同时补了协议测试隔离：`cli::protocol` 里依赖 `OPENPAGE_CONTENT_BOUNDARIES` / `OPENPAGE_MAX_OUTPUT_CHARS` 的测试现在串行持有环境锁，避免并行跑时互相污染，回归结果更可信。
- Stage 5 当前已验证：`cargo test send_request_with_retry_returns_structured_daemon_transient_after_exhaustion`、`cargo test simple_openpage_error_exposes_retryable_daemon_transient_fields`、`cargo test cli::protocol::tests`、`cargo test cli::serve::tests`、`cargo test cli::oneshot::tests` 全部通过。
- Stage 5 又推进了一步：`rpc_webpage()` 现在默认走 attach-or-start，会先确保 `webpage.create` 成功，再发业务请求。为了避免误接管本机 `127.0.0.1:9222` 的现有调试浏览器，auto-start 显式传 `port: 0`，强制新浏览器走独立调试端口。
- Stage 5 runtime smoke：在全新 `OPENPAGE_HOME=/tmp/openpage-v1-stage5-auto2` 下直接执行 `openpage title --session auto-started`，无需预先 `browser start`，返回 `{"title": null}`；随后 `browser status --session auto-started` 显示 daemon healthy，最后 `browser stop` 正常清理。
- 新发现并已修复一个真实生命周期根因：`default_launch_options()` 把所有 session 的默认 profile 都写死到 `~/.openpage/profiles/default`，导致第二个 session 启动时被 Chrome `ProcessSingleton` 拒绝，也让 `stop --all` 的双 session 复现前提不可靠。现在只有 built-in default 会按 session 派生到 `~/.openpage/profiles/<session>`；显式 `user_data_dir` / config / env override 仍保持原优先级，不被覆盖。
- 这次新增验证：
  - `cargo test resolved_config_tracks_user_data_dir_source`
  - `cargo test apply_session_default_user_data_dir`
  - `cargo test browser_stop_all_sessions_deduplicates_and_keeps_alive_incomplete_sessions`
  - `cargo test shutdown_daemon_cleans_stale_sidecars_when_process_is_gone`
  - runtime smoke：两个独立 session `healthy` / `broken` 可以同时启动，profile 分别落到 `/tmp/openpage-stage5-repro4/profiles/healthy` 和 `/tmp/openpage-stage5-repro4/profiles/broken`，随后 `browser stop --all` 在约 `0.23s` 内返回 `all_stopped=true`
  - runtime smoke：手工删除 `broken.port` 制造 “alive 但 sidecar 不完整” 场景后，`browser stop --all` 仍在约 `2.57s` 内返回 `all_stopped=true`
  - runtime smoke：50 轮连续 `url/title/snapshot` 轮询后输出 `burst_ok=1`，最后 `browser stop --session burst` 返回 `forced=false`
- Stage 5 已完成；Stage 4 收尾也已完成。当前文档定义的 5 个阶段都已有代码落地和验证证据。

---

• 我会先把你现有的 OpenPage 定位/链式能力再快速过一遍，避免方案只照搬 agent-browser-main。重点看 locator、find/find_all、@ref、snapshot 生成
  和错误流。

• Ran rg -n "snapshot_payload|agent_snapshot_script|web_element_to_json|format_snapshot_text|snapshot_refs|required_locator_string|
  │ wait_for_locator|Resource temporarily unavailable|os error 35|session `.*` is not active|browser_operation" rust/src -g'*.rs'
  └ zsh:1: command not found: .DS_Store
    rust/src/cli/connection.rs:913:        || message.contains("Resource temporarily unavailable")
    … +99 lines (ctrl + t to view transcript)
    rust/src/cli/oneshot.rs:4117:        assert_eq!(payload["error"]["kind"], "browser_operation");
    rust/src/cli/oneshot.rs:4146:        assert_eq!(payload["error"]["kind"], "browser_operation");

• Explored
  └ Search struct Locator|enum Locator|LocatorInput|normalize_locator|parse_ref|find_all|find\(|child|parent|ancestor|descendant|next|prev|
           chain|xpath|css|text: in rust
    Search get_frame_context|get_frame|wait_until|is_displayed|scroll_to|hover|click|input|select|drag|actions|Element in page.rs

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────

• 从搜索结果看，你的差异化能力确实不只是 @ref：底层已经有 DP 风格的 parent/child/prev/next/before/after、局部查找、shadow root、frame
  context、filter_one 这类链式定位能力。方案应该把这些能力作为“定位语言”的核心，而不是只做一个短 snapshot。

• Explored
  └ Read element.rs, session.rs, connection.rs, serve.rs

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────

• 使用 $karpathy-guidelines 后，我的判断是：这次不应该做“小修小补”，应该把 OpenPage 的 agent 操作面重设计成一个清晰的三层体系：observe、
  locate、act。核心不是少返回几个字段，而是让 CLI 返回“agent 可决策的信息”，同时把你已有的 DP 风格链式定位能力变成 OpenPage 的差异化优势。

  目标判断

  当前 OpenPage 的问题不是能力不够，而是 agent 使用面还没有成熟组织起来：

  ┌─────────────────────────┬────────────────────────────────────────────────────────┬─────────────────────────────────────────────────────┐
  │ 问题                    │ 当前表现                                               │ 正确方向                                            │
  ├─────────────────────────┼────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────┤
  │ 快照过重                │ snapshot + text + refs 重复，find 返回 html 很大       │ 默认只返回 agent 可读摘要，raw 另设命令             │
  │ 快照不够“教学”          │ 只列可交互元素，缺少层级/语义/上下文                   │ 学 agent-browser-main：role/name/ref/状态/层级/裁剪 │
  │ 定位能力没有产品化      │ 底层有 parent/child/next/prev/before/after，CLI 还没形 │ 建一个正式的 locator chain DSL                      │
  │                         │ 成统一定位语言                                         │                                                     │
  │ 错误不够可恢复          │ session inactive、os error 35、元素不可见、无效搜索范  │ 错误要分类，返回下一步建议和可机器处理字段          │
  │                         │ 围                                                     │                                                     │
  │ session/daemon 状态不稳 │ 命令可能卡住、后台 serve 堆积                          │ 连接层要有 health check、单飞启动、僵尸清理         │
  └─────────────────────────┴────────────────────────────────────────────────────────┴─────────────────────────────────────────────────────┘

  新设计

  我建议把 CLI 能力重分成这 3 层。

  ┌─────────┬───────────────────────────────────────────┬──────────────────────────────────────┐
  │ 层      │ 命令方向                                  │ 职责                                 │
  ├─────────┼───────────────────────────────────────────┼──────────────────────────────────────┤
  │ Observe │ snapshot, inspect, find, tree             │ 给 agent 看页面，不执行动作          │
  │ Locate  │ locate, locate-chain, count, near, within │ 把人类描述/链式关系变成稳定元素引用  │
  │ Act     │ click, fill, press, select, hover, drag   │ 只对已定位对象执行动作，失败返回原因 │
  └─────────┴───────────────────────────────────────────┴──────────────────────────────────────┘

  这样之后，agent 的正常流程应该是：

  openpage snapshot
  openpage locate text:"随时了解最新动态"
  openpage click @e17
  openpage wait navigation
  openpage snapshot --changed

  而不是现在这样每一步都靠大 JSON 猜。

  Snapshot v2

  这里要认真吸收 agent-browser-main 的优点，但不要照搬。

  agent-browser-main 做得好的地方：

  ┌────────────────────┬───────────────────────────────────┐
  │ 能力               │ 价值                              │
  ├────────────────────┼───────────────────────────────────┤
  │ Accessibility tree │ 不是 DOM dump，而是浏览器语义视图 │
  │ interactive        │ 默认聚焦可操作元素                │
  │ compact            │ 只保留 ref/value 行和祖先上下文   │
  │ depth              │ 限制层级，避免无限树              │
  │ refs               │ 输出短文本，内部保存可点击引用    │
  │ 文本裁剪           │ 每个节点限制长度                  │
  │ max-output         │ 全局兜底截断                      │
  └────────────────────┴───────────────────────────────────┘

  OpenPage 应该升级为：

  openpage snapshot
  openpage snapshot --mode interactive
  openpage snapshot --mode semantic
  openpage snapshot --selector "#content"
  openpage snapshot --depth 4
  openpage snapshot --format text
  openpage snapshot --format json

  默认建议：

  ┌────────────┬──────────────────────────────────────────────────────────────┐
  │ 默认项     │ 建议                                                         │
  ├────────────┼──────────────────────────────────────────────────────────────┤
  │ 默认格式   │ text                                                         │
  │ 默认模式   │ interactive + semantic context                               │
  │ 默认不返回 │ outerHTML / innerHTML / 大 attrs                             │
  │ 默认包含   │ title、url、viewport、focused、interactive_count             │
  │ 每个元素   │ ref、role、name/text、tag、state、short attrs、locator hints │
  │ 长文本     │ 单字段 80 到 120 字符裁剪                                    │
  │ 总输出     │ 默认 max output，例如 20k 字符                               │
  └────────────┴──────────────────────────────────────────────────────────────┘

  输出示例应该类似：

  Page: 百度一下，你就知道
  URL: https://www.baidu.com/

  @e1 textbox "英伟达最新的处理器" id="kw" focused
  @e2 button "百度一下" id="su"
  @e17 link "NVIDIA Grace CPU 和 ARM 架构 | NVIDIA" href="https://www.nvidia.cn/..."
    context: search result, official-like domain nvidia.cn
  @e24 link "随时了解最新动态" href="https://www.nvidia.cn/..."

  关键点：snapshot 是给 agent 学页面结构的，不是给人看 DOM 的。

  不要再默认返回 HTML

  现在 rust/src/cli/serve.rs:2460 的 web_element_to_json() 返回：

  tag, text, attrs, html

  这个设计要拆掉。成熟设计应该是：

  ┌────────────────┬──────────────────┐
  │ 命令           │ 默认返回         │
  ├────────────────┼──────────────────┤
  │ find           │ element summary  │
  │ find --all     │ summary list     │
  │ element html   │ 单独取 HTML      │
  │ element attrs  │ 单独取 attrs     │
  │ element detail │ 明确请求完整信息 │
  └────────────────┴──────────────────┘

  也就是说 find 默认只返回：

  {
    "ref": "e12",
    "tag": "a",
    "role": "link",
    "name": "随时了解最新动态",
    "text": "随时了解最新动态",
    "attrs": {
      "href": "...",
      "id": "..."
    },
    "state": {
      "visible": true,
      "enabled": true,
      "in_viewport": true
    }
  }

  html 必须变成显式命令，不能混在默认查找里。

  定位系统

  这是 OpenPage 要区别于其他 browser agent 的关键。

  你底层已有 DP 风格能力：

  ┌────────────────────────────────┬──────────────────────────┐
  │ 能力                           │ 位置                     │
  ├────────────────────────────────┼──────────────────────────┤
  │ parent/child/children          │ rust/src/element.rs:1074 │
  │ prev/next                      │ rust/src/element.rs:1170 │
  │ before/after                   │ rust/src/element.rs:1270 │
  │ 静态 HTML session 同款链式查找 │ rust/src/session.rs:3187 │
  │ frame/shadow 支持              │ page.rs、element.rs 已有 │
  └────────────────────────────────┴──────────────────────────┘

  这个不应该隐藏。建议做一个正式的 LocatorChain。

  语法可以长这样：

  openpage locate 'text:"官网"'
  openpage locate '@e12 >> parent >> child a'
  openpage locate '@e12 >> next text:"随时了解最新动态"'
  openpage locate 'role:link[name*="NVIDIA"]'
  openpage locate 'within:@e8 text:"随时了解最新动态"'
  openpage locate 'near:text:"官网" role:link'

  内部模型建议是结构化的，不要只靠字符串乱 parse：

  enum LocatorStep {
      Root(Locator),
      Ref(String),
      Parent { locator: Option<Locator>, index: usize },
      Child { locator: Option<Locator>, index: usize },
      Next { locator: Option<Locator>, index: usize },
      Prev { locator: Option<Locator>, index: usize },
      Before { locator: Option<Locator>, index: usize },
      After { locator: Option<Locator>, index: usize },
      Filter(FilterExpr),
  }

  CLI 可以先接受字符串 DSL，但进入 serve/runtime 后必须转成结构化步骤。这样后面才能稳定做错误提示、日志、测试。

  Ref 机制

  当前 @ref 是通过页面里写 data-op-ref，这个能跑，但不够成熟。建议改成：

  ┌──────────┬──────────────────────────────────────────────────────────────────────────────────────────────┐
  │ 层       │ 做法                                                                                         │
  ├──────────┼──────────────────────────────────────────────────────────────────────────────────────────────┤
  │ 内部     │ RefRegistry 保存 ref -> backend node id / object id / frame id / role / name / locator hints │
  │ 页面     │ 不默认污染 DOM                                                                               │
  │ 失效恢复 │ backend node 失效后，用 role/name/locator hints 重新定位                                     │
  │ 输出     │ 只暴露 @e1 这种短 ref                                                                        │
  └──────────┴──────────────────────────────────────────────────────────────────────────────────────────────┘

  agent-browser-main 的 RefMap 思路值得学：ref 是 agent 和浏览器之间的短句柄，不是 DOM 属性本身。

  如果 Chromiumoxide 能稳定拿 backend node id，就优先用 backend node id；拿不到时再用 selector hints，但这是内部解析策略，不是用户可见的“兼
  容 fallback”。

  错误体系

  你现在遇到的错误要分层解决。

  ┌──────────────────────────────────────────────────┬─────────────────────────────────┬───────────────────────────────────────────────────┐
  │ 错误                                             │ 根因方向                        │ 方案                                              │
  ├──────────────────────────────────────────────────┼─────────────────────────────────┼───────────────────────────────────────────────────┤
  │ session is not active                            │ session 生命周期和 CLI 行为割裂 │ 命令默认 attach-or-start；状态错误返回 state/     │
  │                                                  │                                 │ action                                            │
  │ os error 35                                      │ daemon TCP 瞬时读写/资源竞争    │ 连接层重试、请求 idempotency、daemon 单飞启动     │
  │ 命令卡住                                         │ read timeout 过长且无进度       │ 每个 op 有 timeout，超时返回 op/session/elapsed   │
  │ Node is either not visible or not an HTMLElement │ 定位到了不可交互节点            │ click 前做 visible/enabled/rect/scroll 检查，失败 │
  │                                                  │                                 │ 返回候选原因                                      │
  │ Invalid search result range                      │ Chrome find/search 调用边界问题 │ find-in-page 独立修复，不影响 locator 系统        │
  │ 后台 serve 堆积                                  │ stop/idle/watchdog 不完整       │ session registry + stale daemon cleanup           │
  └──────────────────────────────────────────────────┴─────────────────────────────────┴───────────────────────────────────────────────────┘

  rust/src/cli/connection.rs:913 已经把 os error 35 识别为 transient，但还不够。要做到：

  {
    "ok": false,
    "error": {
      "kind": "daemon_transient",
      "message": "...",
      "retryable": true,
      "session": "watch_baidu_live",
      "suggested_action": "retry_same_command"
    }
  }

  不要只是字符串里含 os error 35。

  CLI 形态

  建议最终命令分组如下：

  ┌────────────────────────────────────────┬────────────────────┐
  │ 命令                                   │ 作用               │
  ├────────────────────────────────────────┼────────────────────┤
  │ snapshot                               │ 默认短快照         │
  │ snapshot --raw                         │ 原始结构           │
  │ find <locator>                         │ 返回短 summary     │
  │ find-all <locator>                     │ 返回短列表         │
  │ element html <locator>                 │ 显式取 HTML        │
  │ element attrs <locator>                │ 显式取 attrs       │
  │ locate <chain>                         │ 链式定位，返回 ref │
  │ click <locator/ref>                    │ 操作               │
  │ click <locator/ref> --expect-nav       │ 点击并等待导航     │
  │ wait navigation/url/title/text/locator │ 明确等待           │
  └────────────────────────────────────────┴────────────────────┘

  我不建议继续让 find 承担“查找 + dump 完整元素”的职责。这个名字对 agent 来说应该是轻量定位，不是调试 dump。

  落地阶段

  1. Snapshot v2
      - 改 snapshot_payload() 和 CLI 参数。
      - 默认 text 短输出。
      - 支持 --format json、--raw、--depth、--selector、--mode interactive|semantic|all。
      - 验证：百度/必应/腾讯新闻快照低于固定字符数，仍能找到目标链接。
  2. Element Summary
      - 改 web_element_to_json()。
      - 移除默认 html。
      - 新增 element html 或保留已有 html 类命令做显式输出。
      - 验证：find "a" 不再返回大 HTML，element html "a" 能取完整 HTML。
  3. LocatorChain
      - 把 DP 风格 parent/child/next/prev/before/after 暴露到 CLI。
      - 先做结构化内部模型，再做 CLI 字符串解析。
      - 验证：能执行 @e12 >> next、@e12 >> parent >> child a 这类路径。
  4. RefRegistry
      - 从 DOM data-op-ref 迁移到内部 registry。
      - ref 失效时用语义信息重新解析。
      - 验证：snapshot 后点击、页面局部更新后点击、frame 内点击都稳定。
  5. 错误与连接稳定性
      - daemon 请求增加 retry policy 和结构化错误。
      - session inactive 默认自动 start，除非用户传 --existing-only。
      - stop/idle 清理 stale daemon。
      - 验证：连续 100 次 url/title/snapshot 不出现 os35 暴露给用户。

  成功标准

  ┌───────────────────────────────┬──────────────────────────────────────────────────────────────────────────┐
  │ 场景                          │ 标准                                                                     │
  ├───────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
  │ 正常 agent 搜索百度并点击官网 │ 不用 JS，只用 CLI，能稳定完成                                            │
  │ 快照长度                      │ 默认足够短，agent 不被大内容淹没                                         │
  │ 元素查找                      │ 默认不返回 HTML                                                          │
  │ 链式定位                      │ 能用相邻、父子、范围关系解决“这个标题下面的按钮”                         │
  │ 错误恢复                      │ transient 错误自动重试，最终错误可读、可机器处理                         │
  │ 差异化                        │ OpenPage 不只是 Playwright CLI，而是 DP 风格定位 + agent snapshot 的结合 │
  └───────────────────────────────┴──────────────────────────────────────────────────────────────────────────┘

  我的建议是：下一步不要先修单个错误，先实现 Snapshot v2 + Element Summary。这是最高收益点，能立刻解决“内容过大”和 agent 决策困难。然后再做
  LocatorChain，把你已有的 DP 能力正式产品化。
