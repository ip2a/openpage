# OpenPage 重构记录

> 本文档记录 OpenPage 顶层模型和 Python 门面重构过程中已经确认的设计决策、待确认问题及术语定义。
>
> 更新日期：2026-07-23

---

## 1. 文档规则

| 状态 | 含义 |
|---|---|
| 已确认 | 已由项目负责人确认，后续设计以此为基础 |
| 待确认 | 尚未形成最终决策，不应提前实现 |
| 已否决 | 明确不采用，避免后续重复讨论 |

每次只确认一个设计问题。确认后将结论写入本文档，再继续下一个问题。

---

## 2. ADR-001：Python 顶层核心模型

| 项目 | 内容 |
|---|---|
| 状态 | **已确认** |
| 决策日期 | 2026-07-23 |
| 作用范围 | Python 公开门面 |

### 2.1 决策

Python 顶层最终保留三个核心概念：

| 类型 | 定位 | 顶层公开 |
|---|---|---:|
| `Browser` | 浏览器进程和多标签页容器 | 是 |
| `Page` | 浏览器中的一个实时页面 | 是 |
| `Session` | HTTP 会话、请求和静态 HTML | 是 |

顶层推荐导入方式：

```python
from openpage import Browser, Page, Session
```

### 2.2 标准浏览器使用方式

```python
from openpage import Browser

browser = Browser.launch()
page = browser.new_page()
page.goto("https://example.com")
```

对象关系：

```text
Browser
└── Page
```

其中：

- `Browser` 管理浏览器进程和多个标签页；
- `Page` 表示浏览器中的一个实时页面；
- `Browser.new_page()` 返回 `Page`；
- `Page` 不再通过不同类名表达 Chromium 页面变体。

### 2.3 快速创建页面

提供顶层便捷入口：

```python
import openpage

page = openpage.open("https://example.com")
```

该入口用于减少快速使用场景中的样板代码。

其内部生命周期、关闭语义和 Browser 所有权仍需单独确认，不能在实现时自行决定。

### 2.4 Session 使用方向

`Session` 表示：

- HTTP 会话；
- GET/POST 等 HTTP 请求；
- Cookie 和请求头状态；
- 静态 HTML 内容和查询能力。

`Session` 不表示浏览器标签页，不继承 `Page`，也不切换为浏览器模式。

### 2.5 设计约束

以下约束已随本决策确定：

1. `Browser`、`Page`、`Session` 是三个不同领域对象；
2. `Page` 专指实时浏览器页面；
3. `Session` 专指 HTTP 会话和静态 HTML；
4. 不让同一个对象在 Browser Page 和 Session 之间运行时切换类型语义；
5. 浏览器多页面管理统一由 `Browser` 负责；
6. 快速入口可以隐藏创建步骤，但不能让对象生命周期变得不可预测。

### 2.6 顶层类型收敛

顶层公开的核心类型最终严格收敛为：

```python
from openpage import Browser, Page, Session
```

| 类型 | 结论 |
|---|---|
| `Browser` | 保留，表示浏览器进程和多页面容器 |
| `Page` | 保留，只表示实时浏览器页面 |
| `Session` | 保留，只表示 HTTP 会话和静态 HTML |
| `ChromiumPage` | 不再作为顶层核心类型 |
| `SessionPage` | 不再作为顶层核心类型，由 `Session` 取代 |
| `WebPage` | 不再作为顶层核心类型，不提供混合模式对象 |

不通过 `XxxPage` 子类型、继承层级或运行时模式切换区分页面能力。具体兼容删除和内部能力迁移后期单独设计。

### 2.7 实时元素与静态快照查询

`Page` 表示整个实时页面，`Element` 表示属于该页面的一个实时 DOM 节点。`Element` 不作为独立页面或独立会话存在。

```text
Browser
  └── Page
        └── Element
              └── Element
```

实时查询统一使用 `find()` 和 `find_all()`：

```python
form = page.find("#login-form")
username = form.find('input[name="username"]')
buttons = form.find_all("button")
```

| 调用 | 返回 | 语义 |
|---|---|---|
| `page.find(selector)` | `Element` | 从整个实时页面查找一个元素 |
| `page.find_all(selector)` | `list[Element]` | 从整个实时页面查找多个元素 |
| `element.find(selector)` | `Element` | 从当前实时元素的子树查找一个元素 |
| `element.find_all(selector)` | `list[Element]` | 从当前实时元素的子树查找多个元素 |

原 `s_ele()`、`s_eles()` 的能力不删除，改由明确的 `snapshot` 入口承接：

```python
page.snapshot.find(".item")
page.snapshot.find_all(".item")

element = page.find("#content")
element.snapshot.find(".item")
```

```text
Page
├── find/find_all         → Element（实时）
└── snapshot              → Snapshot（页面静态快照）
                              └── find/find_all → SnapshotElement

Element
├── find/find_all         → Element（实时）
└── snapshot              → SnapshotElement（当前元素的静态快照）
                              └── find/find_all → SnapshotElement
```

