# OpenPage Python 门面技术文档

> 文档对象：`python/openpage`
>
> 当前状态：领域目录已拆分，中央 `_compat.py` 已删除。
>
> 文档日期：2026-07-23

---

## 1. 文档目的

本文档说明 OpenPage 当前 Python 端的：

- 包目录结构；
- 对外支持的 Python 门面；
- 顶层导入方式；
- 各门面的主要属性和方法；
- 当前代码审查结果；
- 当前测试验证状态；
- 后续需要收口的问题。

Python 端不实现浏览器核心能力。浏览器、页面、元素、下载、事件等底层能力由 `openpage_rs` 提供，Python 包负责提供稳定、易用的 Python API。

---

## 2. 当前目录结构

```text
python/openpage/
├── __init__.py
│
├── _native/
│   ├── __init__.py
│   └── openpage_rs.py
│
├── options/
│   ├── __init__.py
│   ├── chromium.py
│   └── session.py
│
├── browser/
│   ├── __init__.py
│   ├── browser.py
│   ├── wait.py
│   ├── states.py
│   └── settings.py
│
├── page/
│   ├── __init__.py
│   ├── page.py
│   ├── chromium.py
│   ├── session.py
│   ├── web.py
│   ├── wait.py
│   ├── states.py
│   ├── settings.py
│   └── window.py
│
├── element/
│   ├── __init__.py
│   ├── element.py
│   ├── session.py
│   ├── wait.py
│   └── states.py
│
├── download/
│   ├── __init__.py
│   └── mission.py
│
├── console/
│   ├── __init__.py
│   └── console.py
│
├── network/
│   ├── __init__.py
│   ├── listener.py
│   ├── interceptor.py
│   ├── request.py
│   ├── response.py
│   └── failure.py
│
├── keyboard/
│   ├── __init__.py
│   └── keys.py
│
└── py.typed
```

### 2.1 目录职责

| 目录 | 职责 | 主要对象 |
|---|---|---|
| `_native` | 导入 `openpage_rs` 原生扩展 | `openpage_rs` |
| `options` | 浏览器和 Session 配置 | `ChromiumOptions`、`SessionOptions` |
| `browser` | 浏览器实例、浏览器等待、浏览器状态和设置 | `Browser`、`BrowserWait`、`BrowserStates` |
| `page` | 页面类型、页面等待、页面状态、页面设置和窗口控制 | `Page`、`ChromiumPage`、`SessionPage`、`WebPage` |
| `element` | 浏览器元素和 Session HTML 元素 | `Element`、`SessionElement` |
| `download` | 下载任务 | `DownloadMission` |
| `console` | 浏览器控制台 | `Console`、`ConsoleMessage` |
| `network` | 网络监听和请求拦截 | `Listener`、`Interceptor` |
| `keyboard` | 键盘按键常量 | `Keys` |

### 2.2 已删除的结构

以下中央实现文件已经删除：

```text
python/openpage/_compat.py
```

当前实现按用户能够理解的产品领域组织，不再按内部实现方式组织。

---

## 3. 顶层公开 API

推荐的用户导入方式：

```python
from openpage import (
    Browser,
    ChromiumOptions,
    ChromiumPage,
    Console,
    ConsoleMessage,
    DownloadMission,
    Element,
    InterceptedRequest,
    Interceptor,
    Keys,
    Listener,
    ListenerFailInfo,
    ListenerPacket,
    ListenerRequest,
    ListenerRequestExtraInfo,
    ListenerResponse,
    ListenerResponseExtraInfo,
    Page,
    SessionElement,
    SessionOptions,
    SessionPage,
    WebPage,
)
```

### 3.1 顶层门面清单

