# OpenPage 录制架构 v1

## 1. 目标

OpenPage 的“录制”分成两个能力：

1. **操作录制**：记录用户在 Chromium 中的导航、点击、输入、选择、勾选和按键，生成可编辑、可回放的结构化流程。
2. **屏幕录制**：输出视频、GIF 或图片序列。

当前 Core 已有屏幕录制基础：

```text
rust/crates/openpage/src/screencast/mod.rs
```

因此不重新设计屏幕录制。本文主要设计操作录制。

## 2. 总体结论

桌面端不是录制 Core 的前置条件。录制协议、事件采集和回放可以先通过 Core、daemon 和 CLI 完成；桌面端负责把这些能力产品化。

推荐架构：

```text
React UI
  ↓ invoke / event
Tauri Rust Shell
  ↓ local TCP / NDJSON
OpenPage daemon
  ↓
OpenPage Core
  ↓
Chromium + CDP
```

核心原则：

- React 只负责界面和流程编辑，不负责浏览器控制。
- Tauri 只负责桌面壳、本地桥接、文件和窗口，不重写浏览器自动化。
- OpenPage Core 是录制、定位、回放的唯一事实来源。
- CLI、MCP、Python 和桌面端复用同一个 daemon/Core 协议。
- 录制文件先保存为结构化 JSON，再导出 Python、Rust 或 CLI。

## 3. 现有 OpenPage 接入点

当前调用链：

```text
CLI / MCP / Python
        ↓
NDJSON daemon protocol
        ↓
ServeRuntime::dispatch()
        ↓
dispatch_webpage()
        ↓
WebPage / Page / Element / Actions
        ↓
CDP / chromiumoxide
```

重要文件：

- `rust/crates/openpage/src/page/mod.rs`：页面、Actions、导航语义。
- `rust/crates/openpage/src/element/mod.rs`：元素点击、输入、选择、拖动等能力。
- `rust/apps/openpage/src/commands/serve.rs`：daemon 请求分发。
- `rust/apps/openpage/src/commands/serve.rs`：已有 locator chain 解析。
- `rust/crates/openpage/src/screencast/mod.rs`：已有画面录制。

现有 locator chain 主要描述“如何定位元素”，例如：

```text
css:#app >> child text:"Learn more" >> parent
```

录制能力应在此基础上增加“动作链”，而不是修改 locator chain 的基本语义。

## 4. 录制分层

### 4.1 浏览器事件采集

通过 CDP 向页面注入小型 JavaScript，采集：

- `click`
- `input`
- `change`
- `keydown`
- `submit`
- 页面导航
- frame 变化
- 新 tab 变化

推荐使用：

- `Page.addScriptToEvaluateOnNewDocument`
- `Runtime.addBinding`
- `Runtime.bindingCalled`

注入脚本只报告事实，不直接生成最终 locator。

### 4.2 Rust 事件归一化

浏览器原始事件转换为 OpenPage 语义步骤：

```rust
pub enum RecordedAction {
    Goto { url: String },
    Click { target: RecordedTarget },
    Fill { target: RecordedTarget, value: String },
    Select { target: RecordedTarget, values: Vec<String> },
    Check { target: RecordedTarget, checked: bool },
    Press { target: Option<RecordedTarget>, key: String },
}
```

第一版只实现以上六类动作。拖动、上传、下载、hover、弹窗、多窗口等按真实需求追加。

## 5. 录制文件模型

录制文件是结构化 JSON：

```json
{
  "version": 1,
  "steps": [
    {
      "action": "goto",
      "url": "https://example.com/login"
    },
    {
      "action": "fill",
      "target": {
        "locator": "css:input[name=email]",
        "fallbacks": []
      },
      "value": "user@example.com"
    },
    {
      "action": "click",
      "target": {
        "locator": "role:button name:\"登录\""
      }
    }
  ]
}
```

建议内部使用结构化类型，导出阶段再生成：

```rust
pub struct RecordedStep {
    pub action: RecordedAction,
    pub target: Option<RecordedTarget>,
    pub wait_after: Option<RecordedWait>,
    pub sensitive: bool,
}
```

## 6. Locator 生成策略

定位器优先级：

1. 稳定的测试属性。
2. 可访问角色和名称。
3. 唯一 `id`。
4. `name`、`placeholder`、`aria-label`。
5. 稳定文本。
6. 必要时使用父子 locator chain。
7. 最后才使用 CSS path 或 `nth`。

目标示例：

```json
{
  "primary": "role:button name:\"登录\"",
  "fallbacks": [
    "text:\"登录\"",
    "css:#login-button"
  ]
}
```

默认输出只展示 primary locator；fallback 用于定位失败时恢复，不应一开始引入复杂的 AI selector。

链式关系只在单个定位器无法唯一命中时使用：

```text
role:form name:"登录" >> child role:button name:"提交"
```

## 7. 事件压缩

录制器必须把浏览器事件压缩为用户意图：

- 连续 `input` 合并为一个 `fill`。
- checkbox 的 `click + change` 合并为 `check/uncheck`。
- click 后发生导航时，click 标记 `wait_after=navigation`，不重复生成无意义的 goto。
- 用户直接改变地址栏或发生不可归因导航时才记录 `goto`。

现有 `record_navigation_baseline()`、navigation token 和 `wait.navigation` 应复用，不新建第二套导航等待逻辑。

## 8. Core API

建议新增：

```text
rust/crates/openpage/src/recorder/mod.rs
```

最小接口：

```rust
pub struct Recorder { /* shared state */ }

impl Recorder {
    pub fn start(&self) -> OpenPageResult<()>;
    pub fn stop(&self) -> OpenPageResult<Vec<RecordedStep>>;
    pub fn steps(&self) -> OpenPageResult<Vec<RecordedStep>>;
    pub fn clear(&self) -> OpenPageResult<()>;
    pub fn is_running(&self) -> OpenPageResult<bool>;
}
```

