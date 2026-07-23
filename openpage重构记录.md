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