| 门面 | 顶层导入 | 领域导入 | 作用 |
|---|---|---|---|
| `Browser` | `from openpage import Browser` | `from openpage.browser import Browser` | Chromium 浏览器实例 |
| `Page` | `from openpage import Page` | `from openpage.page import Page` | 通用浏览器页面 |
| `ChromiumPage` | `from openpage import ChromiumPage` | `from openpage.page import ChromiumPage` | 浏览器驱动页面 |
| `SessionPage` | `from openpage import SessionPage` | `from openpage.page import SessionPage` | HTTP Session 页面 |
| `WebPage` | `from openpage import WebPage` | `from openpage.page import WebPage` | Driver/Session 综合页面 |
| `Element` | `from openpage import Element` | `from openpage.element import Element` | 浏览器 DOM 元素 |
| `SessionElement` | `from openpage import SessionElement` | `from openpage.element import SessionElement` | Session HTML 元素 |
| `DownloadMission` | `from openpage import DownloadMission` | `from openpage.download import DownloadMission` | 下载任务 |
| `Console` | `from openpage import Console` | `from openpage.console import Console` | 控制台监听 |
| `ConsoleMessage` | `from openpage import ConsoleMessage` | `from openpage.console import ConsoleMessage` | 控制台消息 |
| `Listener` | `from openpage import Listener` | `from openpage.network import Listener` | 网络请求监听 |
| `Interceptor` | `from openpage import Interceptor` | `from openpage.network import Interceptor` | 网络请求拦截 |
| `ChromiumOptions` | `from openpage import ChromiumOptions` | `from openpage.options import ChromiumOptions` | Chromium 配置 |
| `SessionOptions` | `from openpage import SessionOptions` | `from openpage.options import SessionOptions` | Session 配置 |
| `Keys` | `from openpage import Keys` | `from openpage.keyboard import Keys` | 键盘按键常量 |

---

## 4. Browser 门面

导入：

```python
from openpage import Browser
```

### 4.1 Browser 核心 API

| 类型 | API | 说明 |
|---|---|---|
| 类方法 | `Browser.launch()` | 启动浏览器 |
| 方法 | `browser.new_page()` | 创建新页面 |
| 方法 | `browser.get_page(target_id)` | 获取已有页面 |
| 方法 | `browser.close()` | 关闭浏览器 |
| 属性 | `browser.tabs_count` | 当前页面数量 |
| 属性 | `browser.tab_ids` | 当前页面 ID 列表 |
| 属性 | `browser.version` | 浏览器版本信息 |
| 属性 | `browser.timeout` | 默认超时时间 |

### 4.2 Browser 下载 API

| API | 说明 |
|---|---|
| `browser.download_path` | 获取当前下载目录 |
| `browser.set_download_path(path)` | 设置下载目录 |
| `browser.download_file_exists_mode` | 获取文件冲突处理模式 |
| `browser.set_download_file_exists_mode(mode)` | 设置文件冲突处理模式 |
| `browser.wait_for_download()` | 等待下载完成 |
| `browser.download_missions()` | 获取下载任务列表 |
| `browser.last_download()` | 获取最近一次下载任务 |

### 4.3 Browser 等待门面

入口：

```python
browser.wait
```

| API | 说明 |
|---|---|
| `browser.wait.new_tab()` | 等待新标签页 |
| `browser.wait.download_begin()` | 等待下载开始 |
| `browser.wait.downloads_done()` | 等待下载任务完成 |

### 4.4 Browser 状态门面

入口：

```python
browser.states
```

| 属性 | 说明 |
|---|---|
| `browser.states.is_alive` | 浏览器是否仍然存活 |
| `browser.states.is_headless` | 是否为无头模式 |
| `browser.states.is_existed` | 浏览器进程是否存在 |
| `browser.states.is_incognito` | 是否为隐身模式 |

### 4.5 Browser 设置门面

入口：

```python
browser.set
```

| API | 说明 |
|---|---|
| `browser.set.load_mode.normal()` | 普通加载模式 |
| `browser.set.load_mode.eager()` | 快速加载模式 |
| `browser.set.load_mode.none()` | 不等待加载 |

---

## 5. Page 门面

当前支持四种页面类型：

| 页面类型 | 主要用途 |
|---|---|
| `Page` | 通用浏览器页面 |
| `ChromiumPage` | 自己启动并控制 Chromium 的页面 |
| `SessionPage` | 基于 HTTP Session 的页面 |
| `WebPage` | Driver 和 Session 能力合并的页面 |