Page 关联 Recorder，沿用现有 screencast、listener、interceptor 的组件模式。不新增单实现 trait、factory 或复杂 manager。

## 9. daemon 操作

第一版增加：

```text
recorder.start
recorder.stop
recorder.steps
recorder.clear
recorder.status
```

例如：

```json
{
  "op": "recorder.start",
  "target": "wp_1",
  "params": {}
}
```

停止返回结构化 steps。回放不新建执行引擎，而是把 `RecordedStep` 映射到现有 `dispatch_webpage()`：

```text
goto   → webpage.get
click  → element.click
fill   → element.input
select → element.select
press  → Actions/key operation
```

## 10. 敏感数据

密码、token、OTP、信用卡字段和敏感本地路径默认不保存明文：

```json
{
  "value": {
    "secret": "PASSWORD"
  }
}
```

导出 Python 时使用环境变量或运行时输入。敏感字段处理属于信任边界，不能推迟到后续版本。

## 11. iframe 与新 tab

录制事件带上 frame context：

```json
{
  "context": {
    "frames": ["css:iframe[name=payment]"]
  },
  "target": {
    "locator": "text:\"Pay\""
  }
}
```

新 tab 先记录事实：

```json
{
  "expect": {
    "new_tab": true
  }
}
```

具体回放行为沿用现有 tab/page 控制能力。

## 12. React + Tauri 桌面端

桌面端建议采用两个窗口/进程角色：

```text
Tauri 控制台
  - 录制状态
  - 步骤列表
  - locator 编辑
  - 回放、保存、导出

OpenPage Chromium
  - 用户实际操作页面
  - CDP 事件采集
  - 真实浏览器 profile
```

第一版不把浏览器页面嵌到 Tauri WebView 中。Tauri WebView 在 Windows、macOS、Linux 上依赖不同系统 WebView，行为不应作为 OpenPage 自动化浏览器的事实来源。Tauri 应只展示控制台，OpenPage 启动和管理真正的 Chromium。

推荐通信：

```text
React
  ↓ invoke / listen
Tauri Rust
  ↓ local TCP / NDJSON
OpenPage daemon
```

React 第一版只需要 `useState`、`useEffect`、Tauri `invoke` 和事件监听，不需要 Redux、复杂工作流引擎或插件系统。

## 13. 桌面端 MVP

```text
启动浏览器
开始录制
停止录制
显示步骤
保存 JSON
回放
```

推荐目录：

```text
desktop/
├── src/          # React + TypeScript
└── src-tauri/    # Tauri Rust shell
```

桌面端不是 Core 的前置条件。先以 CLI 验证录制协议，再接入 Tauri，可避免把 UI 问题和录制模型问题绑在一起。

## 14. 实施顺序

### 阶段一：Core + CLI

实现：

```text
recorder.start
recorder.stop
RecordedStep
flow.json
click / fill / select / check / goto / press
密码脱敏
```

验收：录制登录流程，停止得到 JSON，JSON 可回放，连续输入合并，密码不落盘。

### 阶段二：Tauri 控制台

使用：

```text
React + TypeScript + Vite + Tauri 2
```

只实现启动浏览器、开始/停止录制、步骤显示、保存 JSON、回放。

### 阶段三：流程编辑器

增加 locator 编辑、删除/排序步骤、单步回放、敏感字段替换。

### 阶段四：导出器

增加 JSON、Python、Rust、CLI shell 导出。所有导出器共享同一个结构化 flow，不复制录制逻辑。

## 15. 类似项目的借鉴边界

- Playwright Codegen：借鉴 locator 生成、输入合并和代码导出。
- Chrome DevTools Recorder：借鉴结构化 flow、wait/assertion 和导出分离。
- rrweb：只借鉴事件采集和隐私处理，不把 DOM replay 当作自动化脚本模型。
- Selenium IDE：借鉴简单的命令式步骤模型，但使用 OpenPage locator chain。

OpenPage 不需要第一版复制大型 Inspector、GUI 录制器或 AI selector。

## 16. MCPStore 架构参考观察

本次检查 `/Volumes/data0/data4work/2025_6/mcpstore` 后，最值得借鉴的是“源码包、应用、分发物和 UI 分离”的边界，而不是具体代码。

### 16.1 源码包与 App 分离

MCPStore 将 Rust workspace 分为：

```text
rust/crates/mcpstore/   # Rust SDK / core
rust/apps/mcpstore/     # 完整 CLI 应用
rust/bindings/python/   # Python binding
```

`crates/mcpstore` 面向库调用；`apps/mcpstore` 面向终端应用。完整 CLI 不塞进 SDK 的公开分发语义中。

这对 OpenPage 的启示是：

```text
rust/crates/openpage/       # 浏览器 core / SDK
rust/apps/openpage/         # CLI / daemon app
rust/bindings/python/       # Python bridge
```

未来桌面端也不应成为 Core 的一部分，而应作为独立 App：

```text
desktop/openpage/           # React + Tauri desktop app
```

### 16.2 Web 与桌面端共享产品能力，不共享运行形态

MCPStore 目前把 Web 前端放在：

```text
web/
```

桌面端 Tauri 放在：

```text
desktop/tauri/
```

两者不是把同一个页面强行变成一个运行时，而是分别承担：

```text
Web       → 浏览器访问的 UI
Tauri     → 本地桌面 UI / 本地系统能力
Rust core → 真实业务和服务能力
```

OpenPage 可以沿用这个关系：

```text
web/                 # 可选：远程/浏览器管理控制台
desktop/openpage/    # React + Tauri 本地录制控制台
rust/crates/openpage # 浏览器 core
rust/apps/openpage   # daemon / CLI
```

Web 和桌面端都应调用同一套 daemon/API/flow 协议，而不是各自重新实现录制逻辑。

### 16.3 分发物与源码包语义分离