| 旧调用 | 新调用 |
|---|---|
| `page.ele(x)` | `page.find(x)` |
| `page.eles(x)` | `page.find_all(x)` |
| `element.ele(x)` | `element.find(x)` |
| `element.eles(x)` | `element.find_all(x)` |
| `page.s_ele()` | `page.snapshot` |
| `page.s_ele(x)` | `page.snapshot.find(x)` |
| `page.s_eles(x)` | `page.snapshot.find_all(x)` |

命名采用 `snapshot`，不采用 `static`。`snapshot` 明确表示某个时刻冻结的 DOM，不再与实时页面同步。

`Page.snapshot` 与 `Session` 必须保持领域区分：

- `Page.snapshot`：浏览器实时页面在某个时刻的冻结结果；
- `Session`：HTTP 请求及其静态 HTML 内容。

二者可以共享 `find/find_all` 的查询词汇，但不能被设计成同一种顶层对象。`Snapshot`、`SnapshotElement` 属于领域类型，不加入顶层三个核心类型。

### 2.8 Session 与静态 HTML 文档

HTTP 会话和单次请求结果分离：

```text
Session
  └── Response
        └── Document
              └── DocumentElement
```

```python
session = Session()
response = session.get("https://example.com")

response.status_code
response.headers
response.text
response.content
response.json()

document = response.document
title = document.find("title")
links = document.find_all("a")
```

| 类型 | 职责 | 顶层公开 |
|---|---|---:|
| `Session` | 管理 Cookie、请求头、连接状态并发起 HTTP 请求 | 是 |
| `Response` | 表示一次 HTTP 响应及其状态码、响应头和响应体 | 否 |
| `Document` | 表示从 HTML 响应体解析出的静态文档 | 否 |
| `DocumentElement` | 表示静态文档中的元素节点 | 否 |

静态文档统一使用：

```python
document.find(selector)
document.find_all(selector)
element.find(selector)
element.find_all(selector)
```

`Session.get()` 返回独立的 `Response`，不修改并返回 `Session` 自身。`Response` 不直接承担 HTML 查询；HTML 查询通过 `response.document` 进入，以便非 HTML 响应仍保持清晰语义。

```text
Page.snapshot  → Snapshot → SnapshotElement
Session.get()  → Response → Document → DocumentElement
```

快照和 HTTP 文档共享 `find/find_all` 查询词汇，但保持不同的来源和领域类型。`Response`、`Document`、`DocumentElement` 都不加入顶层三个核心类型。

### 2.9 状态、等待与配置门面

状态、等待和配置按以下规则组织：

| 类别 | 设计 |
|---|---|
| 当前状态 | 对象上的直接只读属性 |
| 等待条件 | `wait` 门面 |
| 可修改配置 | `settings` 门面 |
| `states` | 删除 |
| `set` | 删除 |

状态直接属于对象，不增加 `states` 中间层：

```python
page.is_loading
page.is_alive
page.ready_state
page.has_alert

element.is_visible
element.is_enabled
element.is_selected
element.is_clickable
```

元素可见性统一使用 `visible` 词汇，不再使用 `displayed`：

```python
element.is_visible
```

等待操作保留 `wait` 分组，但方法名不重复 `ele`：

```python
page.wait.ready()
page.wait.visible("#submit")
page.wait.hidden(".loading")
page.wait.removed(".dialog")
page.wait.clickable("#submit")

element.wait.visible()
element.wait.hidden()
element.wait.enabled()
element.wait.clickable()
```

配置统一使用 `settings`：

```python
page.settings.headers(...)
page.settings.user_agent(...)
page.settings.blocked_urls(...)
page.settings.download_path(...)
page.settings.load_mode = "eager"

browser.settings.load_mode = "eager"
```

不再公开 `page.states`、`element.states`、`page.set` 等旧门面。具体属性赋值还是方法调用由后续内部 API 细化阶段确认。

### 2.10 顶层公开边界

顶层门面严格限制为：

```python
from openpage import Browser, Page, Session
import openpage

page = openpage.open("https://example.com")
```

```python
__all__ = [
    "Browser",
    "Page",
    "Session",
    "open",
]
```

其他对象不进入顶层，由核心对象返回，并可从领域包显式导入用于类型标注：

```python
from openpage.element import Element
from openpage.snapshot import Snapshot, SnapshotElement
from openpage.http import Response, Document, DocumentElement
from openpage.network import Request, Response, Listener, Interceptor
from openpage.download import Download
```

`openpage_rs`、`_openpage_rs`、`openpage._native` 完全属于内部实现，不进入公开门面。

### 2.11 Browser 职责与核心命名

`Browser` 只负责启动或连接浏览器、创建和管理页面、关闭浏览器。

```python
browser = Browser.launch()
page = browser.new_page()
pages = browser.pages
```