---

## 6. Page 通用 API

适用于 `Page`、`ChromiumPage` 或对应页面能力的部分：

| API | 说明 |
|---|---|
| `page.goto(url)` | 导航到 URL |
| `page.get(url)` | 导航到 URL 并返回结果 |
| `page.new_tab(url)` | 创建新标签页 |
| `page.close()` | 关闭当前页面 |
| `page.quit()` | 退出页面或浏览器 |
| `page.url` | 当前 URL |
| `page.title` | 当前页面标题 |
| `page.tab_id` | 当前标签页 ID |
| `page.html` | 当前页面 HTML |
| `page.user_agent` | 当前 User-Agent |
| `page.cookies()` | 获取 Cookie |
| `page.run_js(expression)` | 执行 JavaScript |
| `page.evaluate(expression)` | 执行 JavaScript |
| `page.ele(locator)` | 获取单个元素 |
| `page.eles(locator)` | 获取多个元素 |
| `page.wait_for(locator)` | 等待元素出现 |
| `page.s_ele(locator)` | 获取 Session 元素 |
| `page.s_eles(locator)` | 获取多个 Session 元素 |
| `page.click(locator)` | 点击元素 |
| `page.input(locator, text)` | 输入文本 |
| `page.text(locator)` | 获取元素文本 |
| `page.attr(locator, name)` | 获取元素属性 |
| `page.save_screenshot(path)` | 保存截图 |
| `page.save_pdf(path)` | 保存 PDF |

---

## 7. Page 能力门面

### 7.1 等待

入口：

```python
page.wait
```

| API | 说明 |
|---|---|
| `page.wait.new_tab()` | 等待新标签页 |
| `page.wait.download_begin()` | 等待下载开始 |
| `page.wait.downloads_done()` | 等待下载完成 |
| `page.wait.upload_paths_inputted()` | 等待上传路径生效 |
| `page.wait.ele_displayed()` | 等待元素显示 |
| `page.wait.ele_hidden()` | 等待元素隐藏 |
| `page.wait.ele_deleted()` | 等待元素删除 |
| `page.wait.ele_enabled()` | 等待元素启用 |
| `page.wait.ele_clickable()` | 等待元素可点击 |
| `page.wait.url_change()` | 等待 URL 改变 |
| `page.wait.title_change()` | 等待标题改变 |
| `page.wait.alert_closed()` | 等待弹窗关闭 |

### 7.2 页面状态

入口：

```python
page.states
```

| API | 说明 |
|---|---|
| 页面存活状态 | 页面是否仍然存在 |
| 页面加载状态 | 页面是否正在加载 |
| 页面窗口状态 | 当前窗口状态 |
| 页面模式状态 | `WebPage` 当前 Driver/Session 模式 |

### 7.3 页面设置

入口：

```python
page.set
```

| API | 说明 |
|---|---|
| `page.set.blocked_urls(urls)` | 设置拦截 URL |
| `page.set.headers(headers)` | 设置请求头 |
| `page.set.user_agent(ua)` | 设置 User-Agent |
| `page.set.session_storage(item, value)` | 设置 Session Storage |
| `page.set.local_storage(item, value)` | 设置 Local Storage |
| `page.set.auto_handle_alert(...)` | 设置自动处理弹窗 |
| `page.set.download_path(path)` | 设置页面下载目录 |
| `page.set.download_file_exists(mode)` | 设置下载冲突模式 |
| `page.set.download_file_name(name)` | 设置下载文件名 |
| `page.set.upload_files(files)` | 设置上传文件 |
| `page.set.activate()` | 激活页面 |
| `page.set.load_mode.normal()` | 普通加载模式 |
| `page.set.load_mode.eager()` | 快速加载模式 |
| `page.set.load_mode.none()` | 不等待加载 |

### 7.4 页面窗口

入口：

```python
page.set.window
```

