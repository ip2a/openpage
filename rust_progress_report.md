# OpenPage Rust 版本进度审查报告

> 历史文档说明（2026-05-29）：本报告保留的是 **协议迁移前** 的阶段性审查结论。
> 其中关于 `serve --stdio`、one-shot attach、`page get/page url/page title/page screenshot`
> 的描述都已 **不再代表当前主实现**。当前权威状态请以：
> - `task_plan.md`
> - `notes.md`
> - `claude-progress.txt`
> - `rust/src/cli/*`
> 为准。

日期: 2026-05-21

更新: 2026-05-23 最新代码复核已补充进本报告，尤其是 CLI one-shot attach 与 `serve --stdio` 的对照实测结果。

## 结论摘要

当前 `openpage_rs` 已经不是“只完成了底层替换”的阶段，而是一个可以独立运行的 Rust 包：

- Rust core 已具备浏览器驱动、HTTP session、混合 `WebPage`、下载、上传、监听、拦截、告警、窗口控制、CLI 的完整骨架。
- 不依赖 Python 也能直接运行。`Cargo.toml` 已把 crate 定义为独立包，并提供默认二进制入口 `openpage`。
- CLI 已经可用，且分成两条路径：
  - `serve --stdio`：适合 agent 长连接控制。
  - `browser/page/ele/js`：适合一条条命令跨进程控制同一个浏览器 session。
- 就“Rust 是否已经能单独跑起来”这个问题，答案是：**能**。
- 就“Rust 是否已经把所有内核能力都通过 CLI 暴露出来”这个问题，答案是：**还没有完全暴露，CLI 目前是可用但未全量铺开**。

## 审查发现

### 0. 最新复核结论：Rust core 的“进程内控制”明显强于 one-shot attach 控制

严重级别: `high`

最新实测结论:

- `serve --stdio` 路径：
  - 创建页面成功
  - 打开百度成功
  - 读取标题成功
  - 截图成功
  - 截图内容正确
- one-shot named session 路径：
  - `browser start` 成功
  - `page get https://www.baidu.com` 成功
  - `page url` 成功
  - 但 `page title` 返回 `Cannot find context with specified id`
  - `page screenshot` 虽然返回成功，但截图内容是白屏

这说明当前项目的真实状态不是“CLI 全面可用”，而是：

- **Rust core 进程内控制可用**
- **`serve --stdio` 可作为稳定 agent 控制面**
- **one-shot attach CLI 仍然不稳定，不能视为真实完成**

这条结论比之前的“one-shot 基本可用”更准确，应该作为当前阶段判断基线。

### 1. `page get` 在真实复杂站点上会出现“实际已打开，但命令返回超时”的假失败

严重级别: `medium`

代码位置:

- `rust/src/cli/oneshot.rs:72`
- `rust/src/page.rs:78`

说明:

- one-shot CLI 的 `page get` 直接调用 `Page::goto()`。
- `Page::goto()` 在默认 `LoadMode::Normal` 下，会等待 `doc_loaded + dom_ready`。
- 对百度这类真实站点，这个等待条件偏严格。实际页面已经打开、标题也可读、截图也成功，但 `page get` 仍可能返回 `Request timed out.`。

这不是核心浏览器控制失败，而是 CLI 成功语义过于严格，导致 agent 看到错误时会误判任务失败。

我已现场复现：

- `page title` 返回 `百度一下，你就知道`
- `page url` 返回 `https://www.baidu.com/`
- 截图成功
- 但 `page get https://www.baidu.com` 首次返回过超时

建议:

- 为 CLI 增加 `--load-mode eager/none`
- 或在 `page get` 中把“URL 已变化 + 标题可读”视为可接受成功
- 或增加 `page wait` / `page get --allow-timeout-if-navigated` 这类更明确的策略

补充:

- 2026-05-23 的最新实测里，百度 `page get` 已经可以返回成功
- 但 one-shot attach 紧接着又暴露出更严重的问题：页面 execution context 恢复不稳定
- 所以当前问题已经不只是“成功语义太严格”，而是“attach 后 page 对象的可操作性不稳定”