MCPStore 对 Rust SDK、Python SDK、MCP runner、CLI、npm 平台二进制分别定义语义：

```text
Rust crate       → SDK
Python package   → Python SDK
uvx              → 窄 MCP runner
npm/curl         → 完整 CLI
```

OpenPage 也应该提前定义：

```text
Rust crate       → OpenPage 浏览器 SDK
Python package   → Python wrapper / SDK
CLI binary       → daemon / terminal control
npm package      → CLI 或桌面端安装器，二者不要混淆
Tauri app        → 桌面录制与流程管理产品
```

不要把“桌面端”误认为是另一个 Core，也不要让 npm 包同时承担 JS SDK、CLI 和桌面安装器而没有清晰边界。

### 16.4 MCPStore 对 OpenPage 最有价值的结构

```text
共享 Core 能力
    ↓
CLI / daemon / API / Web / Desktop 多个产品入口
    ↓
各入口只负责适配自己的交互与分发
```

对于录制能力，建议定义一个共享协议：

```text
RecordedStep
RecordedFlow
Recorder status/events
Replay request/result
```

然后由不同入口消费：

```text
CLI      → 命令行录制和 JSON 导出
Python   → Python API
Web      → 远程流程查看和编辑
Desktop  → 本地录制、浏览器控制、流程编辑
MCP      → agent 调用录制/回放
```

## 17. 当前不做

- 不先做浏览器内嵌。
- 不先做完整桌面端。
- 不先做 Web 和 Desktop 两套不同录制协议。
- 不先做 AI selector。
- 不先做完整 Inspector。
- 不把 Screencast 和操作 Recorder 混成一个模块。
- 不把录制逻辑复制到 React、Tauri、CLI、Python 四处。

# 18. 要做的事情与目标

## 18.1 总体目标

为 OpenPage 增加一套基于现有 Rust Core、daemon 和 locator chain 的浏览器操作录制能力，并以 React + Tauri 构建跨平台桌面控制台。

最终用户可以：

```text
启动 OpenPage
打开受控 Chromium
开始录制
手动操作网页
停止录制
查看并编辑步骤
保存录制流程
回放流程
导出 Python / Rust / CLI 脚本
```

目标不是做一个单独的录屏软件，而是做一个能够把真实浏览器操作转换为 OpenPage 自动化流程的产品能力。

## 18.2 需要完成的核心事情

### 事情一：定义录制协议

建立共享的结构化数据模型：

```text
RecordedStep
RecordedTarget
RecordedFlow
RecordedEvent
RecorderStatus
ReplayResult
```

该协议是 Core、daemon、CLI、Python、Web 和 Desktop 之间的共同边界。

目标：

- 不让 React 保存最终代码字符串。
- 不让不同客户端分别设计录制格式。
- 录制结果可以长期保存、编辑和升级。
- 未来可以增加新的导出格式，而不修改录制核心。

### 事情二：在 OpenPage Core 中实现操作录制

新增 Recorder 能力，挂载在 Page 上，复用现有组件模式：

```text
Page
 ├── Screencast
 ├── Listener
 ├── Interceptor
 └── Recorder
```

Recorder 负责：

- 启动和停止录制。
- 接收浏览器事件。
- 合并连续输入事件。
- 识别点击、输入、选择、勾选、按键和导航。
- 生成稳定 locator。
- 记录 frame、tab 和 navigation context。
- 对密码和敏感字段进行脱敏。
- 产出 `RecordedFlow`。

目标：

> 录制逻辑只存在于 OpenPage Core，不复制到 CLI、Python、React 或 Tauri。

### 事情三：通过 CDP 采集浏览器事件

向页面注入最小事件采集脚本，使用 CDP 把事件发送到 Rust：

```text
DOM event
  ↓
Injected recorder script
  ↓
Runtime binding
  ↓
CDP event
  ↓
Rust Recorder
```

第一版支持：

```text
click
input
change
keydown
navigation
```

目标：

- 页面导航后自动重新注入。
- 能识别当前 frame。
- 能识别当前 tab。
- 不依赖轮询 DOM。
- 不使用 console 日志作为正式协议。

### 事情四：复用现有 locator chain

录制器不生成脆弱的绝对 CSS 作为唯一定位方式，而是基于现有 locator 能力生成候选：

```text
稳定属性
  → role/name
  → id
  → name/placeholder/aria-label
  → 文本
  → 父子 chain
  → CSS fallback
```

目标：

- 录制出来的步骤能够被现有 `resolve_locator_chain()` 解析。
- 回放时继续使用现有 `find()`、`element.click()`、`element.input()` 等能力。
- 不新增第二套定位语法。
- locator 失败时可以使用 fallback。

### 事情五：把回放接入现有 dispatch

回放不新建第二套执行引擎，而是把录制步骤转换为现有 daemon operation：

```text
RecordedAction
  ↓
Request
  ↓
dispatch_webpage()
  ↓
现有 Page / Element / Actions
```

目标：

- CLI 录制的流程可以由 daemon 回放。
- Python 录制的流程可以由同一个 daemon 回放。
- Tauri 只发起回放请求，不实现浏览器动作。
- 所有客户端使用相同的错误、等待和导航语义。

### 事情六：增加 daemon 录制接口

增加最小协议：

```text
recorder.start
recorder.stop
recorder.steps
recorder.clear
recorder.status
recorder.replay
```

目标：

- CLI、Python、Web、Desktop 共享同一套接口。
- 录制状态可以被 UI 订阅。
- 新客户端不需要直接访问 Chromium/CDP。
- daemon 继续作为长生命周期浏览器控制入口。

### 事情七：先用 CLI 验证 Core

在桌面端之前，先支持：

```bash
openpage record --session demo --output flow.json
openpage replay flow.json --session demo
```

目标：

- 先验证录制事件模型。
- 先验证 locator 稳定性。
- 先验证 JSON 格式。
- 先验证回放链路。
- 避免 UI 问题掩盖 Core 问题。