| 能力 | API |
|---|---|
| 启动新浏览器 | `Browser.launch()` |
| 连接已有浏览器 | `Browser.connect(endpoint)` |
| 创建页面 | `browser.new_page(url=None)` |
| 全部页面 | `browser.pages` |
| 当前活动页面 | `browser.active_page` |
| 按 URL 或标题查找页面 | `browser.find_page(...)` |
| 关闭浏览器 | `browser.close()` |

Python 用户侧统一使用 `Page`，不再混用 `tab`。关闭操作统一使用 `close()`，不再提供语义重叠的 `quit()`。集合和当前对象使用属性，不使用 `get_pages()`、`get_tabs()`、`latest_tab` 等名称。

`Browser` 不承担页面元素查询、页面交互或 HTTP 请求职责。

### 2.12 Page 作为页面根级操作门面

`Page` 不被限制为只能导航和查找。它在使用语义上相当于一个特殊的页面根元素，因此可以直接按选择器完成查找、点击、输入和读取。

```python
page.find("#submit")
page.click("#submit")
page.input("#name", "hello")
page.text("#title")
page.attr("#link", "href")
```

这些便捷方法与 `Element` 的能力重复是有意设计，属于公开门面的易用性，而不是需要消除的重复。用户仍可选择显式元素链：

```python
element = page.find("#submit")
element.click()
```

此处确认的是使用语义，不要求 Python 实现通过 `Page(Element)` 继承关系完成；内部结构后期单独设计。

页面和浏览器的基础命名仍按已确认方向统一：

```python
page.goto(url)
browser.new_page(url=None)
```

`page.get()` 不作为浏览器导航主名称，`page.new_tab()` 不作为创建页面主入口。

### 2.13 Page 与 Element 的对称操作词汇

`Page` 和 `Element` 使用同一套核心操作词汇；`Page` 接收选择器，`Element` 直接操作自身。

| 能力 | Page | Element |
|---|---|---|
| 查找一个 | `page.find(selector)` | `element.find(selector)` |
| 查找多个 | `page.find_all(selector)` | `element.find_all(selector)` |
| 点击 | `page.click(selector)` | `element.click()` |
| 输入 | `page.input(selector, value)` | `element.input(value)` |
| 文本 | `page.text(selector)` | `element.text` |
| 属性 | `page.attr(selector, name)` | `element.attr(name)` |

输入操作保留 `input` 命名，不改为 `fill`。

### 2.14 点击操作命名

删除可调用对象式的 `click` 代理。点击统一使用普通方法：

```python
element.click()
element.click(button="right")
element.double_click()
element.click_at(x=10, y=20)
element.click(count=3)
```

`Page` 保持相同词汇，但额外接收选择器：

```python
page.click("#submit")
page.double_click("#submit")
page.click_at("#submit", x=10, y=20)
```

中键、右键通过 `button` 参数表达；双击使用明确的 `double_click()`；多次点击使用 `count` 参数。不再使用 `click.left()`、`click.middle()`、`click.right()`、`click.multi()`、`click.at()`。

### 2.15 选择器参数与定位规则

所有查询和元素便捷操作统一使用 `selector` 参数名，不公开 `Locator` 对象，也不拆分 `find_css()`、`find_xpath()`。

```python
page.find(selector)
page.find_all(selector)
page.click(selector)
page.input(selector, value)
page.text(selector)
page.attr(selector, name)
element.find(selector)
```

普通字符串默认按 CSS 选择器处理；显式类型使用前缀：

```python
page.find("#login")
page.find("css:.item")
page.find("xpath://button[@type='submit']")
```

Page、Element、Snapshot、Document 及等待门面共享同一套选择器规则。

---

## 3. 现有类型迁移状态

以下迁移尚未逐项确认，仅记录当前讨论背景：

| 当前类型 | 目标方向 | 状态 |
|---|---|---|
| `Browser` | 保留为浏览器容器 | 已确认 |
| `Page` | 保留为实时浏览器页面 | 已确认 |
| `SessionPage` | 调整为 `Session` | 原则已确认，具体 API 待确认 |
| `ChromiumPage` | 不再作为核心页面类型 | 待确认迁移和废弃方式 |
| `WebPage` | 不再作为顶层核心类型 | 待确认能力迁移和废弃方式 |

---

## 4. 待确认设计树

当前阶段只确认顶层命名、对象分类和公开架构；生命周期及内部实现调整推迟到架构确定之后。

以下问题必须逐项确认，不提前实现：