| API | 说明 |
|---|---|
| `page.set.window.max()` | 最大化窗口 |
| `page.set.window.mini()` | 最小化窗口 |
| `page.set.window.full()` | 全屏窗口 |
| `page.set.window.normal()` | 恢复普通窗口 |
| `page.set.window.hide()` | 隐藏窗口 |
| `page.set.window.show()` | 显示窗口 |
| `page.set.window.size(width, height)` | 设置窗口大小 |
| `page.set.window.location(x, y)` | 设置窗口位置 |

---

## 8. ChromiumPage 门面

导入：

```python
from openpage import ChromiumPage
```

`ChromiumPage` 额外支持浏览器级能力：

| API | 说明 |
|---|---|
| `page.browser` | 关联的 `Browser` 对象 |
| `page.tabs_count` | 页面数量 |
| `page.tab_ids` | 页面 ID 列表 |
| `page.get_tab(target_id)` | 获取标签页 |
| `page.download_path` | 下载目录 |
| `page.set_download_path(path)` | 设置下载目录 |
| `page.wait_for_download()` | 等待下载 |
| `page.download_missions()` | 下载任务列表 |
| `page.last_download()` | 最近下载任务 |
| `page.quit()` | 退出浏览器 |

---

## 9. SessionPage 门面

导入：

```python
from openpage import SessionPage
```

| API | 说明 |
|---|---|
| `page.get(url)` | HTTP GET 请求 |
| `page.post(url, payload)` | HTTP POST 请求 |
| `page.url` | 当前响应 URL |
| `page.status_code` | HTTP 状态码 |
| `page.raw_data` | 原始响应数据 |
| `page.encoding` | 响应编码 |
| `page.html` | HTML 内容 |
| `page.json` | JSON 内容 |
| `page.title` | 页面标题 |
| `page.user_agent` | User-Agent |
| `page.cookies` | Cookie |
| `page.set_user_agent(ua)` | 设置 User-Agent |
| `page.ele(locator)` | 查找 HTML 元素 |
| `page.eles(locator)` | 查找多个 HTML 元素 |
| `page.s_ele(locator)` | 查找 Session 元素 |
| `page.s_eles(locator)` | 查找多个 Session 元素 |

---

## 10. WebPage 门面

导入：

```python
from openpage import WebPage
```

`WebPage` 是当前 Python 端能力最完整的页面门面，支持 Driver 和 Session 两种工作模式。

### 10.1 WebPage 模式能力

| 能力 | 说明 |
|---|---|
| Driver 模式 | 使用浏览器驱动页面 |
| Session 模式 | 使用 HTTP Session 页面 |
| 模式切换 | 在 Driver 和 Session 能力之间切换 |
| Cookie 同步 | Browser 与 Session 之间同步 Cookie |
| JS 执行 | Driver 模式下执行 JavaScript |
| 网络监听 | Driver 模式下监听请求 |
| 下载 | Driver 模式下处理下载 |
| 窗口控制 | Driver 模式下控制窗口 |

### 10.2 WebPage API

| API | 说明 |
|---|---|
| `page.mode` | 当前模式 |
| `page.change_mode()` | 切换模式 |
| `page.is_loading` | 页面是否加载中 |
| `page.get(url)` | 获取页面 |
| `page.post(url, payload)` | POST 请求 |
| `page.url` | 当前 URL |
| `page.title` | 当前标题 |
| `page.html` | 当前 HTML |
| `page.user_agent` | 当前 User-Agent |
| `page.cookies` | 当前 Cookie |
| `page.ele(locator)` | 查找元素 |
| `page.eles(locator)` | 查找多个元素 |
| `page.s_ele(locator)` | 查找 Session 元素 |
| `page.s_eles(locator)` | 查找多个 Session 元素 |
| `page.run_js(expression)` | 执行 JavaScript |
| `page.set_user_agent(ua)` | 设置 User-Agent |
| `page.cookies_to_session()` | 浏览器 Cookie 同步到 Session |
| `page.cookies_to_browser()` | Session Cookie 同步到浏览器 |
| `page.wait_for_download()` | 等待下载 |
| `page.click_to_download()` | 点击下载 |
| `page.click_to_upload()` | 点击上传 |
| `page.click_for_new_tab()` | 点击打开新标签页 |
| `page.click_middle()` | 中键点击 |
| `page.download_missions()` | 获取下载任务 |
| `page.set_download_path(path)` | 设置下载目录 |
| `page.quit()` | 退出页面 |