CLI 版本不是最终产品，而是 Core 的最小验收工具。

### 事情八：构建 React + Tauri 桌面端

桌面端作为独立 App：

```text
desktop/openpage/
├── src/          # React + TypeScript
└── src-tauri/    # Tauri Rust shell
```

React 负责：

- 启动/停止录制按钮。
- 录制状态展示。
- 当前 URL 展示。
- 步骤列表。
- locator 编辑。
- 删除和排序步骤。
- 保存 flow JSON。
- 启动回放。
- 导出脚本。

Tauri Rust 负责：

- 调用 OpenPage daemon。
- 转发 daemon 事件给 React。
- 本地文件选择与保存。
- 跨平台窗口和系统能力。
- 启动或连接 OpenPage 进程。

目标：

> Tauri 是 OpenPage 录制能力的桌面控制台，不是新的浏览器自动化 Core。

### 事情九：明确 Web 与 Desktop 的关系

OpenPage 未来可以有两个 UI 入口：

```text
Web
  → 适合远程管理、查看和编辑流程

Desktop
  → 适合本地浏览器录制、文件管理和桌面控制
```

两者共享：

```text
RecordedFlow
Recorder API
Replay API
daemon / HTTP API
locator 语义
```

两者不共享：

```text
窗口生命周期
本地文件访问方式
系统托盘
浏览器启动方式
桌面权限
```

目标：

- Web 不直接控制本地浏览器进程。
- Desktop 不复制 Web 的业务协议。
- 两端都使用同一个 Core/daemon 能力。
- 后续可以独立发布 Web 和 Desktop，而不破坏 Core。

### 事情十：定义源码包、App 和分发边界

OpenPage 参考 MCPStore 的边界，明确以下产品语义：

```text
Rust crate
  → OpenPage SDK / Core

Python package
  → Python SDK / wrapper

CLI binary
  → daemon / terminal control

Web app
  → Web 控制台

Tauri app
  → 桌面录制和流程管理产品

npm package
  → CLI 或 Desktop 的安装/分发包装器，具体语义单独确定
```

目标：

- SDK 不被桌面端需求污染。
- Desktop 不被 CLI 的命令结构绑定。
- Web 不成为浏览器自动化核心。
- 分发物的名称、用途和安装方式清晰。
- 同一个 Core 可以服务多个产品入口。

## 18.3 第一版成功标准

第一版完成后，应满足：

1. 可以启动 OpenPage 自己管理的 Chromium。
2. 可以开始和停止操作录制。
3. 可以记录 `goto`、`click`、`fill`、`select`、`check`、`press`。
4. 连续键盘输入会合并为一个 `fill` 步骤。
5. 密码和敏感字段不会明文写入 flow 文件。
6. 录制结果可以保存为版本化 JSON。
7. JSON 可以通过现有 daemon/Core 回放。
8. locator 使用现有 OpenPage locator chain 语义。
9. CLI 和桌面端使用同一套录制协议。
10. React 只负责展示和编辑，Tauri 只负责桥接，Core 负责真实行为。

## 18.4 第一版明确不做

第一版暂不做：

- 浏览器页面嵌入 Tauri WebView。
- 接管用户已经打开的任意 Chrome。
- AI 自动生成 selector。
- 完整可视化 Inspector。
- 录制鼠标移动轨迹。
- 完整 DOM snapshot replay。
- 复杂 assertion 编辑器。
- 插件市场。
- Web 与 Desktop 两套独立业务实现。
- 多套录制文件格式。
- 与 Screencast 合并成同一个 Recorder。

这些能力只有在第一版录制和回放稳定后再评估。

## 18.5 推荐工作流

```text
阶段 1：Core 数据模型和 CDP 事件采集
        ↓
阶段 2：事件归一化、locator 生成和 JSON
        ↓
阶段 3：daemon start/stop/steps/replay
        ↓
阶段 4：CLI record/replay 验证
        ↓
阶段 5：React + Tauri 控制台
        ↓
阶段 6：流程编辑、导出和 Web 控制台
```

每个阶段都必须优先验证共享协议，不先堆 UI。

## 18.6 最终产品形态

```text
                    ┌───────────────┐
                    │ React Web UI  │
                    └───────┬───────┘
                            │
                    ┌───────▼───────┐
                    │ Tauri Desktop │
                    └───────┬───────┘
                            │
              ┌─────────────▼─────────────┐
              │ OpenPage daemon / API      │
              └─────────────┬─────────────┘
                            │
              ┌─────────────▼─────────────┐
              │ OpenPage Rust Core         │
              │ Recorder / Locator / Page │
              └─────────────┬─────────────┘
                            │
                    ┌───────▼───────┐
                    │ Chromium/CDP │
                    └───────────────┘
```

一句话目标：

> OpenPage Core 提供唯一的浏览器录制与回放能力，CLI、Web 和 React + Tauri Desktop 只是共享该能力的不同产品入口。

# 19. 执行进度

## 里程碑 1：Core 录制数据模型

状态：**已完成**

完成内容：

- 新增 `rust/crates/openpage/src/recorder/mod.rs`。
- 定义版本化 `RecordedFlow`、`RecordedStep`、`RecordedAction`、`RecordedTarget`。
- 定义敏感值 `RecordedValue::Secret` 和等待语义 `RecordedWait`。
- 实现 `Recorder` 的 `start`、`stop`、`flow`、`clear`、`status`。
- 连续向同一目标输入时合并为一个 `fill` 步骤。
- `Page` 持有并公开同一个共享 `Recorder` 实例。
- 从 Rust crate 根模块公开录制协议类型。

验证证据：

```text
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib

2 passed; 0 failed
```

下一里程碑：CDP 页面事件采集与原始事件归一化。

## 里程碑 2：CDP 页面事件采集

状态：**已完成**

完成内容：