| 顺序 | 问题 | 状态 |
|---:|---|---|
| 1 | 顶层类型最终只保留 `Browser`、`Page`、`Session` | 已确认 |
| 2 | 实时元素查询统一为 `find/find_all`，静态查询通过 `snapshot` 进入 | 已确认 |
| 3 | 静态 HTML 使用 `Session → Response → Document → DocumentElement` | 已确认 |
| 4 | 状态使用直接属性，等待使用 `wait`，配置使用 `settings` | 已确认 |
| 5 | 顶层只公开 `Browser`、`Page`、`Session`、`open()` | 已确认 |
| 6 | `Browser` 只负责浏览器生命周期和页面管理，统一使用 Page/close 命名 | 已确认 |
| 7 | `Page` 作为页面根级操作门面，保留 click/input/text/attr 等选择器便捷方法 | 已确认 |
| 8 | `Page` 与 `Element` 使用对称的 find/click/input/text/attr 词汇 | 已确认 |
| 9 | 点击使用普通 `click/double_click/click_at` 方法，不使用可调用 click 代理 | 已确认 |
| 10 | 统一使用 `selector` 参数；默认 CSS，显式支持 `css:` / `xpath:` 前缀 | 已确认 |
| 11 | 元素树关系 `parent/children/prev/next/before/after` 命名 | 本轮不改 |
| 6 | `ChromiumPage`、`SessionPage`、`WebPage` 直接删除，能力按新 Rust 领域模型重建 | 已确认 |
| 7 | `Session.get()` 的返回对象设计 | 后期确认 |
| 8 | 未找到、超时和失败的统一返回/异常语义 | 后期确认 |
| 9 | `openpage.open()` 创建的 Browser 所有权和关闭语义 | 后期确认 |

## 4.1 第一批实施边界

元素树关系命名本轮保持现状，不调整 `parent/children/prev/next/before/after`。

第一批重构只实施已经确认的顶层类型、领域对象、查询命名、快照入口、状态/等待/配置门面、Browser/Page/Element 核心词汇、点击方法和选择器规则。生命周期、异常语义以及元素树关系留到后续单独确认；旧 API 已确认直接删除，不保留迁移兼容层。

## 4.2 强制重构原则

### 4.2.1 不保留兼容、废弃和回退代码

本次重构采用直接切换，不维护新旧 API 双轨：

- 旧类型、旧方法和旧属性被新设计替代后直接删除；
- 不提供 deprecated alias；
- 不提供兼容包装；
- 不提供 fallback 或本地动态库扫描回退；
- 不保留仅用于旧 API 的分支、转换层和转发层；
- 不以 `runtime`、`helper`、`adapter`、`compat` 等概念组织代码；
- 不用“编排、兼容、适配”作为公开架构或内部模块的主要职责。

涉及但不限于：

```text
ChromiumPage / SessionPage / WebPage
ele / eles / s_ele / s_eles
states / set
click.left / click.middle / click.right / click.multi / click.at
quit / get_tab / get_tabs / new_tab 等被新门面替代的名称
```

删除旧 API 时，应同时删除由本次改动产生的无用导出、类型标注、测试和内部实现，不留下假兼容入口。

### 4.2.2 Rust 是领域设计的实现源头

这次工作不是只修改 Python 包装层。Python API 表达的是最终使用习惯，核心领域模型、职责边界和门面逻辑必须先在 Rust 侧成立。

实施顺序固定为：

```text
领域设计确认
  → Rust 核心类型与能力边界调整
  → Rust Python binding 按新模型导出
  → Python 门面自然映射 Rust 能力
  → 测试与文档同步更新
```

Rust 侧同样遵守已经确认的模型：

```text
Browser
Page
Session → Response → Document → DocumentElement
Page/Element → Snapshot/SnapshotElement
```

具体要求：

1. Rust 不继续维护 `ChromiumPage`、`SessionPage`、`WebPage` 这样的重叠领域模型；
2. Rust 的 `Browser`、`Page`、`Session` 必须具有与公开门面一致的职责边界；
3. `find/find_all`、`snapshot`、状态、等待、配置和点击语义应在 Rust 能力层稳定，而不是由 Python 拼接出另一套模型；
4. Python binding 只负责可靠地暴露 Rust 能力和完成必要的 Python 类型转换；
5. Python 不能通过大量包装、转发或动态判断修补 Rust 侧不清晰的设计；
6. 如果某项能力需要复杂 Python 编排才能成立，应优先回到 Rust 重新划分职责；
7. Rust 模型稳定后，再完成 Python 命名、类型标注和用户体验层的收敛。

因此，本次重构的验收标准不是“Python 看起来像新 API”，而是：

> Rust 核心、Python binding 和 Python 门面表达同一套领域模型，且不存在旧模型兼容层。

---

## 5. 术语表

| 术语 | 定义 |
|---|---|
| Browser | 一个浏览器进程及其标签页容器 |
| Page | Browser 中的一个实时浏览器页面 |
| Session | 持有 Cookie、请求头等状态的 HTTP 会话 |
| 顶层门面 | 可以直接通过 `from openpage import ...` 使用的核心对象 |
| 领域门面 | 通过 `openpage.browser`、`openpage.network` 等领域包使用的对象 |
| 实时页面 | 由浏览器驱动、拥有实时 DOM、可执行点击和输入的页面 |
| 静态 HTML | HTTP 响应或页面快照解析得到的非实时文档内容 |

---

## 6. 已否决方向