---

## 11. Element 门面

### 11.1 Element

导入：

```python
from openpage import Element
```

| API | 说明 |
|---|---|
| `element.text` | 元素文本 |
| `element.html` | 元素 HTML |
| `element.attr(name)` | 获取属性 |
| `element.run_js(script)` | 在元素上执行 JavaScript |
| `element.input(value)` | 输入内容 |
| `element.clear()` | 清空内容 |
| `element.focus()` | 聚焦 |
| `element.hover()` | 悬停 |
| `element.press(key)` | 按键 |
| `element.drag()` | 拖拽 |
| `element.drag_to(target)` | 拖拽到目标 |
| `element.ele(locator)` | 查找子元素 |
| `element.eles(locator)` | 查找多个子元素 |
| `element.click_to_download()` | 点击下载 |
| `element.click_to_upload()` | 点击上传 |
| `element.click_for_new_tab()` | 点击打开新标签页 |
| `element.click_middle()` | 中键点击 |
| `element.save_screenshot(path)` | 保存元素截图 |

### 11.2 Element 点击门面

入口：

```python
element.click
```

| API | 说明 |
|---|---|
| `element.click()` | 普通点击 |
| `element.click.at(...)` | 指定坐标点击 |
| `element.click.multi(times)` | 多次点击 |
| `element.click.left()` | 左键点击 |
| `element.click.middle()` | 中键点击 |
| `element.click.right()` | 右键点击 |
| `element.click.to_download()` | 点击下载 |
| `element.click.to_upload()` | 点击上传 |
| `element.click.for_new_tab()` | 点击打开新标签页 |

### 11.3 Element 状态门面

入口：

```python
element.states
```

| API | 说明 |
|---|---|
| `is_selected` | 是否选中 |
| `is_checked` | 是否勾选 |
| `is_displayed` | 是否显示 |
| `is_enabled` | 是否启用 |
| `is_alive` | 是否仍然存在 |
| `has_rect` | 是否有布局矩形 |
| `is_in_viewport` | 是否在视口内 |
| `is_whole_in_viewport` | 是否完整位于视口内 |
| `is_covered` | 是否被覆盖 |
| `is_clickable` | 是否可点击 |

### 11.4 Element 等待门面

入口：

```python
element.wait
```

| API | 说明 |
|---|---|
| `displayed()` | 等待显示 |
| `hidden()` | 等待隐藏 |
| `enabled()` | 等待启用 |
| `disabled()` | 等待禁用 |
| `deleted()` | 等待删除 |
| `clickable()` | 等待可点击 |
| `has_rect()` | 等待存在布局矩形 |
| `covered()` | 等待被覆盖 |
| `not_covered()` | 等待不再被覆盖 |
| `disabled_or_deleted()` | 等待禁用或删除 |
| `stop_moving()` | 等待停止移动 |

---

## 12. SessionElement 门面

导入：

```python
from openpage import SessionElement
```

### 12.1 内容 API

| API | 说明 |
|---|---|
| `element.tag` | 标签名 |
| `element.text` | 文本 |
| `element.html` | HTML |
| `element.inner_html` | 内部 HTML |
| `element.raw_text` | 原始文本 |
| `element.attrs` | 属性字典 |
| `element.attr(name)` | 获取指定属性 |

### 12.2 HTML 树遍历 API