- `Recorder` 绑定到 `Page`，每个页面使用自己的共享录制状态。
- 通过 CDP `Runtime.addBinding` 接收页面事件。
- 通过 `Page.addScriptToEvaluateOnNewDocument` 保证导航后重新注入。
- 当前 document 启动录制时立即注入。
- 已采集 `click`、`input`、`change`、`keydown`。
- 支持 `fill`、`select`、`check`、`press` 事件转换。
- password input 转换为 `RecordedValue::Secret`，不保存实际输入值。
- 页面事件使用现有 `css:` locator 语义生成目标。
- 连续同目标输入继续由 Core 合并。

验证证据：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib

2 passed; 0 failed
```

下一里程碑：将 Recorder 接入 daemon 协议和 CLI，并增加 JSON flow 的保存入口。

## 里程碑 3：daemon 录制协议

状态：**已完成**

完成内容：

- `WebPage` 暴露 Core Recorder。
- daemon 增加 `recorder.start`。
- daemon 增加 `recorder.stop`。
- daemon 增加 `recorder.steps`。
- daemon 增加 `recorder.clear`。
- daemon 增加 `recorder.status`。
- daemon 返回版本化 `RecordedFlow` 和状态 JSON。
- daemon 仍通过现有 `webpage` target 路由，不新增第二个浏览器控制入口。

验证证据：

```text
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib

2 passed; 0 failed
```

说明：daemon 全量测试中已有与本次改动无关的 sidecar/临时目录测试失败；录制模块测试和 crate 编译通过。当前录制模块专用测试已覆盖核心归一化逻辑；daemon 协议验收继续以真实 daemon 命令链为准。

下一里程碑：CLI `record` / `replay` 入口和 flow JSON 文件保存。

## 里程碑 4：CLI 录制控制

状态：**已完成**

完成内容：

- 新增 `openpage record start --session <name>`。
- 新增 `openpage record stop --session <name>`。
- `record stop --output <file>` 可以保存 flow JSON。
- 新增 `openpage record steps --session <name>`。
- 新增 `openpage record status --session <name>`。
- 新增 `openpage record clear --session <name>`。
- CLI 通过现有 daemon RPC 调用，不直接接触浏览器/CDP。

使用方式：

```bash
openpage browser start --session demo --head
openpage record start --session demo
# 手动操作 Chromium
openpage record stop --session demo --output flow.json
openpage record steps --session demo
```

验证证据：

```text
cargo check --manifest-path rust/Cargo.toml -p openpage-app
cargo run --manifest-path rust/apps/openpage/Cargo.toml -- record --help
```

下一里程碑：基于 `RecordedFlow` 实现 daemon/Core 回放，并补充不依赖真实浏览器的协议测试。

## 里程碑 5：Core/daemon 回放

状态：**已完成**

完成内容：

- daemon 新增 `recorder.replay` 操作。
- 回放复用现有 `Page.goto`、locator chain、元素输入/点击/选择/勾选和键盘操作能力。
- `RecordedAction::Goto`、`Click`、`Fill`、`Select`、`Check`、`Press` 均有对应执行路径。
- `RecordedValue::Secret` 默认拒绝直接回放，不把敏感值写入 flow，也不静默猜测密码。
- CLI 新增：

```bash
openpage record replay flow.json --session demo
```

- 回放结果返回已执行步骤数量和 flow 版本。

验证证据：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml -p openpage
cargo check --manifest-path rust/Cargo.toml -p openpage-app
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib
cargo run --manifest-path rust/apps/openpage/Cargo.toml -- record replay --help
```

说明：secret 运行时注入已通过 daemon replay 的显式 `secrets` 映射完成；flow 文件本身不保存真实密码。

下一里程碑：补充真实 Chromium 录制/回放验收，并开始 React + Tauri 桌面端最小闭环。

## 里程碑 6：真实 Chromium 录制验收

状态：**已完成**

使用真实 OpenPage daemon 管理的 Chromium 验证了：

- 启动浏览器 session。
- 开始和停止录制。
- 通过 CDP 页面事件记录 `goto`。
- 通过页面事件记录 `fill` 和 `click`。
- 连续输入合并为一个 `fill` 步骤。
- 录制结果保存为版本化 JSON。

实际验收结果：

```json
{
  "steps": [
    {"action": "goto", "url": "https://example.com/"},
    {"action": "fill", "target": {"locator": "css:#email"}, "value": "a"},
    {"action": "click", "target": {"locator": "css:#go"}}
  ],
  "version": 1
}
```

补充修复：导航监听使用 CDP `Page.frameNavigated`，只记录主 frame，避免把 iframe 导航误记为主流程步骤。

验证环境：2026 年 7 月 20 日，macOS，本地 OpenPage daemon + Chromium。

下一里程碑：实现 React + Tauri 桌面端最小控制台。

## 里程碑 7：React + Tauri 桌面端最小闭环

状态：**已完成**

新增独立应用：

```text
desktop/openpage/
├── src/                 # React + TypeScript UI
└── src-tauri/           # Tauri 2 Rust shell
```

第一版桌面端完成：

- 开始录制。
- 停止录制。
- 查看录制状态和步骤数量。
- 查看结构化步骤 JSON。
- 保存 `flow.json`。
- 回放当前 flow。
- 清空录制步骤。
- 通过 Tauri Rust shell 使用本地 NDJSON TCP 调用现有 OpenPage daemon。

边界保持不变：

```text
React → Tauri invoke → local TCP/NDJSON → OpenPage daemon → Core → Chromium/CDP
```

桌面端没有复制浏览器控制逻辑，也没有把浏览器嵌入 Tauri WebView。桌面端已支持 session 选择，不改变协议和 Core。

验证证据：

