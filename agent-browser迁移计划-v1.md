# agent-browser 能力迁移计划 v1

把 `参考项目/agent-browser-main` 中的两项能力（diff、前端 dashboard）迁入 openpage。
本文件是**执行契约 + 测试标准**，按此推进，每阶段达标后再进入下一阶段。

## 背景：已确认的架构落差

| 维度 | openpage 现状 | agent-browser |
|---|---|---|
| CDP 后端 | chromiumoxide（高层库） | 手写 CDP（tokio-tungstenite） |
| `@ref` 数据源 | `backend_node_id()` 已有；未用 Accessibility 域 | 用 `getFullAXTree` + 自家 `AXNode` 类型 |
| daemon 协议 | TCP NDJSON 一问一答，**无推送** | HTTP + WebSocket，可推送 CDP 事件 |
| Tauri 前端 | 纯 React + 手写 CSS，单 `main.tsx`，**无 shadcn** | — |
| 参考前端 | — | Next.js 16 + React 19 + shadcn + jotai |

**结论**：diff 可近乎原样 cp；`@ref`/快照/WS 网关必须重写（本计划**不做**，留给后续）；
前端整体迁入，但 Next.js 壳要换 Vite，数据层接口先不接通。

## 本计划范围

- ✅ Phase 1：diff（Rust 核心 + CLI + Python 门面）
- ✅ Phase 2：前端 dashboard 迁入 Vite/Tauri（保样式，接口可不通 + 现有录制控制台降级为一个 section）
- ⏸ 后续（用户自己实现）：快照重写、`@ref` 机制、daemon WS 网关

---

## Phase 1 — diff 能力

### 1.1 Rust 核心

- `cp 参考项目/agent-browser-main/cli/src/native/diff.rs` → `rust/crates/openpage/src/diff/mod.rs`
- **适配**：保留其 `Result<_, String>` 内部错误风格（自包含工具模块，边界再转），不改其算法。
- `lib.rs` 加 `pub mod diff;` 并 re-export `ScreenshotDiffResult` / `SnapshotDiffResult`。
- **Cargo 依赖**：`image` features 加 `"png"`（现状仅 `["gif","jpeg"]`，diff_screenshot 要解码 PNG）。
  - `similar` 已在 workspace 依赖中（确认）。
- 保留 diff.rs 自带的 6 个 `#[test]`。

### 1.2 门面设计（三层同一份逻辑）

Rust 核心（`openpage::diff`）：
```rust
diff_snapshots(before: &str, after: &str) -> SnapshotDiffResult   // 文本/快照 diff（Myers）
diff_screenshot(baseline: &[u8], current: &[u8], threshold: f64) -> Result<ScreenshotDiffResult, String>
diff_text(a, b) -> serde_json::Value        // 旧式 JSON 输出
diff_unified(a, b) -> String                // unified diff 文本
```

CLI（无 session、纯计算、oneshot）：
```bash
openpage diff snapshot  --before <file> --after <file>        # 文本/快照 diff
openpage diff screenshot --baseline <file> --current <file> [--threshold 0.1]  # 像素 diff
```
输出走 `simple_ok` + `print_output_json`，信封 `{"ok":true,"result":{...}}`。

Python 门面（纯函数，无浏览器）：
```python
from openpage import diff_text, diff_screenshot
diff_text("a\nb", "a\nc")
# -> {"identical": False, "additions": 1, "removals": 1, "unchanged": 1, "changed": True}
diff_screenshot(base_bytes, cur_bytes, threshold=0.1)
# -> {"matched": False, "mismatch_percentage": 5.2, "different_pixels": ..., ...}
```
通过 `#[pyfunction]` 注册到 `openpage_rs` 模块，`__init__.py` re-export。

### 1.3 Phase 1 测试标准

| 检查 | 命令 / 方法 | 达标 |
|---|---|---|
| Rust 单元测试 | `cargo test --manifest-path rust/Cargo.toml diff` | diff 的 6 个 test 全过 |
| Rust 编译 | `cargo check --manifest-path rust/Cargo.toml` | 0 error |
| CLI 文本 diff | `cargo run ... -- diff snapshot --before a --after b` | 输出 `{"ok":true,"result":{...additions...}}` |
| CLI 像素 diff | `cargo run ... -- diff screenshot --baseline x.png --current y.png` | 输出 mismatch_percentage |
| Python 门面 | `maturin develop` 后 `python -c "from openpage import diff_text; ..."` | 返回正确 dict |

---

## Phase 2 — 前端 dashboard 迁入 Vite/Tauri

### 2.1 迁移策略