| API | 说明 |
|---|---|
| `ele(locator)` | 查找子元素 |
| `eles(locator)` | 查找多个子元素 |
| `child(locator, index)` | 获取子节点 |
| `parent()` | 获取父节点 |
| `children(locator)` | 获取子节点列表 |
| `prev(locator, index)` | 获取前一个节点 |
| `next(locator, index)` | 获取后一个节点 |
| `before(locator, index)` | 获取前置节点 |
| `after(locator, index)` | 获取后置节点 |
| `prevs(locator)` | 获取所有前置节点 |
| `nexts(locator)` | 获取所有后置节点 |
| `befores(locator)` | 获取所有前置节点 |
| `afters(locator)` | 获取所有后置节点 |

---

## 13. DownloadMission 门面

导入：

```python
from openpage import DownloadMission
```

| API | 说明 |
|---|---|
| `mission.id` | 下载任务 ID |
| `mission.guid` | 下载 GUID |
| `mission.tab_id` | 所属标签页 ID |
| `mission.url` | 下载 URL |
| `mission.folder` | 下载目录 |
| `mission.name` | 当前文件名 |
| `mission.suggested_filename` | 建议文件名 |
| `mission.tmp_path` | 临时文件路径 |
| `mission.state` | 下载状态 |
| `mission.received_bytes` | 已接收字节数 |
| `mission.total_bytes` | 总字节数 |
| `mission.rate` | 下载速率 |
| `mission.final_path` | 最终文件路径 |
| `mission.is_done` | 是否完成 |
| `mission.wait()` | 等待任务完成 |
| `mission.cancel()` | 取消任务 |

---

## 14. Console 门面

导入：

```python
from openpage import Console, ConsoleMessage
```

### 14.1 Console

| API | 说明 |
|---|---|
| `console.start()` | 开始监听控制台 |
| `console.stop()` | 停止监听 |
| `console.clear()` | 清空消息 |
| `console.wait()` | 等待一条消息 |
| `console.steps()` | 迭代控制台消息 |
| `console.listening` | 是否正在监听 |
| `console.messages` | 当前消息列表 |

### 14.2 ConsoleMessage

| API | 说明 |
|---|---|
| `message.all_info` | 完整消息信息 |
| `message.source` | 消息来源 |
| `message.level` | 消息级别 |
| `message.text` | 消息文本 |
| `message.body` | 消息主体 |
| `message.url` | 来源 URL |
| `message.line` | 行号 |
| `message.column` | 列号 |

---

## 15. Network 门面

导入：

```python
from openpage import Listener, Interceptor
```

### 15.1 Listener

| API | 说明 |
|---|---|
| `listener.start()` | 开始监听 |
| `listener.set_targets()` | 设置监听目标 |
| `listener.wait()` | 等待一条请求包 |
| `listener.steps()` | 迭代请求包 |
| `listener.wait_silent()` | 等待静默状态 |
| `listener.clear()` | 清空缓存 |
| `listener.pause()` | 暂停监听 |
| `listener.resume()` | 恢复监听 |
| `listener.stop()` | 停止监听 |
| `listener.listening` | 是否正在监听 |

### 15.2 ListenerPacket

| API | 说明 |
|---|---|
| `packet.target` | 监听目标 |
| `packet.url` | 请求 URL |
| `packet.method` | HTTP 方法 |
| `packet.resource_type` | 资源类型 |
| `packet.is_failed` | 是否失败 |
| `packet.request` | 请求信息 |
| `packet.response` | 响应信息 |
| `packet.fail_info` | 失败信息 |

### 15.3 ListenerRequest

| API | 说明 |
|---|---|
| `request.url` | 请求 URL |
| `request.method` | 请求方法 |
| `request.headers` | 请求头 |
| `request.post_data` | POST 数据 |
| `request.extra_info` | 请求额外信息 |

### 15.4 ListenerResponse

| API | 说明 |
|---|---|
| `response.url` | 响应 URL |
| `response.status` | 状态码 |
| `response.status_text` | 状态文本 |
| `response.headers` | 响应头 |
| `response.mime_type` | MIME 类型 |
| `response.body` | 响应体 |
| `response.body_base64` | 响应体是否 Base64 |
| `response.extra_info` | 响应额外信息 |

### 15.5 Interceptor