```text
cd desktop/openpage
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

v1 结论：录制 Core、daemon、CLI 和 React + Tauri 桌面控制台已经完成。Windows/Linux 的原生安装包应在对应 CI runner 上继续做发布验收；这不改变 v1 的协议和源码闭环。

补充验收：

```text
npm run tauri build -- --bundles app
```

已在 macOS 成功生成：

```text
src-tauri/target/release/bundle/macos/OpenPage.app
```

Core 回放也已用真实 daemon 验证：在相同 DOM 中回放 `fill + click` flow，daemon 返回 `{"replayed":2,"version":1}`。

桌面端补充：

- 增加“启动/连接浏览器”入口。
- Tauri shell 默认调用 PATH 中的 `openpage` CLI；也支持通过 `OPENPAGE_BIN` 指定 CLI 路径。
- 浏览器生命周期仍由 OpenPage CLI/daemon 管理，React 不直接启动 Chromium。

因此桌面端 MVP 已覆盖“启动或连接现有 OpenPage session”，而不是只依赖用户预先启动 daemon。

## 里程碑 8：事件归一化修复

状态：**已完成**

根据真实 Chromium 审查结果修复：

- `checkbox` / `radio` 不再被 `input` 事件误记为 `fill`。
- `select` 不再被 `input` 事件误记为 `fill`，只生成 `select`。
- 同一目标的 `click + check` 合并为单个 `check` 步骤。
- 同一流程中的 `click + 主 frame 导航` 合并为 `click`，并设置 `wait_after: "navigation"`，不再追加重复 `goto`。
- 增加了 checkbox 合并和 click-navigation 合并的单元测试。

真实浏览器验证结果：

```json
{
  "steps": [
    {"action": "check", "target": {"locator": "css:#ok"}, "checked": true},
    {"action": "select", "target": {"locator": "css:#kind"}, "values": ["b"]}
  ],
  "version": 1
}
```

本里程碑的边界是：secret 需要运行时输入；fallback locator、iframe frame 路由和多 session UI 分别由后续里程碑完成。

补充稳健性修复：

- Recorder 初始化 listener 失败时回滚 `recording` 状态、开始时间和空 flow，避免协议返回失败但状态仍显示录制中的不一致。

验证：

```text
4 recorder tests passed
```

补充真实验收：点击链接触发主 frame 导航后，实际 flow 只保留：

```json
{"action":"click","target":{"locator":"css:#go"},"wait_after":"navigation"}
```

没有重复生成 `goto`。Tauri 本地 daemon TCP 调用同时增加了 5 秒写超时和 30 秒读超时，避免桌面 UI 无限等待。

## 里程碑 9：回放定位与敏感值注入

状态：**已完成**

回放协议现在支持：

- primary locator 失败时按 `RecordedTarget.fallbacks` 顺序尝试备用 locator。
- `RecordedValue::Secret` 从 RPC 参数的 `secrets` 对象读取运行时值。
- 缺少 secret 时立即返回明确错误，不会把占位符当成真实密码填写。

RPC 形态：

```json
{
  "flow": {"version": 1, "steps": []},
  "secrets": {
    "PASSWORD": "runtime-only-value"
  }
}
```

真实 flow 文件仍只保存：

```json
{"secret":"PASSWORD"}
```

不会保存真实敏感值。CLI 仍默认拒绝未提供运行时 secret 的回放；需要注入敏感值时使用 daemon RPC 层传入 `secrets`。

验证：

```text
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib
cargo check --manifest-path rust/Cargo.toml -p openpage-app
```

## 里程碑 10：桌面端 session 选择与本轮审查

状态：**已完成**

本轮审查发现桌面端 session 改造存在一个实际编译问题：React state `session` 被桌面端模块外层的 `call()` 函数直接引用，导致 TypeScript `TS18004`。已按最小改动修复：

- `call()` 显式接收 `session` 参数，并将其传给 Tauri `recorder_call` 命令。
- `refresh`、`replay` 等操作均使用当前输入框中的 session。
- session 切换后自动刷新对应 daemon 的状态和步骤。
- Tauri Rust 命令 `recorder_call` 与 `ensure_browser` 接收调用方传入的 session，不再固定使用 `default`。
- 保持原有链路不变：`React → Tauri invoke → local TCP/NDJSON → OpenPage daemon`。

验收命令及结果：

```text
npm run build --prefix desktop/openpage       # 通过
cargo check --manifest-path desktop/openpage/src-tauri/Cargo.toml  # 通过
cargo fmt --all --manifest-path rust/Cargo.toml -- --check         # 通过
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib # 4 passed
```

当前仍未声称完成的事项：

- Tauri 原生文件保存对话框；当前“保存 JSON”使用 WebView 下载。
- stop 时对最后一批异步 CDP 事件的显式 flush。
- `wait_after` 在回放阶段的实际等待语义。
- Windows/Linux 原生安装包和权限验证。
- iframe/frame 路由录制与回放。

这些能力已在后续里程碑纳入当前源码链路；平台安装包仍须在对应原生构建机验收。

## 里程碑 11：桌面端流程编辑与导出

状态：**已完成**

在已有 React + Tauri 控制台上补齐流程管理入口：

- 在线编辑步骤的 primary locator。
- 删除步骤。
- 上移/下移步骤，保持 flow 仍为结构化 JSON。
- 保存 JSON。
- 导出 Python、Rust 和 CLI shell 示例文件。
- 回放使用当前编辑后的 flow，而不是重新从 daemon 拉取后覆盖本地修改。

导出文件是共享 `RecordedFlow` 的轻量包装，不在 React 中复制浏览器控制逻辑；真正执行仍由 daemon/Core 完成。Python/Rust 导出当前是可继续接入项目 SDK/daemon 的模板，CLI 导出包含完整 `flow.json` 和回放命令。

验证：

```text
npm run build --prefix desktop/openpage  # 通过
```

## 里程碑 12：回放兑现录制等待语义

状态：**已完成**

此前 `RecordedStep.wait_after` 只被录制和序列化，回放没有实际消费。本里程碑修复为：

- `click` 步骤带 `wait_after: navigation` 时，回放先建立现有 daemon navigation baseline。
- 点击后调用现有 `wait_for_navigation_payload()`，复用 OpenPage 已有的导航 token、页面 ready 和超时语义。
- locator fallback、secret runtime 注入仍走同一条回放路径。
- 没有新增浏览器控制引擎或第二套等待实现。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 里程碑 13：敏感字段识别扩展

状态：**已完成**

录制脚本不再只依赖 `type=password`：

- password / password-like 字段 → `PASSWORD`。
- OTP / verification 字段 → `OTP`。
- token / secret / api key 字段 → `TOKEN`。
- credit card number 字段 → `CARD_NUMBER`。

flow 仍只保存 `{ "secret": "..." }` 占位符，真实值只允许通过 replay RPC 的 `secrets` 运行时映射注入。该识别是基于字段元数据的保守启发式；无法从页面元数据判断的业务敏感值不能被录制器可靠识别，调用方仍应在导出/保存前复核 flow。

验证：

```text
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 里程碑 14：v1 成功标准最终审计

