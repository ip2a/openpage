# OpenPage Python 门面技术文档

> 文档对象：`python/openpage` 与 `rust/bindings/python`
>
> 当前状态：Rust-first 门面已收敛；Python 不保留旧复合页面模型、兼容别名或回退导入。
>
> 文档日期：2026-07-23

## 1. 设计原则

| 原则 | 结论 |
|---|---|
| 行为来源 | Rust 核心是唯一领域模型和行为来源 |
| Python 职责 | 只暴露 Rust 类型、转换参数和返回值、提供 Python 调用方式 |
| 顶层模型 | `Browser`、`Page`、`Session` |
| 快速入口 | `open(url)` 创建浏览器并返回 `Page` |
| 查询命名 | 实时对象使用 `find` / `find_all`；静态内容使用 `snapshot` 或 `document` |
| 旧模型 | 直接删除，不保留兼容、别名、回退或废弃入口 |
| 产品术语 | 不使用 `runtime`、`helper`、`adapter`、`compat` 组织 Python 门面 |

## 2. 当前目录

```text
python/openpage/
├── __init__.py
├── openpage_rs.<platform>.so   # wheel 中的 PyO3 扩展
└── py.typed

rust/bindings/python/src/
├── lib.rs
└── binding/mod.rs
```

Python 包不是第二套浏览器实现。`binding/mod.rs` 中的 `Py*` 类型只是 PyO3 对 Rust 领域对象的公开容器，不复制领域逻辑。

## 3. 顶层公开表面

当前 `python/openpage/__init__.py` 的公开表面：

| 名称 | 类型 | 是否顶层公开 | 作用 |
|---|---|---:|---|
| `Browser` | Rust `Browser` 的 Python 门面 | 是 | 浏览器进程和页面容器 |
| `Page` | Rust `Page` 的 Python 门面 | 是 | 一个实时浏览器页面 |
| `Session` | Rust `Session` 的 Python 门面 | 是 | HTTP 会话和静态 HTML |
| `open` | 函数 | 是 | 快速启动浏览器并创建页面 |

正式导入：

```python
from openpage import Browser, Page, Session, open
```

不再公开：

| 删除名称 | 删除原因 |
|---|---|
| `ChromiumPage` | `Page` 已经表示浏览器实时页面，不需要实现变体名称 |
| `SessionPage` | HTTP 会话由 `Session` 表示，不伪装成浏览器页面 |
| `WebPage` | 删除混合 Browser/Session 领域对象 |
| `WebElement` | 统一为 `Element` |
| `s_ele` / `s_eles` | 静态查询改用 `snapshot` 或 `document.find` |
| `openpage_rs` 回退模块 | wheel 必须正常安装并导入原生扩展 |

## 4. Browser 与 Page

### 4.1 对象关系

```text
Browser
└── Page
    └── Element
```

| 对象 | 职责 |
|---|---|
| `Browser` | 启动、持有和关闭浏览器；创建页面 |
| `Page` | 导航、实时 DOM 查询和页面操作 |
| `Element` | 页面中的实时 DOM 节点及其子树操作 |

### 4.2 Browser API

| API | 返回 | 语义 |
|---|---|---|
| `Browser.launch()` | `Browser` | 启动浏览器 |
| `browser.new_page(url=None)` | `Page` | 创建页面，可选初始 URL |
| `browser.close()` | `None` | 关闭浏览器 |

### 4.3 Page API

| API | 返回 | 语义 |
|---|---|---|
| `page.goto(url)` | `None` | 导航到 URL |
| `page.find(locator)` | `Element` | 查找一个实时元素 |
| `page.find_all(locator)` | `list[Element]` | 查找多个实时元素 |
| `page.snapshot()` | `Document` | 获取当前页面静态快照 |
| `page.click(locator)` | `None` | 点击匹配元素 |
| `page.input(locator, text)` | `None` | 向匹配元素输入文本 |
| `page.text(locator)` | `str \| None` | 读取匹配元素文本 |
| `page.attr(locator, name)` | `str \| None` | 读取匹配元素属性 |

便捷操作保留重复能力，这是有意的门面设计：

```python
page.click("#submit")
page.input("#name", "hello")
page.text("#title")
page.attr("#link", "href")
```

### 4.4 Element API

| API | 返回 | 语义 |
|---|---|---|
| `element.find(locator)` | `Element` | 在当前元素子树中查找一个元素 |
| `element.find_all(locator)` | `list[Element]` | 在当前元素子树中查找多个元素 |
| `element.click()` | `None` | 点击当前元素 |
| `element.input(text)` | `None` | 向当前元素输入文本 |
| `element.text()` | `str \| None` | 读取当前元素文本 |
| `element.attr(name)` | `str \| None` | 读取当前元素属性 |

## 5. 静态查询

### 5.1 Page 快照

```python
snapshot = page.snapshot()
item = snapshot.find(".item")
items = snapshot.find_all(".item")
```

`Page` 的快照是某一时刻冻结的页面结果，不再与实时页面同步。

### 5.2 Session 响应文档

```python
session = Session()
response = session.get("https://example.com")
document = response.document
heading = document.find("h1")
links = document.find_all("a")
```

`Session` 的领域关系：

```text
Session
└── Response
    └── Document
        └── DocumentElement
```

## 6. Session API

| API | 返回 | 语义 |
|---|---|---|
| `Session()` | `Session` | 创建 HTTP 会话 |
| `session.get(url)` | `Response` | 发起 GET 请求 |
| `session.post(url)` | `Response` | 发起 POST 请求 |

### 6.1 Response API

| API | 返回 | 语义 |
|---|---|---|
| `response.url` | `str \| None` | 最终响应 URL |
| `response.status_code` | `int \| None` | HTTP 状态码 |
| `response.text` | `str` | 响应文本 |
| `response.content` | `bytes` | 原始响应内容 |
| `response.is_success()` | `bool` | 是否为成功状态 |
| `response.document` | `Document` | 响应正文文档 |

### 6.2 Document API

| API | 返回 | 语义 |
|---|---|---|
| `document.html` | `str` | 文档 HTML |
| `document.find(locator)` | `DocumentElement` | 查找一个静态元素 |
| `document.find_all(locator)` | `list[DocumentElement]` | 查找多个静态元素 |

### 6.3 DocumentElement API

| API | 返回 | 语义 |
|---|---|---|
| `element.find(locator)` | `DocumentElement` | 在静态元素子树中查找 |
| `element.find_all(locator)` | `list[DocumentElement]` | 查找多个静态子元素 |
| `element.text()` | `str \| None` | 读取文本 |
| `element.html()` | `str \| None` | 读取 HTML |
| `element.attr(name)` | `str \| None` | 读取属性 |

## 7. 推荐使用方式

```python
from openpage import Browser, Session, open

# 浏览器页面
browser = Browser.launch()
page = browser.new_page()
page.goto("https://example.com")
print(page.text("h1"))
browser.close()

# HTTP 会话
session = Session()
response = session.get("https://example.com")
print(response.document.find("title").text())

# 快速入口
page = open("https://example.com")
```

## 8. 验证状态

已验证：

```text
cargo check --workspace --manifest-path rust/Cargo.toml
cargo test --workspace --manifest-path rust/Cargo.toml -- --test-threads=1
bash scripts/test/check_all.sh
uv build --wheel
python tests/python/test_facade.py -v
```

正式 Python 门面测试只验证当前产品表面，不伪造原生模块，也不恢复旧 API 测试。