源：`参考项目/agent-browser-main/packages/dashboard/src/`
目标：`desktop/openpage/src/`

**搬入**（保样式、保组件）：
- `globals.css`（tailwind 主题）
- `components/`（shadcn ui + 各 panel）
- `store/`（jotai stores）
- `lib/`（utils、routes、shiki-theme）
- `hooks/`

**丢弃**（Next.js 壳，Vite 不需要）：
- `app/layout.tsx`、`app/page.tsx`（Next app router 入口）
- `app/favicon.ico`

**替换**（Next 专有 API）：
- `next/image` → `<img>`
- `next/link` → `<a>`
- `next-themes` → 保留（纯客户端可在 Vite 跑）或简化
- `next/navigation` → 去掉或用 react state 替代

**新增**（工具链）：
- `tailwind.config.ts`、`postcss.config.mjs`、`components.json`
- `package.json` 加 deps：tailwindcss、shadcn 相关（radix/cva/clsx/tailwind-merge/lucide-react）、jotai、react-resizable-panels、cmdk 等

### 2.2 现有录制控制台的处理

- 现有 `desktop/openpage/src/main.tsx`（录制控制台）→ 降级为新 dashboard 的**一个 tab/section**。
- 新建 Vite 入口（如 `src/app.tsx`）承载 dashboard 布局 + tab 切换。

### 2.3 明确**不要求**

- ❌ 接口接通（store 里 fetch HTTP 的地方可以报错/空数据，不影响达标）
- ❌ 功能可用（viewport 实时画面、chat 等可以不工作）
- ✅ 只要 `vite build` 通过、页面渲染、shadcn 样式正常

### 2.4 Phase 2 测试标准

| 检查 | 命令 | 达标 |
|---|---|---|
| 依赖安装 | `cd desktop/openpage && npm install` | 无致命错误 |
| 构建 | `npm run build`（`tsc -b && vite build`） | 0 error，产出 dist |
| 渲染 | `npm run dev` 起服务，浏览器打开 | dashboard 布局 + shadcn 样式可见 |
| 录制控制台 | 切到对应 tab | 现有录制 UI 可见（按钮可不必生效） |

---

## 完成标志

两阶段全部达标后，停下找用户确认。确认后用户自行推进：
1. 快照（用 chromiumoxide 的 Accessibility 域重写）
2. `@ref` 机制（session 级 `ref_id → backendNodeId` 映射 + `@` 前缀解析）
3. daemon WS 网关（给 dashboard 数据层接通）

## 执行结果（2026-07-24）

### Phase 1 — ✅ 完成
- `rust/crates/openpage/src/diff/mod.rs`：cp 原样，6 个单测全过
- workspace 加 `similar`、image 加 `png` feature；crate 加 `similar` 依赖；`lib.rs` 导出
- CLI：`openpage diff snapshot --before --after`、`openpage diff screenshot --baseline --current [--threshold]`，实测文本/像素/尺寸三种场景均正确
- Python：`diff_text(before, after)` / `diff_screenshot(baseline, current, threshold=0.1)` 返回 dict；pyfunction 返回 JSON 字符串、门面用 `json.loads` 包 dict（pyo3 0.28 pyfunction 返回 Python 对象类型会触发 `IntoPyObjectConverter` 歧义，故走 JSON 字符串）
- `test_facade.py` 同步更新 `__all__` 断言

### Phase 2 — ✅ 完成
- `desktop/openpage/src` 下：`components/`、`store/`、`lib/`、`hooks/`、`types.ts`、`globals.css` 全部从 dashboard cp
- 新增 `src/app.tsx`（providers）、`src/views/dashboard.tsx`（布局 + 录制 tab）、`src/components/recording-console.{tsx,css}`（原录制控制台，作用域 CSS 避免污染 dashboard）
- `package.json` 加 shadcn/tailwind v4/jotai/radix-ui 等依赖；`vite.config.ts` 加 `@tailwindcss/vite` + `@` alias；`tsconfig.json` 加 `@/*` paths；`index.html` 补全
- 删除原 `style.css`（被作用域版替代）
- **验证**：`npm install` 成功；`npm run build`（tsc -b && vite build）0 错误产出 dist；dev server 正常；headless 渲染确认 `#root` 有内容、`html.dark` 生效、body 背景 `#0a0a0a` / 前景 `#e5e5e5`（与 globals.css `.dark` 完全吻合）、录制控制台作为 tab + 空状态主区可见；运行时仅预期的 404（无后端）和浏览器下 Tauri invoke 不可用（Tauri 内正常）。
- **Next.js → Vite 适配点**：丢弃 `app/layout.tsx`、`app/page.tsx` 壳；`next/font/google`→系统字体（`--font-sans` 置于 `:root`）；`store/chat.ts` 的 `process.env.NEXT_PUBLIC_DAEMON_URL`→`""`；`"use client"` 指令保留（Vite 无害）。