| API | 说明 |
|---|---|
| `interceptor.start()` | 开始拦截 |
| `interceptor.wait()` | 等待被拦截请求 |
| `interceptor.stop()` | 停止拦截 |
| `interceptor.listening` | 是否正在拦截 |

### 15.6 InterceptedRequest

| API | 说明 |
|---|---|
| `request.request_id` | 请求 ID |
| `request.frame_id` | Frame ID |
| `request.url` | 请求 URL |
| `request.method` | 请求方法 |
| `request.headers` | 请求头 |
| `request.resource_type` | 资源类型 |
| `request.has_post_data` | 是否包含 POST 数据 |
| `request.post_data_entries` | POST 数据条数 |
| `request.continue_request()` | 放行或修改请求 |
| `request.fail()` | 使请求失败 |
| `request.fulfill()` | 伪造响应 |

---

## 16. Options 门面

### 16.1 ChromiumOptions

导入：

```python
from openpage import ChromiumOptions
```

| API | 说明 |
|---|---|
| `set_browser_path(path)` | 设置浏览器路径 |
| `set_user_data_path(path)` | 设置用户数据目录 |
| `set_download_path(path)` | 设置下载目录 |
| `set_file_exists(mode)` | 设置下载文件冲突模式 |
| `set_load_mode(value)` | 设置加载模式 |
| `headless(on_off)` | 设置无头模式 |
| `set_window_size(width, height)` | 设置窗口大小 |
| `no_sandbox(on_off)` | 设置 No Sandbox |

### 16.2 SessionOptions

导入：

```python
from openpage import SessionOptions
```

| API | 说明 |
|---|---|
| `set_timeout(timeout_secs)` | 设置 Session 超时时间 |
| `set_user_agent(user_agent)` | 设置 User-Agent |

---

## 17. Keys 门面

导入：

```python
from openpage import Keys
```

| 常量 | 含义 |
|---|---|
| `BACKSPACE` | 退格 |
| `TAB` | Tab |
| `ENTER` | 回车 |
| `RETURN` | 回车别名 |
| `SHIFT` | Shift |
| `CONTROL` | Control |
| `CTRL` | Control 别名 |
| `ALT` | Alt |
| `ESCAPE` | Escape |
| `ESC` | Escape 别名 |
| `SPACE` | 空格 |
| `META` | Meta |
| `COMMAND` | Meta 别名 |
| `DELETE` | Delete |
| `DEL` | Delete 别名 |
| `CTRL_COMM` | 当前平台组合键 |
| `CTRL_A` | 全选组合键 |
| `CTRL_C` | 复制组合键 |
| `CTRL_X` | 剪切组合键 |
| `CTRL_V` | 粘贴组合键 |
| `CTRL_Z` | 撤销组合键 |
| `CTRL_Y` | 重做组合键 |

---

## 18. 当前代码审查结果

### 18.1 已完成项

| 项目 | 状态 |
|---|---|
| 删除本地动态库 fallback | 已完成 |
| 删除中央 `python/openpage/_compat.py` | 已完成 |
| 按公开领域拆分目录 | 已完成 |
| 重建顶层和子包导出 | 已完成 |
| 保留原有顶层主要导入方式 | 已完成 |
| 增加领域级导入方式 | 已完成 |
| 下载/等待单元测试 | 已通过 |
| 网络监听关键集成测试 | 已通过 |
| 页面下载等待关键测试 | 单独运行通过 |

### 18.2 当前问题

| 优先级 | 问题 | 影响 | 建议 |
|---|---|---|---|
| 高 | 多个类型注解引用未导入的 `Browser`、`Page`、`Element`、`Any` 等名称 | `py.typed` 包的类型检查和运行时类型解析失败 | 补充 `TYPE_CHECKING` 导入并统一注解写法 |
| 高 | 顶层 `__all__` 与实际可访问对象不一致 | `from openpage import *` 与直接导入行为不一致 | 明确顶层稳定 API，移除内部对象导出 |
| 中 | 顶层额外暴露 `_openpage_rs` | 原生实现细节泄漏到产品 API | 从顶层导出中删除 |
| 中 | `_resolve_timeout_ms` 等转换逻辑重复 | 后续行为容易漂移 | 按领域收口，避免继续增加杂物模块 |
| 中 | `Element` 中存在重复定义的 `click_middle` | 前一个定义被覆盖，源码和实际 API 不一致 | 保留一个统一签名 |
| 中 | `browser` 与 `page` 存在延迟导入关系 | 依赖关系复杂，新增 API 时容易再次循环 | 暂不扩大抽象，后续逐步降低交叉依赖 |
| 低 | 部分初始化文件格式过于紧凑 | 可读性较差 | 按项目统一格式化 |