| 方向 | 状态 | 原因 |
|---|---|---|
| 用 `runtime`、`adapter`、`helper`、`compat` 组织公开包结构 | 已否决 | 这些名称表达内部实现方式，不表达产品领域 |
| 恢复本地动态库扫描 fallback | 已否决 | 发布包必须正常依赖并导入 `openpage_rs` |
| 继续使用中央 `_compat.py` 承载全部 Python API | 已否决 | 职责和领域边界不清晰 |

---

## 7. 执行进度

### 里程碑 0：移除中央兼容层并完成 Python 领域拆包（2026-07-23）

状态：已完成。

完成内容：

- 删除 `python/openpage/_compat.py`；
- 删除本地 Rust 动态库扫描 fallback，原生模块改为正常导入；
- 将 Python 代码从中央兼容文件拆入 browser、page、element、network、download、console、keyboard、options、`_native` 领域目录；
- 将兼容命名测试文件改为领域命名；
- 此里程碑只完成旧中央文件的拆除，为后续 Rust 领域模型重构建立可工作的源码基线，不代表旧公开类型已经保留。

验证：

```text
cd python && uv run python -m compileall -q openpage
cd python && uv run python ../tests/python/test_download_wait.py
89 tests passed
```

### 里程碑 1：删除 Rust 顶层历史类型别名（2026-07-23）

状态：已完成。

Rust crate 顶层已直接删除以下历史别名，不提供兼容：

```text
Chromium
ChromiumPage
ChromiumTab
ChromiumElement
ChromiumFrame
ChromiumOptions
MixTab
NoneElement
SessionNoneElement
WebNoneElement
```

同时删除只用于证明这些兼容别名存在的测试。核心真实类型保持 `Browser`、`Page`、`Element`、`Frame`、`LaunchOptions` 等正式名称。

验证：

```text
cargo check -p openpage --lib
Rust 顶层公开类型定向测试通过
```

### 里程碑 2：Rust HTTP 核心从 SessionPage 收敛为 Session（2026-07-23）

状态：已完成。

完成内容：

- Rust 核心类型 `SessionPage` 直接重命名为 `Session`；
- `SessionPageSetter` 重命名为 `SessionSettings`；
- `session/page.rs` 重命名为表达 HTTP 请求职责的 `session/request.rs`；
- Rust 核心内所有调用方同步切换，不保留类型别名或旧名称转发；
- 页面快照、tools 及待删除 WebPage 内部临时调用已统一引用真实 `Session` 类型。

验证：

```text
cargo check -p openpage --lib
session::snapshot::tests::snapshot_find_supports_nested_queries
session::snapshot::tests::session_options_uses_request_pipeline_and_updates_response_snapshot
Rust 核心中 SessionPage / SessionPageSetter 搜索结果为 0
```

### 里程碑 3：Rust Session 配置入口收敛为 settings（2026-07-23）

状态：已完成。

完成内容：

- Rust `Session.set()` 直接替换为 `Session.settings()`，不保留旧方法；
- `SessionSettings` 内部字段从错误的 `page` 语义修正为 `session`；
- 对应 Rust 测试改用正式领域命名，不保留 setter 旧术语。

验证：

```text
cargo fmt --all --check
cargo check -p openpage --lib
cargo test -p openpage --lib session::snapshot::tests::session_settings_accept_supported_values
```

### 里程碑 4：Rust Session 删除请求适配层概念（2026-07-23）

状态：已完成。

完成内容：

- 删除 Rust Session 的 `SessionAdapter`、`SessionAdapterMount` 和按 URL 选择客户端的分支；
- 删除 `SessionOptions`、`SessionRequestOptions`、`SessionSettings`、`Session` 和运行时快照中的对应入口；
- 请求统一使用 Session 自身的 HTTP 客户端配置；
- 删除仅覆盖该旧能力的测试，不保留兼容名称或空实现。

验证：

```text
cargo fmt --all
cargo check -p openpage --lib
cargo test -p openpage --lib session::snapshot::tests::session_settings_accept_supported_values
```

### 里程碑 5：Rust 静态文档元素命名收敛（2026-07-23）

状态：已完成。

完成内容：

- Rust 核心及其调用方中的 `SessionElement` 直接重命名为 `DocumentElement`；
- 静态 HTTP 文档元素不再使用 Session 页面术语；
- 同步更新页面、元素列表、ShadowRoot、工具和 WebPage 内部类型引用，不保留旧名称别名。

说明：本里程碑完成术语收敛；`Document` 独立拥有的响应模型仍在下一里程碑实现。

验证：

```text
cargo fmt --all
cargo check -p openpage --lib
cargo test -p openpage --lib session::snapshot::tests::session_settings_accept_supported_values
Rust 核心 SessionElement 搜索结果为 0
```

### 里程碑 6：Rust Session 请求返回 Response 并进入 Document（2026-07-23）

状态：阶段完成。

完成内容：