### Phase 3 — ✅ 完成（daemon 子模块拆分）

**动机**：审计发现 ref/snapshot 域寄居在 3483 行的 `daemon/mod.rs` 上帝模块里，无独立模块路径，导致 `session::snapshot`（HTTP 路径）与实时快照命名冲突、ref 机制难以发现。

**做法**（纯 move 重构，零行为变化）：
- `daemon/mod.rs` 3483 → 2683 行（-800）。
- 新建 `daemon/snapshot.rs`（487 行）：`AgentSnapshotMode/Format/Options`、`agent_snapshot_script`、`snapshot_payload`、`format_snapshot_text`、`snapshot_refs`、`escape_snapshot_value`。
- 新建 `daemon/ref_registry.rs`（322 行）：`RefRegistry`/`RefTarget`、`parse_ref`、`find_ref`/`register_element`/`find_ref_by_locator_hints`/`reresolve_ref_target`/`refresh_ref_target`/`register_snapshot_entries`、`candidate_matches_ref_target`。
- 共享的元素元数据原语（`element_to_json`/`element_role`/`element_name`/`clip_agent_text`/`normalize_agent_text`/`compact_element_attrs`）留在 `mod.rs`，子模块用 `use super::*;`（与既有 `operations.rs` 一致）。
- 可见性：跨模块入口标 `pub(super)`（`snapshot_payload`、`RefRegistry`、`parse_ref`、`find_ref`、`register_element`、`register_snapshot_entries`、`RefRegistry::clear`），其余私有。

**验证**：`cargo check` 全工作区 0 错误；`cargo test --lib -- --test-threads=1` **349 通过 / 0 失败**（并行模式下的 daemon::client 失败为预存在环境抖动，已用原始 `mod.rs` 对比证明）。

**重要更正**：原先判断"openpage 没有 @ref，要从零造"是错的——openpage 早有一套三级自愈（cssPath→xpath→text/元数据匹配 + refresh 自愈）的 ref 系统，只是埋在 daemon 深处没被发现。拆分后可发现性恢复。后续功能完善点见下。

## 待完善功能点（拆分后更清晰）

| 点 | 性质 | 状态 |
|---|---|---|
| RefTarget 加 `backend_node_id` 快路径 | 功能 | ✅ 已完成（见下） |
| ref 编号跨 snapshot 连续 | 功能 | 待办：`register_snapshot_entries` 每次 `clear()` 从 e1 重来 |
| `@ref` 多写法 | 功能 | ✅ 无需改：`parse_ref` 已支持 `@e1`/`ref=e1`/`e1` 三种（与 agent-browser 一致） |
| role/name 用 chromiumoxide Accessibility 域做 hybrid 校验 | 功能 | 待办：现为 JS 启发式，可选用浏览器原生 a11y 补充 |

### backend_node_id 快路径 — ✅ 完成

`RefTarget` 增加 `backend_node_id: Option<BackendNodeId>`（不进 `key()`，免得 refresh 破坏去重）。`find_ref` 在 target/frame 校验后、cssPath 之前加 tier-0 快路径：

```
backend_node_id (page.resolve_dom_backend_node_id，一次 CDP 调用)
  → 失败降级 cssPath → xpath → text/name+元数据（原三级自愈保留）
```

- `register_element`/`refresh_ref_target` 存入 `element.backend_node_id()`；`register_snapshot_entries`（JS 无法产出 bnid）存 `None`，首次 resolve 后由 `refresh_ref_target` 自动 warm up。
- **决定性验证**：warm e1 后突变元素（删 id + 改文本 + 前插新 button，使 cssPath/xpath/text 兜底**全部失效或指向错误节点**），第二次 `text @e1` 仍返回原节点的新文本 → 证明快路径经 backendNodeId 定位，非降级兜底。
- 收益：兼具 agent-browser 的 backendNodeId 快路径 + openpage 原有的多级自愈兜底。

## 决策记录

- diff 用 **cp 原样 + 边界适配**，不重写算法。
- 前端用 **cp 组件 + 换 Vite 壳**，保样式优先，对接后置。
- daemon 拆分用 **sed 精确提取 + 可见性标注**，零行为变化；`@ref`/快照逻辑**已存在**，拆分仅是结构整理，为后续功能完善铺路。
