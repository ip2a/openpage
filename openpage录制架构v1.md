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

说明：daemon 全量测试中已有与本次改动无关的 sidecar/临时目录测试失败；录制模块测试和 crate 编译通过。后续增加专门的 daemon recorder 协议测试，避免依赖真实浏览器启动。

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

说明：当前尚未把 secret 的运行时注入纳入 v1；后续可以在 RPC 参数中增加显式 `secrets` 映射，但必须保持 flow 文件本身不保存真实密码。

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

桌面端没有复制浏览器控制逻辑，也没有把浏览器嵌入 Tauri WebView。桌面端当前连接 `default` session；后续只需把 session 选择加入 UI，不改变协议和 Core。

验证证据：

```text
cd desktop/openpage
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

下一步：补充跨平台打包验收、session 选择和真实桌面运行验收；这些不属于 Core 录制协议的必要前置条件。