状态：**已完成**

逐项对照 18.3 的第一版成功标准：

| 标准 | 当前证据 |
|---|---|
| 启动 OpenPage 管理的 Chromium | CLI `browser start` 与 Tauri `ensure_browser` |
| 开始/停止操作录制 | daemon `recorder.start/stop`、CLI、Desktop |
| 记录 goto/click/fill/select/check/press | Core `RecordedAction` 与 CDP 注入脚本 |
| 连续输入合并 | `merge_step()` 与 recorder 单元测试 |
| 敏感值不明文落盘 | password/OTP/token/card 元数据脱敏与 runtime `secrets` |
| 版本化 JSON | `RecordedFlow.version` 与 CLI/Desktop 保存 |
| JSON 可回放 | daemon `recorder.replay`，复用 Page/Element/Actions |
| 使用现有 locator chain | `css:` locator、现有 `Page.find()`，fallback 仍是同一 locator 语义 |
| CLI/Desktop 共用协议 | 两者均通过 daemon recorder RPC，不直接控制 CDP |
| React/Tauri/Core 职责清晰 | React 展示/编辑，Tauri 桥接，Core 录制和回放 |

最终验证命令：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib
npm run build --prefix desktop/openpage
cargo check --manifest-path desktop/openpage/src-tauri/Cargo.toml
```

以上检查均通过；完整 `openpage` 测试集中的若干浏览器运行时测试依赖本机 Chromium/页面环境，出现的失败属于既有运行环境条件，不作为录制模块验收依据。录制模块专用测试为 5 passed。

v1 核心链路已闭环；stop flush、原生文件选择器和 iframe/frame 上下文均已完成。

## 里程碑 15：文档与实现一致性复核

状态：**已完成**

复核发现里程碑 10 的历史记录仍保留了“`wait_after` 回放尚未完成”的旧限制描述；里程碑 12 已完成该功能。以最新里程碑为准，当前 v1 已兑现 navigation wait，旧描述仅作为当时审查时点的历史记录，不再代表当前状态。

当时复核时尚未纳入验收的项目已分别在后续里程碑处理；该列表仅保留用于说明历史审查结论。

## 里程碑 16：桌面端当前 URL 展示

状态：**已完成**

桌面端刷新录制状态时同时调用现有 `webpage.url`，展示当前 OpenPage Chromium 页面地址。没有把 URL 探测放进 React 或新增 recorder 专用接口，继续复用 daemon 已有页面协议。

验证：

```text
npm run build --prefix desktop/openpage  # 通过
```

## 里程碑 17：停止录制时 flush 待处理 CDP 事件

状态：**已完成**

`Recorder::stop()` 不再直接 abort listener：

- 先向 listener task 发送停止信号；
- 在有限窗口内继续消费 binding 和主 frame navigation 队列；
- 完成 drain 后再把 recording 状态置为 false；
- 超过 250ms 才强制终止异常卡住的 listener，避免 stop 永久阻塞。

这样 stop 返回的 flow 会包含停止请求前已经进入 CDP listener 队列的事件，同时保留有界退出保障。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 里程碑 18：Tauri 原生 flow 文件打开与保存

状态：**已完成**

桌面端不再依赖 WebView 下载保存 flow：

- 接入 Tauri Dialog plugin；
- “保存 JSON”打开系统保存对话框，再由 Tauri Rust `save_flow` 写入用户选择路径；
- 新增“打开 JSON”，通过系统打开对话框选择 flow，再由 Tauri Rust `read_flow` 读取；
- 增加 `dialog:default` capability，保持 macOS/Windows/Linux 使用系统对话框的路径。

浏览器下载导出仍保留用于 Python、Rust 和 CLI 示例文件；结构化 flow 的主保存/打开路径已切换为原生桌面能力。

验证：

```text
npm run build --prefix desktop/openpage  # 通过
cargo check --manifest-path desktop/openpage/src-tauri/Cargo.toml  # 通过
```

## 里程碑 19：iframe/frame 录制上下文与链式回放

状态：**已完成**

录制脚本为每个元素目标记录从顶层页面到当前文档的 frame locator 链，使用现有 `css:` locator 表达，不新增定位语法。same-origin iframe 可以得到完整链；跨域 iframe 无法从页面脚本读取 `frameElement` 时保持空链，不伪造上下文。

回放时复用 daemon 已有 `WebFrame` 和 `get_frame_context` 能力，按链逐层切换 frame，再继续使用现有 `find`、click、fill、select、check 和 press 执行路径。没有新增第二套元素执行器。顶层步骤会清除 active frame，避免 frame 状态泄漏到后续步骤。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 当前验收结论

当前源码链路已完成；Windows/Linux 安装包发布矩阵仍必须在对应原生构建机验收，本机不能替代该环境。新 tab 录制与回放已接入现有 tab 能力，并已通过编译与模块测试。

## 里程碑 20：回放兑现 `new_tab` 等待语义

状态：**已完成（回放侧）**

回放遇到 `wait_after: "new_tab"` 时，复用现有 `WebPage::wait_for_new_tab`，以当前 tab target id 建立基线，等待新 tab 出现并调用现有 `activate_tab` 激活。没有新增 tab 管理器或第二套回放执行器。

录制侧的新 tab 自动判定仍不能仅依赖当前页面的 binding/navigation 事件；它需要浏览器 target 生命周期事件或真实页面端到端采集验证。本里程碑只提交已经可验证的回放能力，不把未验证的自动录制宣称为完成。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
```