- `Session.get/head/options/post/put/delete/patch` 及 JSON、表单、请求体变体统一返回 `Response`；
- 新增 Rust `Response`，独立保存 URL、状态码、响应头、编码、字节内容和文本内容；
- 新增 Rust `Document`，通过 `response.document()` 进入静态 HTML 查询；
- `Document.find/find_all` 统一使用静态文档元素模型 `DocumentElement`；
- 删除 WebPage 内部对 Session 请求返回值的旧布尔假设，改为显式检查 `Response.is_success()`。

本阶段尚未删除 Session 上历史的最后响应查询方法；该删除将在 Response 行为验证后单独完成，不保留兼容入口。

验证：

```text
cargo fmt --all
cargo check -p openpage --lib
cargo test -p openpage --lib session::snapshot::tests::session_get
cargo test -p openpage --lib session::snapshot::tests::session_request_returns_owned_response_with_document
```

### 里程碑 7：重构总原则确认——删除废弃层，Rust 先行（2026-07-23）

状态：原则已确认，持续执行中。

本次重构不是一次 Python 端的表面 API 整理，而是从 Rust 核心领域模型开始，逐层收敛到统一门面：

```text
Rust Core
→ PyO3 binding
→ Python facade
→ tests / docs
```

#### 1. 废弃与兼容代码直接删除

- 已废弃的类型、方法、别名、分支和测试直接移除；
- 不保留兼容入口、回退路径、空实现、转发别名或行为兼容层；
- 不以 `runtime`、`helper`、`adapter`、`compat` 等概念组织产品架构；
- 底层依赖自身的技术实现（例如 Tokio 的异步运行时）不等同于产品架构概念，不因名称机械删除；
- 如果旧测试只验证已经删除的语义，应删除或改写为新领域模型测试，而不是恢复旧 API。

#### 2. Rust 是门面设计和逻辑的源头

- `Browser`、`Page`、`Session` 及其响应、文档、元素模型首先在 Rust 中建立清晰边界；
- Rust 核心稳定后，PyO3 只负责直接暴露该模型，Python 只提供符合 Python 使用习惯的薄门面；
- Python 不负责通过厚重包装修补 Rust 设计缺陷，也不重新发明一套与 Rust 不一致的对象模型；
- 每次公共 API 调整都按 Rust 核心、绑定、Python、测试和文档的顺序完成；
- 任何旧概念如果在 Rust 核心被删除，绑定和 Python 不得继续以别名或回退形式保留。

#### 当前执行边界

本阶段已删除 `SessionHandle` 及其共享“最后一次响应”相关入口和测试；Session 请求返回独立的 `Response`，静态内容通过 `Response.document()` 进入 `Document`。后续继续按上述顺序删除 `WebPage` 等旧复合模型，并完成 Snapshot、PyO3 和 Python 门面的统一。

验证：

```text
cargo fmt --all
cargo check -p openpage --lib
cargo test -p openpage --lib session::snapshot::tests::session_get --no-fail-fast
```

### 里程碑 8：PyO3 绑定切换为新核心门面（2026-07-23）

状态：阶段完成。

完成内容：

- 删除 `legacy_full_binding` 目录及其旧类型绑定；
- 新建 `binding` 模块，直接暴露 `Browser`、`Page`、`Session`、`Element`、`Response`、`Document`、`DocumentElement`；
- PyO3 不再导出 `SessionPage`、`SessionElement`、`WebPage` 等旧公开类型；
- Session 请求绑定返回 `Response`，不再返回布尔值；
- Page 增加 `snapshot()`，在 Rust 中先冻结当前 HTML，再交给 Document 查询；
- Python 绑定保持最小直接映射，不建立兼容、适配或回退层。

验证：

```text
cargo fmt --all
cargo check -p openpage --lib
cargo check -p openpage-python
```

### 里程碑 9：Python 顶层门面收敛（2026-07-23）

状态：阶段完成。

Python 顶层现在只公开四个入口：

| 名称 | 语义 |
|---|---|
| `Browser` | 浏览器进程和多标签页容器 |
| `Page` | 浏览器中的实时页面 |
| `Session` | HTTP 会话和静态请求 |
| `open()` | 快速创建浏览器页面的便捷函数 |

使用方式：

```python
from openpage import Browser, Page, Session, open

browser = Browser.launch()
page = browser.new_page()
page.goto("https://example.com")

response = Session().get("https://example.com")
document = response.document
title = document.find("title")
```

已删除 Python 端旧页面分类和薄封装堆叠：

```text
ChromiumPage
SessionPage
WebPage
SessionElement
s_ele / s_eles
options / states / wait / settings / window 的旧门面模块
```

Page 保留直接便利操作：

```python
page.click("#submit")
page.input("#name", "hello")
page.text("#title")
page.attr("#link", "href")
```

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check -p openpage-python --manifest-path rust/Cargo.toml
```

### 里程碑 10：Rust 顶层门面移除 WebPage 导出（2026-07-23）

状态：阶段完成。

- `webpage` 不再作为 Rust crate 的公开模块；
- `WebPage`、`WebElement`、`WebFrame`、`WebMode` 及相关 setter/wait 类型不再从 `openpage` 顶层导出；
- PyO3 和 Python 也不再依赖该复合模型；
- 当前 `webpage` 仅作为待删除的内部迁移遗留，后续按依赖链逐步删除，不增加新的调用方。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check -p openpage --lib --manifest-path rust/Cargo.toml
cargo check -p openpage-python --manifest-path rust/Cargo.toml
```