### 18.3 类型注解问题详情

当前已经确认以下类的 `typing.get_type_hints()` 不能完整解析：

| 模块 | 类 | 未解析名称 |
|---|---|---|
| `browser.wait` | `BrowserWait` | `Browser`、`Any` |
| `browser.states` | `BrowserStates` | `Browser` |
| `browser.settings` | `BrowserSetter`、`LoadModeSetter` | `Browser`、`Any` |
| `page.wait` | `PageWait`、`WebPageWait` | `Page`、`WebPage`、`Any`、`Element` |
| `page.states` | `PageStates`、`WebPageStates` | `Page`、`WebPage` |
| `page.settings` | `PageSetter`、`ChromiumPageSetter`、`WebPageSetter` | `Page`、`ChromiumPage`、`WebPage`、`Any` |
| `page.window` | `WindowSetter` | `Page`、`WebPage` |
| `element.wait` | `ElementWait` | `Element` |
| `element.states` | `ElementStates` | `Element` |

---

## 19. 顶层 API 收口建议

正式发布时，顶层 `openpage` 建议只保留产品使用频率最高的稳定对象：

```python
from openpage import (
    Browser,
    ChromiumOptions,
    ChromiumPage,
    Console,
    ConsoleMessage,
    DownloadMission,
    Element,
    Interceptor,
    Keys,
    Listener,
    Page,
    SessionElement,
    SessionOptions,
    SessionPage,
    WebPage,
)
```

以下对象建议只从领域包导入：

```python
from openpage.browser import BrowserWait, BrowserStates, BrowserSetter
from openpage.page import PageWait, PageStates, PageSetter, WindowSetter
from openpage.element import ElementWait, ElementStates
```

以下对象不应成为顶层公开 API：

```python
openpage._openpage_rs
```

---

## 20. 测试状态

当前已执行的关键检查：

| 检查 | 状态 |
|---|---|
| `compileall` Python 语法检查 | 通过 |
| 顶层 `openpage` 导入 | 通过 |
| 各领域包导入 | 通过 |
| 下载/等待测试 | 89 个通过 |
| 网络监听关键集成测试 | 通过 |
| 页面下载等待关键测试 | 单独运行通过 |
| `git diff --check` | 通过 |

完整浏览器集成测试仍需继续关注下载事件时序问题。此前出现过下载等待偶发失败，单独运行对应测试可以通过，因此该问题更接近浏览器事件时序稳定性，而不是包拆分后的导入错误。

---

## 21. 总结

当前 Python 端已经形成以下稳定领域：

```text
Browser
Page
Element
Download
Console
Network
Options
Keyboard
```

当前架构判断：

| 方面 | 结论 |
|---|---|
| 领域划分 | 合理 |
| Rust 作为底层能力来源 | 合理 |
| Python API 组织方式 | 基本合理 |
| 中央单文件结构 | 已解决 |
| 顶层公开 API | 还需要收口 |
| 类型契约 | 需要修复 |
| 重复转换逻辑 | 需要逐步减少 |
| 当前可用性 | 基本可用，尚未完全发布级收口 |

最优先的后续顺序：

1. 修复所有类型注解解析问题；
2. 收紧顶层 `__all__` 和内部对象导出；
3. 删除重复的 `click_middle` 定义；
4. 收口重复的超时、上传路径和下载任务转换逻辑；
5. 重新执行完整 Python 测试矩阵。