## 里程碑 21：Windows Tauri 打包前置检查

状态：**配置已补齐，构建环境阻断已明确**

补充 `src-tauri/icons/icon.ico`，解决 Windows `tauri-build` 首个真实错误：缺少 Windows Resource 所需图标。当前 macOS 工作站已完成：

```text
npm run build --prefix desktop/openpage  # 通过
cargo check --manifest-path desktop/openpage/src-tauri/Cargo.toml  # 通过
```

Windows target 检查已实际启动，但当前机器缺少 Windows 资源编译器 `llvm-rc`，因此不能把交叉检查结果冒充 Windows 构建通过。真正的 Windows 安装包验收必须在安装了 WebView2/Windows SDK/`llvm-rc` 的 Windows 构建机执行。

## 里程碑 22：录制侧新 tab 自动识别

状态：**已完成（浏览器连接场景）**

Recorder 在绑定事件与主 frame 导航之外复用现有 `Browser::tab_ids()` 做低频 target 列表观察：

- 录制开始时建立当前 tab 列表基线；
- 发现新增 tab 后，将最近一个已录制步骤标记为 `wait_after: "new_tab"`；
- 回放侧继续复用 `wait_for_new_tab` 和 `activate_tab`；
- 没有新增 tab 管理器、兼容层或第二套录制引擎。

这是事实识别，不猜测新 tab 的业务 URL，也不把新 tab 伪造成额外的 `goto` 步骤。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 里程碑 23：敏感字段识别补强

状态：**已完成**

在原有 password、OTP、token、API key、信用卡字段启发式之外，录制脚本补充识别：

- `cc-number`、卡号缩写和安全码字段；
- credential、auth code、private key 等字段元数据；
- `data-sensitive="true"` 与 `data-secret` 页面显式标记。

仍保持保守策略：识别到的值只写入 `RecordedValue::Secret` 占位符，真实值只能通过 replay 的运行时 `secrets` 注入；无法从 DOM 元数据判断的业务敏感值不会被猜测。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 里程碑 24：跨平台 target 检查复核

状态：**已验证当前工作站可验证范围**

补充验证结果：

```text
PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo check --manifest-path desktop/openpage/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc  # 通过
```

Windows target 已通过 Tauri Rust 交叉检查；此前的 `llvm-rc` 问题已通过使用本机 LLVM 工具链解决。Linux target 检查已启动，但 macOS 工作站没有 Linux GTK/WebKit sysroot，`pkg-config` 无法提供 `gdk-pixbuf`、`atk`、`cairo` 等原生依赖。因此 Linux 安装包仍必须在 Linux 构建机完成，不能把 macOS 交叉检查冒充 Linux 发布验收。

## 里程碑 25：全量 Core 测试审计

状态：**已完成录制范围审计**

执行：

```text
cargo test --manifest-path rust/Cargo.toml -p openpage --lib
```

结果：`713 passed; 24 failed`。失败集中在既有浏览器启动/daemon sidecar/运行时 Chromium 和跨进程环境测试；录制专用测试仍为 `4 passed; 0 failed`，且 Core、daemon、Desktop 的定向编译均通过。全量测试结果已保留，不能将定向测试结果扩大解释为整个 OpenPage 测试集无失败。

## 里程碑 26：新 tab 归一化单元验收

状态：**已完成**

补充 Recorder 单元测试，验证新增 tab 事实到达后只给最近步骤设置 `wait_after: "new_tab"`，不新增伪造动作步骤。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
```

## 里程碑 27：录制启动运行时审查

本轮审查发现，Recorder 启动路径会在 daemon 的 Tokio 异步处理线程中调用 Chromiumoxide 的同步包装 API。直接调用 `runtime.block_on(...)` 或 `Browser::tab_ids()` 会触发 `Cannot start a runtime from within a runtime`，并可能阻塞后续 RPC。

当前修复方向保持最小化：

- 录制初始化阶段，在已有异步 runtime 中使用 `block_in_place` 包裹必要的同步等待；
- `Browser::tab_ids()` 放入现有 Tokio runtime 的 `spawn_blocking`，避免把同步 CDP 等待阻塞 daemon worker；
- 页面 URL 观察继续使用已有异步 `page.url().await`；
- 不新增 Recorder、TabManager、Adapter 或第二套事件系统。

已经验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml -- --check  # 通过
cargo test --manifest-path rust/Cargo.toml -p openpage recorder:: --lib  # 5 passed
CARGO_INCREMENTAL=0 cargo check --manifest-path rust/Cargo.toml -p openpage -p openpage-app  # 通过
```

真实 daemon/CLI 链路已在新进程上复测通过。验证命令使用已构建的 `rust/target/debug/openpage`，避免把 Cargo 编译耗时混入运行验证：

```text
browser start --session recorder-check12 --headless https://example.com  # 返回 session、port、target
record start --session recorder-check12                            # recording=true
record status --session recorder-check12                           # recording=true
goto https://example.org --session recorder-check12                # 返回 loaded=true
record steps --session recorder-check12                            # 返回 goto https://example.org/
record stop --session recorder-check12                             # 返回同一条 goto 步骤
record status --session recorder-check12                           # recording=false, step_count=1
```

daemon 日志没有出现 `Cannot start a runtime from within a runtime`。