### 1.1 one-shot attach 后的 page context 恢复仍然不稳定

严重级别: `high`

代码位置:

- `rust/src/cli/oneshot.rs:239`
- `rust/src/page.rs:143`
- `rust/src/page.rs:361`

说明:

- `open_page()` 在每条命令执行时重新 `Browser::connect()`，再从 `target_id` 恢复 `Page`
- `page url` 使用的是 `inner.url()`，它能成功
- `page title` 使用的是 `inner.get_title()`，它在最新实测中失败，报 `Cannot find context with specified id`
- `page screenshot` 走的是 CDP screenshot，命令返回成功，但截图内容是白屏

这说明当前 one-shot 模式恢复出来的 `Page`：

- 不一定有稳定的 execution context
- 不一定处在真正可交互的可视页面状态
- “命令成功”不等于“页面状态正确”

这已经不是小缺口，而是 one-shot attach 方案当前最核心的真实性问题。

建议:

- 优先把 agent 主控制路径收敛到 `serve --stdio`
- one-shot CLI 暂时只当作辅助调试工具，而不是高置信主通路
- 如果要继续做 one-shot：
  - 需要显式恢复或重建 page context
  - 需要在 attach 后做可交互性校验，而不是只校验 `target_id`
  - 需要增加“标题正确 + DOM 可读 + 截图非空白”的联合验证

### 2. one-shot attach 每次都会重置浏览器下载行为，并创建新的临时下载目录

严重级别: `medium`

代码位置:

- `rust/src/browser.rs:199`
- `rust/src/browser.rs:223`
- `rust/src/browser.rs:779`
- `rust/src/browser.rs:602`

说明:

- `Browser::connect()` 每次 attach 都会调用 `make_temp_download_dir()`
- 随后立刻执行 `configure_download_behavior(...)`
- 这意味着每次 CLI attach 到已有浏览器，都会重新设置浏览器级下载目录

问题有两个：

- one-shot CLI 命令之间会不断改写浏览器的下载行为
- attach 过程中创建的临时下载目录只在 `browser.close()` 时清理，普通 `page title` / `page url` / `ele text` 这种 attach 后直接退出的命令不会触发清理

这点已经从本机临时目录看到残留的 `openpage-downloads-*` 目录。

建议:

- `Browser::connect()` 不要默认重设下载行为
- 把 attach 场景和 launch 场景拆开
- 只有真正执行下载相关命令时再初始化下载配置
- 对 attach 模式创建的临时目录使用显式清理策略

### 3. 命名 session 文件没有锁，也不是原子写入

严重级别: `medium`

代码位置:

- `rust/src/cli/oneshot.rs:271`
- `rust/src/cli/oneshot.rs:282`

说明:

- session 元数据通过 JSON 文件保存在 `OPENPAGE_HOME/sessions/<name>.json`
- `save_session()` 直接 `fs::write()`
- `load_session()` 直接 `fs::read()`
- 没有文件锁、没有原子 rename、没有并发写保护

这对单 agent、串行命令通常够用，但如果后面真的让多个 agent 或多个进程同时操作同一个 session，就可能出现：

- session 文件被覆盖
- target id 回退
- 中途写坏 JSON
- `stop` / `page new` / `page get` 并发时状态不一致

建议:

- 至少改为“写临时文件后 rename”
- 更稳妥的做法是增加 per-session 文件锁

### 4. CLI 暴露面已经不小，但自动化测试仍然偏浅

严重级别: `medium`

代码位置:

- `rust/src/cli/protocol.rs:72`
- `rust/src/cli/oneshot.rs:462`
- `rust/src/cli/serve.rs:61`

说明:

- Rust 当前总共有 `16` 个单元测试
- CLI 相关自动化测试目前主要是：
  - 协议结构序列化/反序列化
  - 参数解析
- 但 `serve --stdio` 的大量 dispatch 分支，以及 one-shot session 的跨进程行为，主要还是靠手动 smoke test 验证

这意味着：

- CLI 现在是“能用的”
- 但还不是“高置信度回归安全”的状态

建议:

- 增加 `openpage` bin 级别 integration test
- 至少覆盖：
  - `serve --stdio` 的 `create/get/title/element/shutdown`
  - `browser start -> page get -> title -> screenshot -> stop`
  - headless / headed 两种 session 模式

## 当前 Rust 实现范围

### 1. crate 结构

代码位置:

- `rust/Cargo.toml:1`
- `rust/src/lib.rs:1`

现状:

- crate 名称: `openpage_rs`
- edition: `2024`
- 产物类型: `cdylib + rlib`
- 默认二进制入口: `openpage`
- `python-module` 只是可选 feature，不是 Rust 运行前提

Rust `src` 目录当前共有 `21` 个源文件。

公开模块包括：

- `alert`
- `browser`
- `cli`
- `download`
- `element`
- `error`
- `intercept`
- `listener`
- `locator`
- `page`
- `session`
- `upload`
- `webpage`
- `window`
- `python` 为 feature-gated

### 2. Browser 层

代码位置:

- `rust/src/browser.rs`

已实现能力:

- 启动 Chromium
- attach 到已有 CDP debugger URL
- 新建 page
- 获取 page 列表 / 指定 page
- 浏览器级下载目录控制
- 下载冲突策略 `rename / overwrite / skip`
- 页面级下载目录和文件名覆盖
- 下载任务跟踪 / 等待 / 取消
- load mode 默认值和运行时切换
- tab 枚举、版本、存活状态、隐身状态

成熟度判断:

- `high`

备注:

- 这是当前 Rust 版本最完整、最核心的能力层

### 3. Page / Element 层

代码位置:

- `rust/src/page.rs`
- `rust/src/element.rs`

已实现能力:

- 导航 `goto`
- 读取 `url / title / html`
- `find / find_all`
- `click / input / clear`
- `run_js`
- `save_screenshot`
- `save_pdf`
- waiter 系列
- headers / localStorage / sessionStorage
- user agent override
- upload files
- blocked urls
- window state / size / location / max / min / full / normal / hide / show`
- `activate()`
- `listener()`
- `interceptor()`
- alert handling
- snapshot `find / find_all / root`

成熟度判断:

- `high`

备注:

- 从能力密度看，这一层已经不是“最小替代实现”，而是比较完整的浏览器页面操作层

### 4. Session 层

代码位置:

- `rust/src/session.rs`

已实现能力:

- `GET`
- `POST JSON`
- cookies jar
- user agent
- 自定义 headers
- `url / status_code / encoding / raw_data / json / html`
- 快照查找和节点遍历

成熟度判断:

- `high`

备注:

- Session 路径已经能支撑 `WebPage` 的 driver/session 双模式

### 5. `WebPage` 混合层

代码位置:

- `rust/src/webpage.rs:132`

已实现能力:

- 同时持有 `Browser + Page + SessionPage`
- 模式切换 `Driver <-> Session`
- cookies 从 browser 同步到 session
- cookies 从 session 同步到 browser
- 统一暴露：
  - `get / post_json`
  - `url / title / html / json / cookies`
  - `find / find_all`
  - `run_js`
  - 截图
  - 下载设置
  - upload / blocked urls
  - alert
  - waiters
  - window 控制
  - listener / interceptor

成熟度判断:

- `high`

备注:

- `WebPage` 已经是 Rust 版最重要的统一抽象
- 从架构上看，Python 现在更像是这个 Rust core 的绑定层，而不是主实现层

### 6. 辅助子系统

代码位置:

- `rust/src/download.rs`
- `rust/src/listener.rs`
- `rust/src/intercept.rs`
- `rust/src/alert.rs`
- `rust/src/upload.rs`
- `rust/src/window.rs`

已实现子系统:

- 下载任务模型
- 请求监听
- 请求拦截 / rewrite / block / fulfill
- alert 状态跟踪与处理
- 文件上传
- 窗口可见性与激活

成熟度判断:

- `medium-high`

备注:

- 核心功能基本齐了
- 但这部分比页面基础操作更依赖集成测试，当前自动化验证还不够深

## 缺口清单

这一节只写“还没完成”或者“还不够稳”的部分，不重复已完成能力。

### 1. Rust core 仍然缺的部分

- one-shot attach 场景还没有专门的轻量连接路径，`Browser::connect()` 仍然复用了下载初始化逻辑。
- `Page::goto()` 对真实站点的成功判定仍然偏严格，复杂站点容易出现“已打开但报超时”。
- 目前没有把 CLI / attach / launch 三种运行模式彻底分层，导致一部分浏览器生命周期逻辑仍然交叉。
- session 元数据持久化没有锁、没有原子写、没有冲突恢复。

这些都不是“功能不存在”，而是“工程化边界还不够硬”。

### 2. Rust CLI 还缺的部分

Rust CLI 当前能用，但不是 Rust core 的全量外壳。明确还缺这些：

- one-shot CLI 没有 `page pdf`
- one-shot CLI 没有完整 listener 命令族
- one-shot CLI 没有完整 interceptor 命令族
- one-shot CLI 没有 snapshot 命令族
- one-shot CLI 没有多 tab 的显式命令面
- one-shot CLI 没有 `load_mode` 级别的命令参数暴露，真实站点导航策略仍然比较死
- one-shot CLI 没有 attach 后 context 健康检查与自动恢复逻辑
- one-shot CLI 没有截图内容正确性的回归验证

`serve --stdio` 也不是全覆盖：

- 没有把 listener lifecycle 完整协议化
- 没有把 interceptor lifecycle 完整协议化
- 没有把 snapshot 能力完整协议化
- 没有对象级 element registry；当前 element 操作以 locator 即时执行为主

### 3. 测试层还缺的部分

- 缺少 bin 级 integration tests
- 缺少真实跨进程 session 生命周期自动化验证
- 缺少 CLI 错误语义回归测试
- 缺少多 session / 并发 session 冲突验证
- 缺少 attach 模式资源清理验证

### 4. 文档层还缺的部分

- 还没有单独的 Rust CLI 协议文档
- 还没有“Rust core vs CLI vs Python wrapper”三层边界文档
- 还没有 agent 接入约定文档

## Python 端结合方式

这一节回答两个问题：

1. Python 现在和 Rust 是怎么接上的  
2. Python 现在到底是“薄封装”到什么程度

### 1. 当前集成结构

代码位置:

- `python/pyproject.toml:5`
- `python/openpage/_compat.py:8`
- `rust/src/python.rs:2576`

当前结构是：

- Rust crate `openpage_rs` 通过 PyO3 暴露 Python extension module
- Python 包 `openpage` 负责兼容层和旧 API 形状
- Python 用户入口并不是直接操作裸 PyO3 类，而是通过 `_compat.py` 里的 facade 类

也就是说：

- **Rust 是主实现**
- **PyO3 是桥**
- **Python 是兼容包装层**

这和“Rust 端完整可用，Python 后续只做很薄的封装”这个目标是基本一致的。

### 2. Python 到 Rust 的调用链

#### Browser

代码位置:

- `python/openpage/_compat.py:111`

Python `Browser.launch(...)` 最终直接调用：

- `_openpage_rs.Browser.launch(...)`

也就是：

- 浏览器启动逻辑在 Rust
- Python 只负责把 Python 风格参数转成 Rust 参数

#### WebPage

代码位置:

- `python/openpage/_compat.py:789`
- `python/openpage/_compat.py:800`

Python `WebPage(...)` 最终直接调用：

- `_openpage_rs.WebPage.create(...)`

也就是：

- `WebPage` 的 driver/session 双持有、模式切换、cookie 同步、waiters 主逻辑都在 Rust

#### Python extension 暴露面

代码位置:

- `rust/src/python.rs:2576`

PyO3 当前注册的核心类型包括：

- `PyBrowser`
- `PyPage`
- `PyElement`
- `PySessionPage`
- `PySessionElement`
- `PyWebPage`
- `PyListener`
- `PyInterceptor`
- `PyInterceptedRequest`
- `PyDownloadMission`
- listener request/response packet 相关类型

这说明 Python 端访问的大部分核心对象，底层已经都有 Rust 类型承接。

### 3. Python 端现在到底有多“薄”

结论先说：

- **已经明显比传统双实现结构薄很多**
- **但还不是“零逻辑转发”**

Python 侧仍然保留了这些兼容逻辑：

- 参数归一化
  - `_normalize_listener_values()`
  - `_normalize_url_patterns()`
  - `_normalize_upload_files()`
- `wait / states / set` facade 组合
- 返回值形状适配
- 包装对象转换
- 少量 JSON decode

具体代码位置:

- `python/openpage/_compat.py:13`
- `python/openpage/_compat.py:149`
- `python/openpage/_compat.py:304`
- `python/openpage/_compat.py:825`
- `python/openpage/_compat.py:892`
- `python/openpage/_compat.py:1250`

这类逻辑的性质不是“业务主实现”，而是：

- 兼容旧 Python API 习惯
- 把 Rust 暴露的原始对象包装成更接近原 openpage 风格的接口

所以准确表述应该是：

> Python 端现在已经不是主实现层，但仍然承担着一层不算小的兼容适配职责。

### 4. Python 端目前还缺什么

这部分要写清楚，因为它直接关系到你后续是不是还需要继续收薄 Python。

#### 已缺失但符合你当前要求的部分

- 没有 Python CLI 封装
- 没有 Python 版 agent client
- 没有 Python 对 Rust CLI 的 stdio client

这并不是问题，因为你当前明确要求的是：

- Rust 必须独立可用
- Python 暂时不优先

所以从当前目标看，这些“缺失”是可接受的。

#### 如果以后要把 Python 真正收成“极薄封装”，还需要继续做的部分

- 把 `_compat.py` 里更多 facade 逻辑继续下沉
- 减少 Python 端对返回值形状的再加工
- 尽量把 listener/interceptor 的参数归一化也下沉到 Rust 或 PyO3 边界
- 明确哪些 Python API 是“兼容历史接口”，哪些是“新 Rust 原生接口”

### 5. 当前 Python-Rust 结合状态判断

可以把当前状态分成三句话：

- Python 已经依赖 Rust extension，而不是自己再实现一套浏览器核心
- Rust 已经是单一事实来源的大方向
- Python 仍然保留了一层兼容 API 适配，不是纯机械转发

这意味着：

- 从架构方向看，迁移是成功的
- 从收口程度看，仍然还有“最后一层兼容壳”没有彻底变薄

## Rust CLI 当前到什么程度

### 1. 架构

代码位置:

- `rust/src/cli/mod.rs`
- `rust/src/cli/args.rs`
- `rust/src/cli/serve.rs`
- `rust/src/cli/oneshot.rs`
- `rust/src/bin/openpage.rs`

当前 CLI 不是单独包，而是：

- Rust crate 内嵌模块 `openpage_rs::cli`
- 外加一个便于本地调试的 `openpage` bin

这个设计和你的要求一致。

### 2. 已经实现的 CLI 模式

#### `serve --stdio`

适用场景:

- agent 长连接控制
- 一个进程里保留对象注册表
- 适合复杂交互

当前已支持的方向:

- `webpage.create`
- `webpage.get`
- `webpage.post_json`
- `webpage.change_mode`
- `webpage.url`
- `webpage.title`
- `webpage.html`
- `webpage.json`
- `webpage.cookies`
- `webpage.user_agent`
- `webpage.status_code`
- `webpage.run_js`
- element `text/html/attr/click/input/clear/run_js/screenshot`
- waiters
- alert
- 下载设置
- window 控制
- storage / headers / user_agent 设置
- `daemon.shutdown`

成熟度判断:

- `medium-high`

评价:

- 已经能支撑 agent 的基本控制闭环
- 但还没有把 Rust core 的所有能力都暴露到协议层

#### one-shot named session CLI

适用场景:

- shell 一条条命令控制浏览器
- 跨进程保留浏览器 session

当前已支持的命令:

- `browser start`
- `browser stop`
- `browser status`
- `page new`
- `page get`
- `page url`
- `page title`
- `page html`
- `page screenshot`
- `ele text`
- `ele html`
- `ele click`
- `ele input`
- `ele attr`
- `js`

成熟度判断:

- `medium`

评价:

- 已经“能用”
- 但离“完整 CLI 面”还有差距

### 3. Rust core 已有，但 CLI 还没完整暴露的能力

这部分是当前最重要的边界说明。

Rust core 已实现，但 CLI 还没有完整铺开的能力包括：

- `save_pdf()` 已在 `Page` 层实现，但 CLI 没有 `page pdf`
- `listener()` 已在 core 实现，但 CLI 没有完整网络监听命令族
- `interceptor()` 已在 core 实现，但 CLI 没有完整拦截命令族
- snapshot `root/find/find_all` 已在 core 实现，但 CLI 没有完整 snapshot 命令族
- 更细粒度的 tab 管理还没有作为 one-shot CLI 单独展开

这意味着当前状态是：

- **Rust core 完整度高**
- **CLI 完整度中等偏上**
- **CLI 还不是 Rust core 的 1:1 暴露**

## 验证情况

### 自动化验证

我确认的自动化验证包括：

- `cargo check --manifest-path rust/Cargo.toml`
- `cargo test --manifest-path rust/Cargo.toml`
- `cargo check --manifest-path rust/Cargo.toml --features python-module`

Rust 当前有 `16` 个单元测试。

### 文档与计划同步

现有文档已经声明：

- README 已记录 Rust CLI 用法
- `task_plan.md` 已把 Rust CLI 视为 `Phase 7 complete`

### 手动 smoke 验证

我这次实际跑过的 CLI 链路包括：

- `serve --stdio`
  - `webpage.create`
  - `webpage.get`
  - `webpage.title`
  - `element.text`
  - `daemon.shutdown`
- one-shot named session
  - `browser start`
  - `page get`
  - `page title`
  - `ele text`
  - `js`
  - `browser status`
  - `browser stop`
- 百度实测
  - 打开百度
  - 读取标题
  - 截图成功

## 2026-05-23 最新人工复核

### 1. 当前可确认的事实

- `cargo check --manifest-path rust/Cargo.toml` 通过
- `cargo test --manifest-path rust/Cargo.toml` 通过
- `cargo check --manifest-path rust/Cargo.toml --features python-module` 通过
- `serve --stdio` 路径下，百度标题读取和截图内容都正确
- one-shot named session 路径下，百度页面出现了“URL 正确、截图命令成功、但标题失败且截图白屏”的问题

### 2. 这意味着什么

- Rust core 本身不是坏的
- `serve --stdio` 更接近真实可用实现
- one-shot attach 目前更像“概念打通”，还不是稳定产品能力

## 我会怎么调试和确认

这一节只写我自己会真正执行的调试路径。

### 1. 先做分层确认

我不会一上来只跑 one-shot CLI，因为那样很容易把问题混成一团。

我的顺序会是：

1. `cargo check`
2. `cargo test`
3. `cargo check --features python-module`
4. `serve --stdio` 真实页面验证
5. one-shot named session 对照验证
6. Python wrapper 端到端验证

判断原则：

- 如果 `serve` 绿而 one-shot 红，问题在 attach/session 恢复
- 如果 Rust 绿而 Python 红，问题在 PyO3 边界或 `_compat.py`
- 如果截图文件存在但内容错，问题不是“命令执行失败”，而是页面状态恢复失败

### 2. 我会实际执行的命令

#### 基础构建与测试

```bash
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
cargo check --manifest-path rust/Cargo.toml --features python-module
```

#### `serve --stdio` 路径验证

启动：

```bash
cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio
```

发送请求示例：

```json
{"id":"1","op":"webpage.create","params":{"headless":true}}
{"id":"2","op":"webpage.get","target":"wp_1","params":{"url":"https://www.baidu.com"}}
{"id":"3","op":"webpage.title","target":"wp_1"}
{"id":"4","op":"page.screenshot","target":"wp_1","params":{"path":"/tmp/openpage-cli-artifacts/serve-baidu.png"}}
{"id":"5","op":"daemon.shutdown"}
```

#### one-shot named session 验证

```bash
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session review --replace --headless
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- page get https://www.baidu.com --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- page title --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- page screenshot /tmp/openpage-cli-artifacts/review-baidu.png --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser stop --session review
```

#### Python 端验证

```bash
bash ./scripts/dev_install.sh
python/.venv/bin/python -m unittest discover -s python/tests -v
python/.venv/bin/python python/examples/basic_usage.py
python/.venv/bin/python python/examples/webpage_modes.py
```

### 3. 我怎么做截图确认

我不会只检查“截图文件是否存在”。我会同时检查 3 件事：

1. 文件是否生成

```bash
ls -lh /tmp/openpage-cli-artifacts/serve-baidu.png
file /tmp/openpage-cli-artifacts/serve-baidu.png
```

2. 页面元数据是否正确

例如：

- `page url`
- `page title`

3. 截图内容本身是否正确

这一步必须直接看图。

当前最新复核结果：

- `/tmp/openpage-cli-artifacts/serve-baidu.png` 内容正确，是百度首页
- `/tmp/openpage-cli-artifacts/latest-review-baidu.png` 文件存在，但内容是白屏

所以“截图命令返回成功”在当前代码里还不能当成充分证据。

### 4. 我怎么定位 one-shot attach 的问题

我会按下面这组观察点去缩小范围：

1. `page get` 是否成功
2. `page url` 是否正确
3. `page title` 是否还能读取
4. `page html` 是否还能读取
5. `page screenshot` 是否非空白
6. `browser status` 的 `target` 是否稳定

如果出现这种组合：

- `url` 正确
- `title/html` 失败
- `screenshot` 空白

那我会优先怀疑：

- attach 后恢复出来的 target 不是当前真实可交互 page
- 或者 execution context 没恢复好
- 或者 page 恢复后还需要一次显式激活/等待/document-ready 校验

### 5. 现在我会给自己的判定标准

对于当前项目，我不会再把下面这些当成“真实完成”：

- 命令退出码是 0
- 返回 JSON 里 `ok: true`
- 截图文件存在

我会把“真实完成”定义成：

- URL 正确
- 标题正确
- HTML/元素可读
- 截图内容正确
- 同一条链路可重复跑通

## 当前完成度判断

### Rust core

完成度判断:

- `high`

理由:

- 浏览器驱动、页面操作、HTTP session、混合 `WebPage`、下载/上传/拦截/监听/alert/window 基本都在 Rust 内

### Rust 独立运行能力

完成度判断:

- `high`

理由:

- 不需要 Python 即可启动浏览器、导航、截图、执行 JS、做 session 持久化控制

### Rust CLI

完成度判断:

- `medium-high`

理由:

- 两种控制模式已经打通
- agent 可用
- 但 CLI 面尚未覆盖全部 Rust core 能力

### 回归安全度

完成度判断:

- `medium`

理由:

- 基础自动化验证已存在
- 但 CLI integration tests 深度不足
- 真实站点导航和并发 session 仍有边界问题

## 建议的下一步

建议按下面顺序推进，而不是继续无差别扩展功能：

1. 先修 one-shot CLI 的成功语义

- 重点解决真实站点导航“已打开但报超时”的假失败

2. 再修 attach 场景的下载行为污染和临时目录残留

- 把 `Browser::connect()` 与下载初始化解耦

3. 给 session 文件加原子写和文件锁

- 否则后面多 agent CLI 控制会出现状态竞争

4. 补 CLI integration tests

- 先覆盖 `serve --stdio`
- 再覆盖 one-shot session 生命周期

5. 最后再扩 CLI 面

- `page pdf`
- listener 命令族
- interceptor 命令族
- snapshot 命令族

## 最终判断

如果你的问题是：

- Rust 版本是否已经能独立运行？答案是：**可以**
- Rust 是否已经是当前实现主轴？答案是：**是**
- 现在是否已经可以让 agent 通过 CLI 控制浏览器？答案是：**可以，且已经现场验证**
- Rust 版本是否已经 100% 完整覆盖所有理想 CLI 能力？答案是：**还没有**

更准确的表述是：

> Rust core 已经进入“可用且功能密集”的阶段；CLI 已进入“可实战使用”的阶段；下一步工作的重点不再是证明 Rust 能不能跑，而是把 CLI 的成功语义、attach 生命周期和测试强度补到工程可长期维护的水平。