### 里程碑 11：Page 删除旧 s_ele / s_eles 入口（2026-07-23）

状态：阶段完成。

- Rust `Page::s_ele()` 和 `Page::s_eles()` 已直接删除；
- Page 的静态查询入口统一进入 `Page::snapshot()`，再使用 `Document.find()` / `Document.find_all()`；
- 不保留旧方法别名，不增加新的转发层。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check -p openpage --lib --manifest-path rust/Cargo.toml
```

### 里程碑 12：删除 dp 兼容 CLI（2026-07-23）

状态：阶段完成。

- 删除 `dp` 兼容二进制及其 Cargo 声明；
- 删除 `CompatCli`、兼容参数解析和 `dp` 模式判断；
- 删除浏览器路径写入、配置复制和兼容启动分支；
- 删除全部 `dp` 兼容测试和帮助文案；
- OpenPage CLI 只保留正式的 `openpage` 命令入口；
- 同步移除应用 crate 对已隐藏 `webpage` 模块的重新导出，恢复应用层编译边界；
- 不保留别名、转发、fallback 或兼容提示入口。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check -p openpage-app --manifest-path rust/Cargo.toml
cargo test -p openpage-app --manifest-path rust/Cargo.toml cli::tests --lib
```

### 里程碑 13：Rust Core 删除 WebPage 复合模型（2026-07-23）

状态：阶段完成。

本阶段从 Rust 核心删除旧的混合页面模型，不再让 Rust 通过 `WebPage` 同时承载浏览器页面和 HTTP Session 语义。

已完成：

- 删除 `rust/crates/openpage/src/webpage/` 整个旧模块；
- 删除 Rust 中 `WebPage`、`WebElement`、`WebFrame`、`WebMode` 及其相关目标变体；
- `BrowserTabReference` 只保留 `Page` 和目标 ID，不再返回旧页面包装类型；
- daemon 页面服务统一持有 `Page`，创建流程直接使用 `Browser::launch()` 和 `Browser::new_page()`；
- 删除 daemon 中旧 Session 模式、页面内 HTTP 请求和 Cookie 双向转换入口；
- 删除仅服务旧复合模型的 ElementList 扩展、设置消息和测试；
- 删除旧 WebPage 测试，不以恢复旧 API 的方式维持测试通过；
- daemon 协议统一使用 `page.*` 页面语义，不保留 `webpage.*` 兼容入口。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check -p openpage --lib --manifest-path rust/Cargo.toml
cargo check -p openpage-app --manifest-path rust/Cargo.toml
cargo check -p openpage-python --manifest-path rust/Cargo.toml
cargo test -p openpage --lib --no-run --manifest-path rust/Cargo.toml
```

验收结果：上述命令通过。Rust Core 现在继续沿 `Browser → Page` 与 `Session → Response → Document` 两条清晰领域路径演进，Python binding 不再需要为 `WebPage` 提供兼容导出。

### 里程碑 14：确立 Rust-first 门面重构原则（2026-07-23）

状态：设计原则确认，作为后续 Rust 与 Python 改造的共同约束。

本次确认的重点不是单独整理 Python，而是先从 Rust Core 建立正式的领域模型、公开门面和行为边界，再让 Python 通过 PyO3 直接暴露这套已经稳定的 Rust 设计。Python 的目标是符合 Python 使用习惯的薄门面，但 Python 不负责重新定义领域模型，也不负责弥补 Rust 的设计缺口。

#### 一、废弃与兼容代码的处理原则

- 兼容代码直接删除，不保留兼容入口、兼容别名、兼容转发和兼容提示。
- 废弃代码直接删除，不保留 deprecated API 供过渡使用。
- fallback 直接删除，不以回退路径掩盖正式实现的问题。
- 不建立或保留 `runtime`、`helper`、`adapter`、`compat` 等产品架构概念。
- 不通过额外编排层、包装层或动态分派层拼接旧模型与新模型。
- 旧测试如果只验证已删除的模型，应删除或按正式新模型重写；不得为了让旧测试通过而恢复旧 API。
- 每一行新增代码都必须属于正式领域模型、正式公开门面或必要的语言绑定；不能为历史调用方服务。

#### 二、Rust 与 Python 的职责关系

```text
Rust Core
  ├── 定义领域对象
  ├── 定义对象关系
  ├── 定义行为和状态
  ├── 定义错误边界
  └── 定义正式公开 API
          ↓ PyO3 直接绑定
Python
  ├── 使用 Python 命名和调用习惯
  ├── 暴露少量顶层入口
  └── 不重新实现 Rust 领域逻辑
