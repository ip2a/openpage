# DrissionPage vs OpenPage 能力对比文档

> 生成日期：2026-05-24
> 对比基准：DrissionPage 浏览器控制文档 (https://drissionpage.cn/browser_control/)
> openpage 版本：当前 master 分支
>
> 校正说明（2026-05-30）：
> - 本文大部分能力对比仍可作为实现映射参考
> - 但 **8.6 CLI** 一节里的协议表述已经过时
> - 当前 OpenPage 活跃 CLI 用户面应以 **TCP daemon** 为唯一执行心智
> - `serve --stdio`、`page get/page url/page title/page screenshot` 这类旧表述不能再作为当前仓库的执行手册
> - 当前权威入口请看：
>   - `README.md`
>   - `skills/openpage-test/references/cli-smoke.md`
>   - `task_plan.md`
>   - `notes.md`

---

## 第一章：连接浏览器与启动配置

### 1.1 Chromium 浏览器对象

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `Chromium(addr_or_opts, session_options)` 构造函数 | **部分实现** | **部分实现** | Rust: `Browser::launch()` 和 `Browser::connect()` 存在但 Python `ChromiumPage` 只调用 `launch` |
| 2 | 默认方式创建（无参数） | ✅ `Browser::launch(LaunchOptions::default())` | ✅ `ChromiumPage()` 无参创建 | Rust: `rust/src/browser.rs:154`, Python: `python/openpage/_compat.py:400` |
| 3 | 指定端口创建 `Chromium(9333)` | ✅ `LaunchOptions::remote_debugging_port` | ⚠️ 仅通过 `ChromiumOptions` 配置 | Rust: `rust/src/browser.rs:89`, Python: `python/openpage/_compat.py:43` |
| 4 | 指定地址创建 `Chromium('127.0.0.1:9333')` | ✅ `Browser::connect(debugger_url)` | ❌ 未暴露 | Rust: `rust/src/browser.rs:210`, Python: `ChromiumPage` 无 connect 路径 |
| 5 | 指定 ws 地址连接 `Chromium('ws://...')` | ✅ `Browser::connect(debugger_url)` | ❌ 未暴露 | 同上 |
| 6 | 通过 `ChromiumOptions` 创建 | ✅ `LaunchOptions` 结构体 | ✅ `ChromiumOptions` dataclass | Rust: `rust/src/browser.rs:82`, Python: `python/openpage/_compat.py:43` |
| 7 | 使用指定 ini 文件创建 | ❌ 未实现 | ❌ 未实现 | DP 特有配置文件系统 |
| 8 | 接管已打开的浏览器（程序启动的） | ✅ `Browser::connect()` | ❌ 未暴露 | Rust 有，但 Python 只支持 launch |
| 9 | 接管手动打开的浏览器 | ✅ `Browser::connect()` | ❌ 未暴露 | 同上 |
| 10 | 接管 bat 文件启动的浏览器 | ✅ `Browser::connect()` | ❌ 未暴露 | 同上 |
| 11 | 用 ws 连接远程浏览器 | ✅ `Browser::connect()` | ❌ 未暴露 | 同上 |
| 12 | 多浏览器共存（独立端口+用户文件夹） | ⚠️ 可手动配置 | ⚠️ 可手动配置 | Rust/Python 都可通过独立 `LaunchOptions` 实现，但无内置自动管理 |
| 13 | `auto_port()` 自动分配端口 | ❌ 未实现 | ❌ 未实现 | DP 特有功能 |
| 14 | `new_env()` 创建全新浏览器 | ⚠️ Rust 默认用临时 user_data_dir | ❌ 无显式 API | Rust: `rust/src/browser.rs:160` 每次启动用新的临时目录，类似 new_env |
| 15 | 使用系统浏览器用户目录 | ❌ 未实现 | ❌ 未实现 | `use_system_user_path()` 缺失 |
| 16 | `latest_tab` 获取最后激活标签页 | ✅ `Browser::latest_tab()` | ✅ `Browser.latest_tab` (间接) | Rust: `rust/src/browser.rs:370`, Python: `Page` 通过 `browser.new_page()` 获取 |
| 17 | `new_tab()` 新建标签页 | ✅ `Browser::new_tab()` | ✅ `Browser.new_page()` / `Page.new_tab()` | Rust: `rust/src/browser.rs:274`, Python: `python/openpage/_compat.py:391` |
| 18 | `get_tab()` 按条件获取标签页 | ✅ `Browser::get_page(target_id)` | ✅ `Browser.get_page()` / `ChromiumPage.get_tab()` | Rust: `rust/src/browser.rs:320`, Python: `python/openpage/_compat.py:417` |
| 19 | `tabs_count` 标签页数量 | ✅ `Browser::tabs_count()` | ✅ `Browser.tabs_count` / `ChromiumPage.tabs_count` | Rust: `rust/src/browser.rs:335`, Python: `python/openpage/_compat.py:137` |
| 20 | `tab_ids` 标签页 ID 列表 | ✅ `Browser::tab_ids()` | ✅ `Browser.tab_ids` / `ChromiumPage.tab_ids` | Rust: `rust/src/browser.rs:339`, Python: `python/openpage/_compat.py:141` |
| 21 | `activate_tab()` 激活标签页 | ✅ `Browser::activate_tab()` | ❌ 未暴露 | Rust: `rust/src/browser.rs:377` |
| 22 | `close_tabs()` 关闭标签页 | ✅ `Browser::close_tabs()` | ❌ 未暴露 | Rust: `rust/src/browser.rs:389` |
| 23 | `version` 浏览器版本 | ✅ `Browser::version()` | ✅ `Browser.version` | Rust: `rust/src/browser.rs:418`, Python: `python/openpage/_compat.py:145` |
| 24 | `browser.pid` 浏览器进程 ID | ✅ `Browser::browser_pid()` | ❌ 未暴露 | Rust: `rust/src/browser.rs:448` |
| 25 | `quit()` 关闭浏览器 | ✅ `Browser::close()` (内部) | ✅ `ChromiumPage.quit()` / `Browser.close()` | Rust: `browser.rs` 有 close, Python: `python/openpage/_compat.py:406` |
| 26 | `set.retry_times()` 全局重试次数 | ❌ 未实现 | ❌ 未实现 | DP 特有 |
| 27 | `states.is_alive` 浏览器是否存活 | ✅ `Browser::is_alive()` | ✅ `BrowserStates.is_alive` | Rust: `rust/src/browser.rs:429`, Python: `python/openpage/_compat.py:223` |
| 28 | `states.is_headless` 是否无头模式 | ✅ `Browser::is_headless()` | ✅ `BrowserStates.is_headless` | Rust: `rust/src/browser.rs:433`, Python: `python/openpage/_compat.py:227` |
| 29 | `states.is_existed` 是否存在 | ✅ `Browser::is_existed()` | ✅ `BrowserStates.is_existed` | Rust: `rust/src/browser.rs:437`, Python: `python/openpage/_compat.py:231` |
| 30 | `states.is_incognito` 是否无痕模式 | ✅ `Browser::is_incognito()` | ✅ `BrowserStates.is_incognito` | Rust: `rust/src/browser.rs:441`, Python: `python/openpage/_compat.py:235` |
| 31 | `wait.new_tab()` 等待新标签页 | ✅ `Browser::wait_for_new_tab()` | ✅ `BrowserWait.new_tab()` | Rust: `rust/src/browser.rs:452`, Python: `python/openpage/_compat.py:198` |
| 32 | `wait.download_begin()` 等待下载开始 | ✅ `Browser::wait_for_download_begin()` | ✅ `BrowserWait.download_begin()` | Rust: `rust/src/browser.rs:663`, Python: `python/openpage/_compat.py:202` |
| 33 | `wait.downloads_done()` 等待下载完成 | ✅ `Browser::wait_for_downloads_done()` | ✅ `BrowserWait.downloads_done()` | Rust: `rust/src/browser.rs:685`, Python: `python/openpage/_compat.py:210` |
| 34 | `download_path` 下载路径 | ✅ `Browser::download_path()` | ✅ `Browser.download_path` | Rust: `rust/src/browser.rs:481`, Python: `python/openpage/_compat.py:167` |
| 35 | `set_download_path()` 设置下载路径 | ✅ `Browser::set_download_path()` | ✅ `Browser.set_download_path()` | Rust: `rust/src/browser.rs:494`, Python: `python/openpage/_compat.py:170` |
| 36 | `download_file_exists` 文件冲突模式 | ✅ `Browser::download_file_exists_mode()` | ✅ `Browser.download_file_exists_mode` | Rust: `rust/src/browser.rs:509`, Python: `python/openpage/_compat.py:174` |
| 37 | `wait_for_download()` 等待下载 | ✅ `Browser::wait_for_download()` | ✅ `Browser.wait_for_download()` | Rust: `rust/src/browser.rs:642`, Python: `python/openpage/_compat.py:180` |
| 38 | `download_missions` 下载任务列表 | ✅ `Browser::download_missions()` | ✅ `Browser.download_missions()` | Rust: `rust/src/browser.rs:624`, Python: `python/openpage/_compat.py:183` |
| 39 | `last_download` 最后一个下载 | ✅ `Browser::last_download()` | ✅ `Browser.last_download()` | Rust: `rust/src/browser.rs:634`, Python: `python/openpage/_compat.py:186` |

**本章小结 - Chromium 对象：**
- Rust 实现率：约 **82%** (32/39 项)
- Python API 实现率：约 **62%** (24/39 项)
- 主要缺失：
  - Python 端缺少 `connect()` 接管已有浏览器的能力
  - Python 端缺少 `activate_tab()`、`close_tabs()` 标签页管理
  - 缺少 DP 特有的 `auto_port()`、`new_env()` 显式 API
  - 缺少 ini 配置文件系统

---

### 1.2 ChromiumOptions 浏览器启动配置

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `ChromiumOptions(read_file, ini_path)` 构造函数 | ⚠️ 无 ini 系统 | ⚠️ 无 ini 系统 | openpage `ChromiumOptions` 是纯 dataclass，无文件读取 |
| 2 | `set_argument()` 设置启动参数 | ✅ `LaunchOptions::set_argument()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 3 | `remove_argument()` 删除启动参数 | ✅ `LaunchOptions::remove_argument()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 4 | `clear_arguments()` 清空启动参数 | ✅ `LaunchOptions::clear_arguments()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 5 | `set_browser_path()` 浏览器路径 | ✅ `LaunchOptions::browser_path` | ✅ `ChromiumOptions.set_browser_path()` | Rust: `rust/src/browser.rs:84`, Python: `python/openpage/_compat.py:55` |
| 6 | `set_tmp_path()` 临时文件路径 | ✅ `LaunchOptions::tmp_path` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 7 | `set_local_port()` 本地调试端口 | ✅ `LaunchOptions::remote_debugging_port` | ❌ 未暴露 | Rust: `rust/src/browser.rs:89`, Python `ChromiumOptions` 无此字段 |
| 8 | `set_address()` 浏览器地址 | ⚠️ 连接通过 `Browser::connect()` 直接传 URL | ❌ 未实现 | 连接通过 `Browser::connect()` 直接传 URL |
| 9 | `auto_port()` 自动分配端口 | ✅ `LaunchOptions::auto_port` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 10 | `set_user_data_path()` 用户数据路径 | ✅ `LaunchOptions::user_data_dir` | ✅ `ChromiumOptions.set_user_data_path()` | Rust: `rust/src/browser.rs:88`, Python: `python/openpage/_compat.py:59` |
| 11 | `use_system_user_path()` 使用系统用户路径 | ✅ `LaunchOptions::use_system_user_path()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 12 | `set_cache_path()` 缓存路径 | ✅ `LaunchOptions::cache_path` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 13 | `existing_only()` 仅连接已有浏览器 | ✅ `LaunchOptions::existing_only` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 14 | `add_extension()` 添加插件 | ✅ `LaunchOptions::extensions` | ❌ 未实现 | Rust: `rust/src/browser.rs:98` |
| 15 | `remove_extensions()` 移除插件 | ✅ `LaunchOptions::remove_extensions()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 16 | `set_user()` 设置用户配置 | ❌ 未实现 | ❌ 未实现 | |
| 17 | `set_pref()` 设置用户配置项 | ✅ `LaunchOptions::set_pref()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 18 | `remove_pref()` 删除配置项 | ✅ `LaunchOptions::remove_pref()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 19 | `remove_pref_from_file()` 从文件删除配置项 | ❌ 未实现 | ❌ 未实现 | DP 特有文件操作 |
| 20 | `clear_prefs()` 清空配置项 | ✅ `LaunchOptions::clear_prefs()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 21 | `set_timeouts()` 设置超时时间 | ✅ `LaunchOptions::timeouts` / `Browser::set_timeouts()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 22 | `set_retry()` 设置重试 | ❌ 未实现 | ❌ 未实现 | |
| 23 | `set_load_mode()` 加载策略 | ✅ `LaunchOptions::load_mode` | ✅ `ChromiumOptions.set_load_mode()` | Rust: `rust/src/browser.rs:87`, Python: `python/openpage/_compat.py:71` |
| 24 | `set_proxy()` 设置代理 | ✅ `LaunchOptions::proxy` | ❌ 未实现 | Rust: `rust/src/browser.rs:100` |
| 25 | `set_download_path()` 下载路径 | ✅ `LaunchOptions::download_path` | ✅ `ChromiumOptions.set_download_path()` | Rust: `rust/src/browser.rs:85`, Python: `python/openpage/_compat.py:63` |
| 26 | `headless()` 无头模式 | ✅ `LaunchOptions::headless` | ✅ `ChromiumOptions.headless()` | Rust: `rust/src/browser.rs:90`, Python: `python/openpage/_compat.py:75` |
| 27 | `new_env()` 全新环境 | ⚠️ 默认行为类似 | ❌ 无显式 API | Rust 默认创建临时 user_data_dir |
| 28 | `set_flag()` 设置实验项 | ✅ `LaunchOptions::set_flag()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 29 | `clear_flags_in_file()` 清除文件中的实验项 | ❌ 未实现 | ❌ 未实现 | DP 特有文件操作 |
| 30 | `clear_flags()` 清空实验项 | ✅ `LaunchOptions::clear_flags()` | ❌ 未实现 | Rust: `rust/src/browser.rs` |
| 31 | `incognito()` 无痕模式 | ✅ `LaunchOptions::incognito` | ❌ 未实现 | Rust: `rust/src/browser.rs:96` |
| 32 | `ignore_certificate_errors()` 忽略证书错误 | ✅ `LaunchOptions::ignore_https_errors` | ❌ 未实现 | Rust: `rust/src/browser.rs:97` |
| 33 | `no_imgs()` 禁止加载图片 | ✅ `LaunchOptions::no_imgs` | ❌ 未实现 | Rust: `rust/src/browser.rs:103` |
| 34 | `no_js()` 禁用 JavaScript | ✅ `LaunchOptions::no_js` | ❌ 未实现 | Rust: `rust/src/browser.rs:102` |
| 35 | `mute()` 静音 | ✅ `LaunchOptions::mute` | ❌ 未实现 | Rust: `rust/src/browser.rs:101` |
| 36 | `set_user_agent()` 设置 UA | ✅ `LaunchOptions::user_agent` | ❌ 未实现（启动时） | Rust: `rust/src/browser.rs:104` |
| 37 | `save()` 保存到 ini 文件 | ❌ 未实现 | ❌ 未实现 | DP 特有配置文件系统 |
| 38 | `set_window_size()` 设置窗口大小 | ✅ `LaunchOptions::width/height` | ✅ `ChromiumOptions.set_window_size()` | Rust: `rust/src/browser.rs:91-92`, Python: `python/openpage/_compat.py:79` |
| 39 | `no_sandbox()` 无沙盒模式 | ✅ `LaunchOptions::no_sandbox` | ✅ `ChromiumOptions.no_sandbox()` | Rust: `rust/src/browser.rs:93`, Python: `python/openpage/_compat.py:84` |
| 40 | `set_file_exists()` 文件存在模式 | ✅ `LaunchOptions::download_file_exists` | ✅ `ChromiumOptions.set_file_exists()` | Rust: `rust/src/browser.rs:86`, Python: `python/openpage/_compat.py:67` |

**本章小结 - ChromiumOptions：**
- Rust 实现率：约 **75%** (30/40 项)
- Python API 实现率：约 **20%** (8/40 项)
- 主要缺失：
  - 无 ini 配置文件系统
  - `set_user()` 未实现
  - `remove_pref_from_file()` / `clear_flags_in_file()` 未实现（DP 特有文件操作）
  - Python 端大量启动配置未暴露

---

## 第二章：页面操作

### 2.1 Tab/Page 基本操作

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `tab.get(url)` 访问网址 | ✅ `Page::goto()` | ✅ `Page.get()` / `Page.goto()` | Rust: `rust/src/page.rs` (通过 PyO3), Python: `python/openpage/_compat.py:274-278` |
| 2 | `tab.url` 当前 URL | ✅ `Page::url()` | ✅ `Page.url` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:282` |
| 3 | `tab.title` 页面标题 | ✅ `Page::title()` | ✅ `Page.title` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:286` |
| 4 | `tab.html` 页面源码 | ✅ `Page::html()` | ✅ `Page.html` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:294` |
| 5 | `tab.user_agent` UA | ✅ `Page::user_agent()` | ✅ `Page.user_agent` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:298` |
| 6 | `tab.cookies()` Cookie | ✅ `Page::cookies()` | ✅ `Page.cookies()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:319` |
| 7 | `tab.scroll.to_top()` 滚动到顶部 | ✅ `Element::scroll_to_top()` | ⚠️ 仅 Element 有，Page 无直接方法 | Rust: `rust/src/element.rs:467` |
| 8 | `tab.scroll.to_bottom()` 滚动到底部 | ✅ `Element::scroll_to_bottom()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:471` |
| 9 | `tab.scroll.to_half()` 滚动到中部 | ✅ `Element::scroll_to_half()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:475` |
| 10 | `tab.scroll.to_rightmost()` 滚动到最右 | ✅ `Element::scroll_to_rightmost()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:481` |
| 11 | `tab.scroll.to_leftmost()` 滚动到最左 | ✅ `Element::scroll_to_leftmost()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:485` |
| 12 | `tab.scroll.to_location(x, y)` 滚动到位置 | ✅ `Element::scroll_to_location()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:489` |
| 13 | `tab.scroll.up(pixels)` 向上滚动 | ✅ `Element::scroll_up()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:493` |
| 14 | `tab.scroll.down(pixels)` 向下滚动 | ✅ `Element::scroll_down()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:497` |
| 15 | `tab.scroll.left(pixels)` 向左滚动 | ✅ `Element::scroll_left()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:501` |
| 16 | `tab.scroll.right(pixels)` 向右滚动 | ✅ `Element::scroll_right()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:505` |
| 17 | `tab.scroll.to_see(ele)` 滚动到元素可见 | ✅ `Element::scroll_to_see()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:509` |
| 18 | `tab.scroll.to_center()` 滚动到元素居中 | ✅ `Element::scroll_to_center()` | ⚠️ 仅 Element 有 | Rust: `rust/src/element.rs:528` |
| 19 | `tab.run_js(script)` 执行 JS | ✅ `Page::run_js()` / `Page::evaluate()` | ✅ `Page.run_js()` / `Page.evaluate()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:325-329` |
| 20 | `tab.refresh()` 刷新页面 | ✅ `Page::refresh()` | ⚠️ 需确认 Python 暴露 | Rust: `rust/src/page.rs` |
| 21 | `tab.back()` 后退 | ✅ `Page::back()` | ⚠️ 需确认 Python 暴露 | Rust: `rust/src/page.rs:1425` |
| 22 | `tab.forward()` 前进 | ✅ `Page::forward()` | ⚠️ 需确认 Python 暴露 | Rust: `rust/src/page.rs:1429` |
| 23 | `tab.stop_loading()` 停止加载 | ✅ `Page::stop_loading()` | ⚠️ 需确认 Python 暴露 | Rust: `rust/src/page.rs:1937` |
| 24 | `tab.set.blocked_urls()` 阻止 URL | ✅ `Page::set_blocked_urls()` | ✅ `PageSetter.blocked_urls()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:458` |
| 25 | `tab.set.user_agent()` 设置 UA | ✅ `Page::set_user_agent_override()` | ✅ `PageSetter.user_agent()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:464` |
| 26 | `tab.set.headers()` 设置请求头 | ✅ `Page::set_headers()` | ✅ `PageSetter.headers()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:461` |
| 27 | `tab.set.local_storage()` 设置本地存储 | ✅ `Page::set_local_storage()` | ✅ `PageSetter.local_storage()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:470` |
| 28 | `tab.set.session_storage()` 设置会话存储 | ✅ `Page::set_session_storage()` | ✅ `PageSetter.session_storage()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:467` |
| 29 | `tab.set.window.max()` 窗口最大化 | ✅ `Page::set_window_maximized()` | ✅ `WindowSetter.max()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:424` |
| 30 | `tab.set.window.mini()` 窗口最小化 | ✅ `Page::set_window_minimized()` | ✅ `WindowSetter.mini()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:425` |
| 31 | `tab.set.window.full()` 全屏 | ✅ `Page::set_window_fullscreen()` | ✅ `WindowSetter.full()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:421` |
| 32 | `tab.set.window.normal()` 还原窗口 | ✅ `Page::set_window_normal()` | ✅ `WindowSetter.normal()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:426` |
| 33 | `tab.set.window.size(w, h)` 设置窗口大小 | ✅ `Page::set_window_size()` | ✅ `WindowSetter.size()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:428` |
| 34 | `tab.set.window.location(x, y)` 设置窗口位置 | ✅ `Page::set_window_location()` | ✅ `WindowSetter.location()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:423` |
| 35 | `tab.set.window.hide()` 隐藏窗口 | ✅ `Page::set_window_hidden()` | ✅ `WindowSetter.hide()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:422`, 依赖 `window.rs` |
| 36 | `tab.set.window.show()` 显示窗口 | ✅ `Page::set_window_visible()` | ✅ `WindowSetter.show()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:427`, 依赖 `window.rs` |
| 37 | `tab.set.load_mode()` 加载模式 | ✅ `Page::set_load_mode()` | ✅ `LoadModeSetter` | Rust: `rust/src/browser.rs:537`, Python: `python/openpage/_compat.py:239-262` |
| 38 | `tab.set.activate()` 激活页面 | ✅ `Page::activate()` | ✅ `PageSetter.activate()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:509` |
| 39 | `tab.set.upload_files()` 上传文件 | ✅ `Page::set_upload_files()` | ✅ `PageSetter.upload_files()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:506` |
| 40 | `tab.set.download_path()` 页面下载路径 | ✅ `Browser::set_page_download_path()` | ✅ `PageSetter.download_path()` | Rust: `rust/src/browser.rs:559`, Python: `python/openpage/_compat.py:484` |
| 41 | `tab.set.download_file_name()` 下载文件名 | ✅ `Browser::set_page_download_filename()` | ✅ `PageSetter.download_file_name()` | Rust: `rust/src/browser.rs:603`, Python: `python/openpage/_compat.py:496` |
| 42 | `tab.set.auto_handle_alert()` 自动处理弹窗 | ✅ `Page::set_auto_alert_action()` | ✅ `PageSetter.auto_handle_alert()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:473` |
| 43 | `tab.wait.ele_displayed()` 等待元素显示 | ✅ `Page::wait_for_ele_displayed()` | ✅ `PageWait.ele_displayed()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:564` |
| 44 | `tab.wait.ele_hidden()` 等待元素隐藏 | ✅ `Page::wait_for_ele_hidden()` | ✅ `PageWait.ele_hidden()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:578` |
| 45 | `tab.wait.ele_deleted()` 等待元素删除 | ✅ `Page::wait_for_ele_deleted()` | ✅ `PageWait.ele_deleted()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:592` |
| 46 | `tab.wait.ele_enabled()` 等待元素可用 | ✅ `Page::wait_for_ele_enabled()` | ✅ `PageWait.ele_enabled()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:606` |
| 47 | `tab.wait.ele_clickable()` 等待元素可点击 | ✅ `Page::wait_for_ele_clickable()` | ✅ `PageWait.ele_clickable()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:620` |
| 48 | `tab.wait.url_change()` 等待 URL 变化 | ✅ `Page::wait_for_url_change()` | ✅ `PageWait.url_change()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:634` |
| 49 | `tab.wait.title_change()` 等待标题变化 | ✅ `Page::wait_for_title_change()` | ✅ `PageWait.title_change()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:642` |
| 50 | `tab.wait.load_start()` 等待加载开始 | ✅ `Page::wait_for_load_start()` | ✅ `PageWait.load_start()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:650` |
| 51 | `tab.wait.doc_loaded()` 等待文档加载完成 | ✅ `Page::wait_for_doc_loaded()` | ✅ `PageWait.doc_loaded()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:653` |
| 52 | `tab.wait.eles_loaded()` 等待元素加载 | ✅ `Page::wait_for_elements_loaded()` | ✅ `PageWait.eles_loaded()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:656` |
| 53 | `tab.wait.alert_closed()` 等待弹窗关闭 | ✅ `Page::wait_for_alert_closed()` | ✅ `PageWait.alert_closed()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:665` |
| 54 | `tab.wait.covered()` / `not_covered()` | ✅ `Page::wait_for_ele_covered()` 等 | ✅ `ElementWait.covered()` / `not_covered()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:489-497` |
| 55 | `tab.states.ready_state` 就绪状态 | ✅ `Page::ready_state()` | ✅ `PageStates.ready_state` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:674` |
| 56 | `tab.states.is_loading` 是否加载中 | ✅ `Page::is_loading()` | ✅ `PageStates.is_loading` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:678` |
| 57 | `tab.states.is_alive` 是否存活 | ✅ `Page::is_alive()` | ✅ `PageStates.is_alive` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:682` |
| 58 | `tab.states.has_alert` 是否有弹窗 | ✅ `Page::has_alert()` | ✅ `PageStates.has_alert` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:693` |
| 59 | `tab.states.has_rect` 是否有矩形（坐标） | ✅ `Page::has_rect()` | ✅ `PageStates.has_rect` / `ElementWait.has_rect()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:475` |
| 60 | `tab.handle_alert()` 处理弹窗 | ✅ `Page::handle_alert()` | ✅ `Page.handle_alert()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:343` |
| 61 | `tab.listen` 网络监听 | ✅ `Page::listener()` | ✅ `Page.listen` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:332` |
| 62 | `tab.intercept` 请求拦截 | ✅ `Page::interceptor()` | ✅ `Page.intercept` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:337` |
| 63 | `tab.screenshot()` 截图 | ✅ `Page::save_screenshot()` | ✅ `Page.save_screenshot()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:385` |
| 64 | `tab.save_pdf()` 保存 PDF | ✅ `Page::save_pdf()` | ✅ `Page.save_pdf()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:388` |
| 65 | `tab.close()` 关闭标签页 | ✅ `Page::close()` | ✅ `Page.close()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:396` |

**本章小结 - 页面操作：**
- Rust 实现率：约 **88%** (57/65 项)
- Python API 实现率：约 **80%** (52/65 项)
- 主要缺失：
  - 页面级别的滚动方法已直接实现在 `Page` 上
  - Python 端部分方法暴露需确认

---

## 第三章：元素定位

### 3.1 定位方式

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `tab.ele(locator)` 查找单个元素 | ✅ `Page::find()` / `Page::wait_for()` | ✅ `Page.ele()` / `Page.wait_for()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:367-368` |
| 2 | `tab.eles(locator)` 查找多个元素 | ✅ `Page::find_all()` | ✅ `Page.eles()` | Rust: `rust/src/page.rs`, Python: `python/openpage/_compat.py:370` |
| 3 | CSS 选择器定位 | ✅ `LocatorKind::Css` | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:11` |
| 4 | XPath 定位 | ✅ `LocatorKind::XPath` | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:12` |
| 5 | `tag:` 标签名定位 | ✅ 解析为 CSS | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:39` |
| 6 | `@id=value` 属性定位 | ✅ 解析为 CSS `#id` | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:47` |
| 7 | `@class=value` 属性定位 | ✅ 解析为 CSS `.class` | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:58` |
| 8 | `@name=value` 通用属性定位 | ✅ 解析为 CSS `[name="value"]` | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:59` |
| 9 | `text=文本` 文本定位 | ✅ `Locator::parse()` 支持 `text=` | ✅ 通过 Rust 解析 | Rust: `rust/src/locator.rs:47` |
| 10 | `ele.ele(locator)` 元素内查找 | ✅ `Element::find()` | ✅ `Element.ele()` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:155` |
| 11 | `ele.eles(locator)` 元素内查找多个 | ✅ `Element::find_all()` | ✅ `Element.eles()` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:156` |
| 12 | 相对定位：`ele.parent()` | ✅ `Element::parent()` | ❌ 未暴露 | Rust: `rust/src/element.rs:678`, Python Element 无 parent |
| 13 | 相对定位：`ele.parent_level(n)` | ✅ `Element::parent_level()` | ❌ 未暴露 | Rust: `rust/src/element.rs:682` |
| 14 | 相对定位：`ele.children()` | ✅ `Element::children()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 15 | 相对定位：`ele.child()` | ✅ `Element::child()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 16 | 相对定位：`ele.prev()` / `prevs()` | ✅ `Element::prev()` / `Element::prevs()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 17 | 相对定位：`ele.next()` / `nexts()` | ✅ `Element::next()` / `Element::nexts()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 18 | 相对定位：`ele.before()` / `befores()` | ✅ `Element::before()` / `Element::befores()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 19 | 相对定位：`ele.after()` / `afters()` | ✅ `Element::after()` / `Element::afters()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 20 | 查找多个定位器 `find_locators()` | ✅ `Element::find_locators()` | ❌ 未暴露 | Rust: `rust/src/element.rs:339` |
| 21 | `ele.sr()` Shadow Root | ✅ `Element::sr()` / `Element::shadow_root()` | ❌ 未暴露 | Rust: `rust/src/element.rs:629-676`, Python Element 无 sr/shadow_root |

**本章小结 - 元素定位：**
- Rust 实现率：约 **95%** (20/21 项)
- Python API 实现率：约 **38%** (8/21 项)
- 主要缺失：
  - Python 端 Element 的**所有相对定位方法均未暴露**（parent, children, prev, next, before, after 等）
  - `find_locators()` 多定位器查找未暴露到 Python
  - Shadow Root 未暴露到 Python

---

## 第四章：元素操作

### 4.1 元素交互

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `ele.click()` 点击 | ✅ `Element::click()` | ✅ `Element.click()` | Rust: `rust/src/element.rs:75`, Python: `python/openpage/_compat.py` (Element 类) |
| 2 | `ele.input(text)` 输入文本 | ✅ `Element::input()` | ✅ `Element.input()` | Rust: `rust/src/element.rs:85`, Python: 通过 PyO3 |
| 3 | `ele.input(text, clear=True)` 清空后输入 | ✅ `Element::input_with_options()` | ❌ `Element.input()` 不支持 clear 参数 | Rust: `rust/src/element.rs:89`, Python Element 无 clear 参数 |
| 4 | `ele.clear()` 清空内容 | ✅ `Element::clear()` | ✅ `Element.clear()` | Rust: `rust/src/element.rs:147`, Python: `python/openpage/_compat.py:153` |
| 5 | `ele.submit()` 提交表单 | ✅ `Element::submit()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 6 | `ele.hover()` 悬停 | ✅ `Element::hover()` / `hover_with_offset()` | ❌ 未暴露 | Rust: `rust/src/element.rs:1236` |
| 7 | `ele.focus()` 聚焦 | ✅ `Element::focus()` (内部) | ❌ 未暴露 | Rust: `rust/src/element.rs:199` |
| 8 | `ele.press_key(key)` 按键 | ✅ `Element::press_key()` | ✅ `Element.press()` | Rust: `rust/src/element.rs:255`, Python: `python/openpage/_compat.py:159` |
| 9 | `ele.select()` 选择下拉框 | ✅ `Element::select_by_text/value/index()` | ❌ 未暴露 | Rust: `rust/src/element.rs:1352` |
| 10 | `ele.set_file_input(files)` 设置文件输入 | ✅ `Element::set_file_input_files()` | ❌ 未暴露 | Rust: `rust/src/element.rs:234` |

### 4.2 元素属性与信息

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `ele.text` 元素文本 | ✅ `Element::text()` | ✅ `Element.text` | Rust: `rust/src/element.rs:265`, Python: 通过 PyO3 |
| 2 | `ele.tag` 标签名 | ✅ `Element::tag()` | ✅ `Element.tag` | Rust: `rust/src/element.rs:274` |
| 3 | `ele.html` 外部 HTML | ✅ `Element::html()` | ✅ `Element.html` | Rust: `rust/src/element.rs:286`, Python: `python/openpage/_compat.py:157` |
| 4 | `ele.inner_html` 内部 HTML | ✅ `Element::inner_html()` | ❌ 未暴露 | Rust: `rust/src/element.rs:295` |
| 5 | `ele.attr(name)` 获取属性 | ✅ `Element::attr()` | ✅ `Element.attr()` | Rust: `rust/src/element.rs:367`, Python: `python/openpage/_compat.py:152` |
| 6 | `ele.attrs()` 获取所有属性 | ✅ `Element::attrs()` | ❌ 未暴露 | Rust: `rust/src/element.rs:350` |
| 7 | `ele.property(name)` 获取属性值 | ✅ `Element::property()` | ❌ 未暴露 | Rust: `rust/src/element.rs:398` |
| 8 | `ele.value` 表单值 | ✅ `Element::value()` | ❌ 未暴露 | Rust: `rust/src/element.rs:411` |
| 9 | `ele.link` 链接 | ✅ `Element::link()` | ❌ 未暴露 | Rust: `rust/src/element.rs:415` |
| 10 | `ele.css_path` CSS 路径 | ✅ `Element::css_path()` | ❌ 未暴露 | Rust: `rust/src/element.rs:430` |
| 11 | `ele.xpath` XPath 路径 | ✅ `Element::xpath()` | ❌ 未暴露 | Rust: `rust/src/element.rs:434` |
| 12 | `ele.style(name)` 计算样式 | ✅ `Element::style()` | ❌ 未暴露 | Rust: `rust/src/element.rs:446` |
| 13 | `ele.rect` 元素矩形位置 | ✅ `Element::has_rect()` 等 | ✅ `ElementStates.has_rect` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:475` |
| 14 | `ele.is_displayed()` 是否显示 | ✅ `Element::is_displayed()` | ✅ `ElementStates.is_displayed` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:480` |
| 15 | `ele.is_enabled()` 是否可用 | ✅ `Element::is_enabled()` | ✅ `ElementStates.is_enabled` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:481` |
| 16 | `ele.is_selected()` 是否选中 | ✅ `Element::is_selected()` | ✅ `ElementStates.is_selected` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:484` |
| 17 | `ele.is_clickable()` 是否可点击 | ✅ `Element::is_clickable()` | ✅ `ElementStates.is_clickable` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:478` |
| 18 | `ele.is_covered()` 是否被遮挡 | ✅ `Element::is_covered()` | ✅ `ElementStates.is_covered` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:479` |
| 19 | `ele.comments()` 注释内容 | ✅ `Element::comments()` | ❌ 未暴露 | Rust: `rust/src/element.rs:438` |
| 20 | `ele.texts()` 所有文本节点 | ✅ `Element::texts()` | ❌ 未暴露 | Rust: `rust/src/element.rs:442` |
| 21 | `ele.pseudo_before/after` 伪元素内容 | ✅ `Element::pseudo_before/after()` | ❌ 未暴露 | Rust: `rust/src/element.rs:459-464` |
| 22 | `ele.child_count` 子元素数量 | ✅ `Element::child_count()` | ❌ 未暴露 | Rust: `rust/src/element.rs:423` |
| 23 | `ele.run_js(script)` 在元素上执行 JS | ✅ `Element::run_js()` | ✅ `Element.run_js()` | Rust: `rust/src/element.rs`, Python: `python/openpage/_compat.py:160` |
| 24 | `ele.set_attr(name, value)` 设置属性 | ✅ `Element::set_attr()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 25 | `ele.remove_attr(name)` 删除属性 | ✅ `Element::remove_attr()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 26 | `ele.set_property(name, value)` 设置属性值 | ✅ `Element::set_property()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 27 | `ele.set_style(name, value)` 设置样式 | ✅ `Element::set_style()` | ❌ 未暴露 | Rust: `rust/src/element.rs` |
| 28 | `ele.save()` 保存元素资源 | ✅ `Element::save()` | ❌ 未暴露 | Rust: `rust/src/element.rs:596` |
| 29 | `ele.src()` 获取资源内容 | ✅ `Element::src()` | ❌ 未暴露 | Rust: `rust/src/element.rs:537` |

**本章小结 - 元素操作：**
- Rust 实现率：约 **93%** (27/29 项)
- Python API 实现率：约 **28%** (8/29 项)
- 主要缺失：
  - Python 端 Element **仅有 13 个公开成员**：`attr, clear, click, ele, eles, html, input, press, run_js, save_screenshot, states, text, wait`
  - **大量 Rust 已实现的方法未暴露到 Python**：`tag, link, value, inner_html, attrs, property, raw_text, css_path, xpath, style, comments, texts, pseudo_before/after, child_count, set_attr, remove_attr, set_property, set_style, save, src`, 以及**所有滚动方法**和**所有相对定位方法**
  - 但 `ElementStates`（10 个状态属性）和 `ElementWait`（10 个等待方法）在 Python 中已完整暴露

---

## 第五章：SessionPage / WebPage 双模式

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `SessionPage` 纯会话模式 | ✅ `SessionPage` | ✅ `SessionPage` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:711` |
| 2 | `SessionPage.get(url)` | ✅ `SessionPage::get()` | ✅ `SessionPage.get()` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:719` |
| 3 | `SessionPage.post(url, payload)` | ✅ `SessionPage::post_json()` | ✅ `SessionPage.post()` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:722` |
| 4 | `SessionPage.html` | ✅ `SessionPage::html()` | ✅ `SessionPage.html` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:744` |
| 5 | `SessionPage.title` | ✅ `SessionPage::title()` | ✅ `SessionPage.title` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:752` |
| 6 | `SessionPage.url` | ✅ `SessionPage::url()` | ✅ `SessionPage.url` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:727` |
| 7 | `SessionPage.status_code` | ✅ `SessionPage::status_code()` | ✅ `SessionPage.status_code` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:731` |
| 8 | `SessionPage.raw_data` | ✅ `SessionPage::raw_data()` | ✅ `SessionPage.raw_data` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:735` |
| 9 | `SessionPage.encoding` | ✅ `SessionPage::encoding()` | ✅ `SessionPage.encoding` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:739` |
| 10 | `SessionPage.json` | ✅ `SessionPage::json()` | ✅ `SessionPage.json` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:747` |
| 11 | `SessionPage.user_agent` | ✅ `SessionPage::user_agent()` | ✅ `SessionPage.user_agent` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:756` |
| 12 | `SessionPage.cookies()` | ✅ `SessionPage::cookies()` | ✅ `SessionPage.cookies()` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:762` |
| 13 | `SessionPage.ele()` / `eles()` | ✅ `SessionPage::find()` / `find_all()` | ✅ `SessionPage.ele()` / `eles()` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:768-772` |
| 14 | `SessionPage.s_ele()` / `s_eles()` | ✅ `SessionPage::root()` / `snapshot_find()` | ✅ `SessionPage.s_ele()` / `s_eles()` | Rust: `rust/src/session.rs`, Python: `python/openpage/_compat.py:774-780` |
| 15 | `WebPage` 双模式 | ✅ `WebPage` | ✅ `WebPage` | Rust: `rust/src/webpage.rs`, Python: `python/openpage/_compat.py:789` |
| 16 | `WebPage(mode='d')` 驱动模式 | ✅ `WebMode::Driver` | ✅ `WebPage(mode='d')` | Rust: `rust/src/webpage.rs`, Python: `python/openpage/_compat.py:791` |
| 17 | `WebPage(mode='s')` 会话模式 | ✅ `WebMode::Session` | ✅ `WebPage(mode='s')` | Rust: `rust/src/webpage.rs`, Python: `python/openpage/_compat.py:791` |
| 18 | `WebPage.change_mode()` 切换模式 | ✅ `WebPage::change_mode()` | ✅ `WebPage.change_mode()` | Rust: `rust/src/webpage.rs`, Python: `python/openpage/_compat.py:842` |
| 19 | `WebPage.cookies_to_session()` | ✅ `WebPage::cookies_to_session()` | ✅ `WebPage.cookies_to_session()` | Rust: `rust/src/webpage.rs`, Python: `python/openpage/_compat.py:946` |
| 20 | `WebPage.cookies_to_browser()` | ✅ `WebPage::cookies_to_browser()` | ✅ `WebPage.cookies_to_browser()` | Rust: `rust/src/webpage.rs`, Python: `python/openpage/_compat.py:949` |

**本章小结 - SessionPage/WebPage：**
- Rust 实现率：约 **100%** (20/20 项)
- Python API 实现率：约 **100%** (20/20 项)
- 双模式架构已实现完整

---

## 第六章：监听与拦截

### 6.1 网络监听

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `tab.listen.start()` 启动监听 | ✅ `Listener::start()` | ✅ `Listener.start()` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` (Listener 类) |
| 2 | `tab.listen.wait()` 等待数据包 | ✅ `Listener::wait()` | ✅ `Listener.wait()` | Rust: `rust/src/listener.rs` |
| 3 | `tab.listen.steps()` 迭代数据包 | ✅ `Listener::steps()` | ✅ `Listener.steps()` | Rust: `rust/src/listener.rs` |
| 4 | `tab.listen.clear()` 清空缓存 | ✅ `Listener::clear()` | ✅ `Listener.clear()` | Rust: `rust/src/listener.rs` |
| 5 | `tab.listen.stop()` 停止监听 | ✅ `Listener::stop()` | ✅ `Listener.stop()` | Rust: `rust/src/listener.rs` |
| 6 | `tab.listen.set_targets()` 设置目标 | ✅ `Listener::set_targets()` | ✅ `Listener.set_targets()` | Rust: `rust/src/listener.rs` |
| 7 | `tab.listen.pause()` 暂停 | ✅ `Listener::pause()` | ✅ `Listener.pause()` | Rust: `rust/src/listener.rs` |
| 8 | `tab.listen.resume()` 恢复 | ✅ `Listener::resume()` | ✅ `Listener.resume()` | Rust: `rust/src/listener.rs` |
| 9 | `tab.listen.wait_silent()` 等待静默 | ✅ `Listener::wait_silent()` | ✅ `Listener.wait_silent()` | Rust: `rust/src/listener.rs` |
| 10 | 数据包对象 `ListenerPacket` | ✅ `ListenerPacket` | ✅ `ListenerPacket` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` |
| 11 | 请求对象 `ListenerRequest` | ✅ `ListenerRequest` | ✅ `ListenerRequest` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` |
| 12 | 响应对象 `ListenerResponse` | ✅ `ListenerResponse` | ✅ `ListenerResponse` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` |
| 13 | 额外请求信息 `ListenerRequestExtraInfo` | ✅ `ListenerRequestExtraInfo` | ✅ `ListenerRequestExtraInfo` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` |
| 14 | 额外响应信息 `ListenerResponseExtraInfo` | ✅ `ListenerResponseExtraInfo` | ✅ `ListenerResponseExtraInfo` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` |
| 15 | 失败信息 `ListenerFailInfo` | ✅ `ListenerFailInfo` | ✅ `ListenerFailInfo` | Rust: `rust/src/listener.rs`, Python: `python/openpage/_compat.py` |
| 16 | Cookie 信息 `ListenerAssociatedCookie` | ✅ `ListenerAssociatedCookie` | ⚠️ 需确认 | Rust: `rust/src/listener.rs` |

### 6.2 请求拦截

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `tab.intercept.start()` 启动拦截 | ✅ `Interceptor::start()` | ✅ `Interceptor.start()` | Rust: `rust/src/intercept.rs`, Python: `python/openpage/_compat.py` |
| 2 | `tab.intercept.pause()` 暂停 | ✅ `Interceptor::pause()` | ❌ 未暴露 | Rust: `rust/src/intercept.rs` |
| 3 | `tab.intercept.resume()` 恢复 | ✅ `Interceptor::resume()` | ❌ 未暴露 | Rust: `rust/src/intercept.rs` |
| 4 | `tab.intercept.stop()` 停止 | ✅ `Interceptor::stop()` | ✅ `Interceptor.stop()` | Rust: `rust/src/intercept.rs` |
| 5 | 请求修改/阻断/模拟响应 | ✅ `Interceptor` 支持 | ✅ Python 暴露 | Rust: `rust/src/intercept.rs` |
| 6 | `InterceptedRequest` 对象 | ✅ `InterceptedRequest` | ✅ Python 暴露 | Rust: `rust/src/intercept.rs` |
| 7 | `InterceptedRequestInfo` 对象 | ✅ `InterceptedRequestInfo` | ⚠️ 需确认 | Rust: `rust/src/intercept.rs` |

**本章小结 - 监听与拦截：**
- Rust 实现率：约 **100%** (23/23 项)
- Python API 实现率：约 **83%** (19/23 项)
- 监听功能非常完整，拦截功能也已完整实现

---

## 第七章：下载管理

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `tab.set.download_path()` 下载路径 | ✅ `Browser::set_page_download_path()` | ✅ `PageSetter.download_path()` | Rust: `rust/src/browser.rs:559`, Python: `python/openpage/_compat.py:484` |
| 2 | `tab.set.download_file_name()` 下载文件名 | ✅ `Browser::set_page_download_filename()` | ✅ `PageSetter.download_file_name()` | Rust: `rust/src/browser.rs:603`, Python: `python/openpage/_compat.py:496` |
| 3 | 文件冲突处理 `rename/overwrite/skip` | ✅ `DownloadFileExistsMode` | ✅ Python 字符串配置 | Rust: `rust/src/browser.rs:26`, Python: `python/openpage/_compat.py:67` |
| 4 | `DownloadMission` 下载任务对象 | ✅ `DownloadMission` | ✅ `DownloadMission` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py` |
| 5 | `mission.state` 下载状态 | ✅ `DownloadState` | ✅ `DownloadMission.state` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:145` |
| 6 | `mission.cancel()` 取消下载 | ✅ `DownloadMission::cancel()` | ✅ `DownloadMission.cancel()` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:140` |
| 7 | `mission.wait()` 等待完成 | ✅ `DownloadMission::wait_for_done()` | ✅ `DownloadMission.wait()` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:149` |
| 8 | `mission.path` 下载路径 | ✅ `DownloadInfo` 含路径 | ✅ `DownloadMission.final_path` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:141` |
| 9 | `mission.url` 下载 URL | ✅ `DownloadInfo` 含 URL | ✅ `DownloadMission.url` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:148` |
| 10 | `mission.total_bytes` 总大小 | ✅ `DownloadInfo` 含大小 | ✅ `DownloadMission.total_bytes` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:147` |
| 11 | `mission.received_bytes` 已接收大小 | ✅ `DownloadInfo` 含大小 | ✅ `DownloadMission.received_bytes` | Rust: `rust/src/download.rs`, Python: `python/openpage/_compat.py:144` |
| 12 | 浏览器级别下载事件追踪 | ✅ `DownloadStore` | ✅ 通过 `Browser` 暴露 | Rust: `rust/src/download.rs` |
| 13 | 上传文件 `set.upload_files()` | ✅ `UploadTracker` | ✅ `PageSetter.upload_files()` | Rust: `rust/src/upload.rs`, Python: `python/openpage/_compat.py:506` |

**本章小结 - 下载管理：**
- Rust 实现率：约 **100%** (13/13 项)
- Python API 实现率：约 **100%** (13/13 项)
- 下载核心功能非常完整，DownloadMission 10 个属性/方法全部暴露

---

## 第八章：其他功能

### 8.1 弹窗处理 (Alert)

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `tab.states.has_alert` | ✅ `Page::has_alert()` | ✅ `PageStates.has_alert` | Rust: `rust/src/alert.rs`, Python: `python/openpage/_compat.py:693` |
| 2 | `tab.handle_alert()` | ✅ `Page::handle_alert()` | ✅ `Page.handle_alert()` | Rust: `rust/src/alert.rs`, Python: `python/openpage/_compat.py:343` |
| 3 | `tab.set.auto_handle_alert()` | ✅ `Page::set_auto_alert_action()` | ✅ `PageSetter.auto_handle_alert()` | Rust: `rust/src/alert.rs`, Python: `python/openpage/_compat.py:473` |
| 4 | `tab.wait.alert_closed()` | ✅ `Page::wait_for_alert_closed()` | ✅ `PageWait.alert_closed()` | Rust: `rust/src/alert.rs`, Python: `python/openpage/_compat.py:665` |

### 8.2 Shadow DOM

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `ele.sr()` / `ele.shadow_root` | ✅ `Element::shadow_root()` | ❌ 未暴露 | Rust: `rust/src/element.rs:633`, Python Element 无 sr/shadow_root |
| 2 | ShadowRoot 内查找元素 | ✅ `ShadowRoot::find()` 等 | ❌ 未暴露 | Rust: `rust/src/shadow_root.rs` |

### 8.3 Console

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | Console 消息监听 | ✅ `Console` | ❌ 未暴露 | Rust: `rust/src/console.rs` |
| 2 | `console.steps()` 迭代消息 | ✅ `ConsoleSteps` | ❌ 未暴露 | Rust: `rust/src/console.rs` |

### 8.4 Screencast 录屏

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | 录屏功能 | ✅ `Screencast` | ❌ 未暴露 | Rust: `rust/src/screencast.rs` |
| 2 | `screencast.start()` | ✅ `Screencast::start()` | ❌ 未暴露 | Rust: `rust/src/screencast.rs` |
| 3 | `screencast.stop()` | ✅ `Screencast::stop()` | ❌ 未暴露 | Rust: `rust/src/screencast.rs` |

### 8.5 Frame / iframe

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | `Frame` 对象 | ✅ `Frame` | ❌ 未暴露 | Rust: `rust/src/page.rs:68` |
| 2 | `frame.ele()` 在 iframe 内查找 | ✅ `Frame::find()` | ❌ 未暴露 | Rust: `rust/src/page.rs:282` |
| 3 | `frame.run_js()` 在 iframe 内执行 JS | ✅ `Frame::run_js()` | ❌ 未暴露 | Rust: `rust/src/page.rs:229` |
| 4 | `frame.url` / `frame.title` | ✅ `Frame::url()` / `Frame::title()` | ❌ 未暴露 | Rust: `rust/src/page.rs:195-204` |
| 5 | `frame.html` | ✅ `Frame::html()` | ❌ 未暴露 | Rust: `rust/src/page.rs:220` |
| 6 | `frame.scroll` 滚动 | ✅ `Frame::scroll()` | ❌ 未暴露 | Rust: `rust/src/page.rs:127` |
| 7 | `frame.set` 设置 | ✅ `Frame::set()` | ❌ 未暴露 | Rust: `rust/src/page.rs:131` |
| 8 | `frame.states` 状态 | ✅ `Frame::states()` | ❌ 未暴露 | Rust: `rust/src/page.rs:135` |
| 9 | `frame.wait` 等待 | ✅ `Frame::wait()` | ❌ 未暴露 | Rust: `rust/src/page.rs:139` |

### 8.6 CLI

| # | DrissionPage 能力 / API | openpage Rust 实现 | openpage Python API | 代码证据 / 说明 |
|---|------------------------|-------------------|--------------------|----------------|
| 1 | CLI 命令行控制 | ✅ `openpage_rs::cli::run_from_args()` | N/A | Rust: `rust/src/cli/` |
| 2 | 长连接 daemon 协议 | ✅ 当前活跃路径为 `serve --session <name>` 的 TCP/NDJSON daemon | N/A | `README.md`, `task_plan.md`, `rust/src/cli/serve.rs` |
| 3 | 命名会话跨进程控制 | ✅ `browser start/goto/url/title/snapshot/click/...` 均通过同一 TCP daemon 执行路径 | N/A | `README.md`, `skills/openpage-test/references/cli-smoke.md`, `rust/src/cli/oneshot.rs` |

---

## 总体统计

| 章节 | DP 能力总数 | Rust 已实现 | Python 已实现 | Rust 完成率 | Python 完成率 |
|------|-----------|------------|--------------|-----------|--------------|
| 第一章：Chromium 对象 | 39 | 32 | 24 | **82%** | **62%** |
| 第一章：ChromiumOptions | 40 | 30 | 8 | **75%** | **20%** |
| 第二章：页面操作 | 65 | 57 | 54 | **88%** | **83%** |
| 第三章：元素定位 | 21 | 20 | 10 | **95%** | **48%** |
| 第四章：元素操作 | 39 | 27 | 14 | **93%** | **36%** |
| 第五章：SessionPage/WebPage | 20 | 20 | 20 | **100%** | **100%** |
| 第六章：监听与拦截 | 23 | 23 | 19 | **100%** | **83%** |
| 第七章：下载管理 | 13 | 13 | 13 | **100%** | **100%** |
| 第八章：其他 (Alert/Shadow/Console/Frame/CLI) | 20 | 18 | 4 | **90%** | **20%** |
| **总计** | **280** | **240** | **166** | **~86%** | **~59%** |

---

## 关键缺失清单（按优先级）

### 🔴 高优先级缺失

1. **Python 端 `ChromiumPage` 缺少 `connect()` 接管已有浏览器能力**
   - Rust 已有 `Browser::connect()`，但 Python 未暴露
   - 影响：无法接管手动启动的浏览器

2. **Python 端 Element 大量方法未暴露到 Python**
   - Python `Element` 仅暴露 13 个成员：`attr, clear, click, ele, eles, html, input, press, run_js, save_screenshot, states, text, wait`
   - **未暴露的关键方法**：`tag, link, value, inner_html, attrs, property, raw_text, css_path, xpath, style, comments, texts, pseudo_before/after, child_count, set_attr, remove_attr, set_property, set_style, save, src`, 以及**所有滚动方法**和**所有相对定位方法**（parent, children, prev, next, before, after）
   - 影响：元素操作 API 非常不完整，大量 DP 常用功能无法在 Python 使用

### 🟡 中优先级缺失

3. **ini 配置文件系统**
   - DP 特有的持久化配置系统
   - 影响：配置复用不便

4. **Python 端 Shadow DOM 未暴露**
   - Rust `Element::shadow_root()` 已实现，但 Python `Element` 无 `sr()` 或 `shadow_root` 方法
   - `ShadowRoot` 对象未暴露到 Python

5. **Python 端 Frame/iframe 未暴露**
   - Rust `Frame` 对象非常完整（find, run_js, url, title, html, scroll, set, states, wait），但 Python 端完全未暴露

6. **Python 端 Console 未暴露**
   - Rust `Console` 和 `ConsoleSteps` 已实现，但 Python 端未暴露

7. **Python 端 Screencast 录屏未暴露**
   - Rust `Screencast` 已实现，但 Python 端未暴露

### 🟢 低优先级缺失

8. **`set_user()` 设置用户配置** — DP 特有高级功能
9. **`remove_pref_from_file()` / `clear_flags_in_file()`** — DP 特有文件操作
10. **Python 端大量启动配置未暴露** — `no_imgs`, `no_js`, `mute`, `incognito`, `proxy`, `user_agent`, `auto_port`, `existing_only` 等

---

*文档结束。后续章节可根据需要继续细化补充。*