```

PyO3 中为了持有 Rust 对象而存在的 Python 类型容器不属于产品层的 `adapter` 或 `helper`。但是，这些绑定类型不得额外复制业务规则、兼容旧接口或引入第二套对象关系。

#### 三、正式顶层领域模型

```text
Browser
└── Page

Session
└── Response
      └── Document
            └── DocumentElement
```

Python 顶层入口保持：

```python
from openpage import Browser, Page, Session, open
```

其中：

| 对象 | 所属领域 | 核心职责 | 顶层公开 |
|---|---|---|---|
| `Browser` | 浏览器 | 浏览器进程和多标签页容器 | 是 |
| `Page` | 浏览器 | 一个实时浏览器页面，同时提供页面级查询和操作便利方法 | 是 |
| `Session` | HTTP | HTTP 会话、请求和静态 HTML 获取 | 是 |
| `Response` | HTTP | 一次 HTTP 请求的结果 | 作为返回对象 |
| `Document` | 静态文档 | Response 内容解析后的文档查询入口 | 作为返回对象 |
| `DocumentElement` | 静态文档 | 静态文档中的元素及其查询能力 | 作为返回对象 |

`Page` 是实时页面，同时是一个具有特殊能力的页面级元素入口。因此保留：

```python
page.find(selector)
page.find_all(selector)
page.click(selector)
page.input(selector, text)
page.text(selector)
page.attr(selector, name)
```

`Page` 找到的实时元素继续使用：

```python
element = page.find("#content")
element.find("a")
element.find_all("li")
element.click()
```

静态内容不再使用 `s_ele` / `s_eles` 这种历史缩写。静态语义由明确的快照/文档对象承接，再继续 `find` / `find_all` 查询。具体采用 `Document` 还是单独命名为 `Snapshot`，必须以 Rust Core 的领域语义统一后再同步 Python，不在 Python 层单独制造第二套命名。

#### 四、后续实施顺序

1. 先在 Rust Core 删除旧模型、旧命名和旧兼容路径；
2. 在 Rust Core 确认 `Browser → Page`、`Session → Response → Document` 的对象关系和方法语义；
3. 为 Rust 正式门面补最小必要测试；
4. 通过 PyO3 直接绑定 Rust 正式类型和行为；
5. Python 仅保留符合 Python 习惯的顶层导入和 `open()` 便捷入口；
6. 最后从 Python 侧验证调用体验，不反向要求 Rust 恢复旧 API。

后续每个里程碑必须同时完成：实际代码改动、匹配范围的验证、本文档记录和 Git 提交。最终验收必须执行全仓审计，确认旧兼容、废弃、fallback、旧复合页面模型以及 `runtime` / `helper` / `adapter` 等架构概念没有以代码、导出、协议或测试形式残留。

### 里程碑 15：Rust Core 删除 `s_ele` / `s_eles`（2026-07-23）

状态：阶段完成。

- 删除 Session、DocumentElement、浏览器 Element、Frame、ShadowRoot 和 ElementList 上的 `s_ele` / `s_eles` 方法；
- 静态查询统一使用 `snapshot_find` / `snapshot_find_all`；
- Session 文档查询统一使用 `find` / `find_all`；
- 没有保留旧别名、兼容转发或废弃入口；
- 同步修改 Rust 内部类型检查测试，测试正式命名而不是旧命名。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo check -p openpage --lib --manifest-path rust/Cargo.toml
cargo test -p openpage --lib --no-run --manifest-path rust/Cargo.toml
rg -n '\\bs_ele\\b|\\bs_eles\\b' rust python --glob '*.rs' --glob '*.py'
```

验收结果：Rust Core 和 Python 源码中不再存在 `s_ele` / `s_eles` 方法或调用。

### 里程碑 16：打通 Python 发布包的真实导入链路（2026-07-23）

状态：阶段完成。

- Python 门面改为从同一包内导入 Rust 扩展：`from .openpage_rs import Browser, Page, Session`；
- 根 `pyproject.toml` 将扩展模块命名为 `openpage.openpage_rs`，避免混合项目构建时寻找不存在的 `python/openpage_rs` 源码包；
- 顶层 Python 公开面仍只保留 `Browser`、`Page`、`Session` 和 `open`；
- 没有增加 Python 业务包装类、兼容层或回退路径。

验证：

```text
uv build --wheel
独立临时虚拟环境安装 dist/openpage-*.whl
import openpage
import openpage.openpage_rs
```

验收结果：从 wheel 安装后可以导入 Python 门面和 Rust 扩展，`Browser`、`Page`、`Session` 均可见。

### 里程碑 17：删除 CLI 中无效的旧测试入口（2026-07-23）

状态：阶段完成。

- 删除未被正式代码调用的 `rpc_webpage_existing` 测试专用函数；
- 删除与正式 `rpc_webpage` 测试重复的旧测试块；
- 不保留无效的旧命名和死代码，只保留当前正式请求路径。

验证：

```text
cargo fmt --all --manifest-path rust/Cargo.toml
cargo test -p openpage-app --lib --no-run --manifest-path rust/Cargo.toml
```
