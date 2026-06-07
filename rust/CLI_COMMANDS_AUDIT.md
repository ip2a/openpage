# OpenPage CLI 命令审计与缺口追踪

> 生成时间: 2026-05-31
> 对比对象: Playwright MCP 命令体系
> 原则: 底层已有能力优先暴露到 CLI；完全缺失的需底层开发。

---

## 一、已对齐（双方都有）

| 类别 | 对方命令 | OpenPage CLI 命令 | 状态 |
|------|---------|-------------------|------|
| 导航 | open, goto, back, reload | `browser start`, `goto`, `back`, `forward`, `reload [--ignore-cache]` | ✅ |
| 核心交互 | click, fill | `click`, `fill` | ✅ |
| 滚动 | scroll, scrollintoview | `scroll`, `scroll-into-view` | ✅ |
| 等待 | wait, wait --url, wait --title | `wait`, `wait-for-url`, `wait-for-title` | ✅ |
| 快照 | snapshot | `snapshot` | ✅ |
| 截图/归档 | screenshot, pdf, save page | `screenshot`, `pdf`, `save` | ✅ |
| 查询 | get title, get url | `title`, `url` | ✅ |
| 执行 | eval | `js` | ✅ |
| 对话框 | dialog accept, dialog dismiss | `alert accept`, `alert dismiss`, `alert has`, `alert text` | ✅ |
| 关闭 | close, quit, exit | `browser stop` | ✅ |

---

## 二、OpenPage 独有优势

| 功能 | 说明 |
|------|------|
| `@ref` 引用体系 | `snapshot` 后通过 `@e1` `@e5` 直接定位元素，对方无此概念 |
| 隐式 Daemon | 任何命令自动启动浏览器 + 5 分钟 idle 自动退出 |
| `serve` | NDJSON daemon / TCP 多连接模式，对方纯单次 CLI 调用 |
| `html` | 获取完整页面 HTML |
| `download` | URL 直下载到本地 |
| `intercept status` | 网络拦截状态查询 |
| `scroll` 细分方向 | `top`, `bottom`, `half`, `rightmost`, `leftmost`, `location` |
| `window size` | 设置窗口精确尺寸 |

---

## 三、本次已补齐（底层已有，CLI 新暴露）

全部经过编译 + 运行验证：

| 命令 | 示例 | 底层方法 |
|------|------|---------|
| `hover <locator>` | `openpage hover "#btn"` | `Element::hover` |
| `hover-at <locator> [--x --y]` | `openpage hover-at "#canvas" --x 30 --y 40` | `Element::hover_with_offset` |
| `press <locator> <key>` | `openpage press "#kw" "Enter"` | `Element::press_key` |
| `focus <locator>` | `openpage focus "#kw"` | `Element::focus` |
| `clear <locator>` | `openpage clear "#kw"` | `Element::clear` |
| `submit <locator>` | `openpage submit "#form"` | `Element::submit` |
| `check <locator>` | `openpage check "#agree"` | `Element::set_checked(true)` |
| `uncheck <locator>` | `openpage uncheck "#agree"` | `Element::set_checked(false)` |
| `right-click <locator>` | `openpage right-click "#menu"` | `Element::click_right` |
| `middle-click <locator>` | `openpage middle-click "#link"` | `Element::click_middle` |
| `double-click <locator>` | `openpage double-click "#row"` | `Element::click_multi(2)` |
| `drag <locator> --dx <x> --dy <y>` | `openpage drag "#knob" --dx 120 --dy 0` | `Element::drag` |
| `drag-to <source> <target>` | `openpage drag-to "#item" "#dropzone"` | `Element::drag_to` |
| `drag-to-point <locator> --x <x> --y <y>` | `openpage drag-to-point "#item" --x 320 --y 80` | `Element::drag_to_point` |
| `drag-in <target> --text/--files` | `openpage drag-in "#drop" --text "hello"` | `Page::actions().drag_in` |
| `select <locator> --text/--value/--index` | `openpage select "#s" --text "B"` | `Element::select_by_text/value/index` |
| `option-texts <locator>` | `openpage option-texts "#s"` | `Element::option_texts` |
| `selected-option <locator>` | `openpage selected-option "#s"` | `Element::selected_option` |
| `selected-options <locator>` | `openpage selected-options "#many"` | `Element::selected_options` |
| `select-all-options <locator>` | `openpage select-all-options "#many"` | `Element::select_all` |
| `clear-selected-options <locator>` | `openpage clear-selected-options "#many"` | `Element::clear_selected` |
| `invert-selected-options <locator>` | `openpage invert-selected-options "#many"` | `Element::invert_selected` |
| `select-text <locator> [--start --end]` | `openpage select-text "#article" --start 6 --end 10` | `Element::run_js` + DOM Range / `window.getSelection()` |
| `select-range <locator> <start> <end>` | `openpage select-range "#kw" 1 4` | `Element::run_js` + `selectionStart/selectionEnd` |
| `upload <locator> <files>...` | `openpage upload "#f" a.pdf b.pdf` | `Element::set_file_input_files` |
| `scroll-into-view <locator>` | `openpage scroll-into-view "#btn" --center` | `Element::scroll_to_see/center` |
| `scroll-element <locator> <direction>` | `openpage scroll-element "#pane" down --pixels 180` | `WebElement::scroll_down/up/left/right/to_*` |
| `scroll-element-position <locator>` | `openpage scroll-element-position "#pane"` | `WebElement::rect_scroll_position` |
| `is-visible <locator>` | `openpage is-visible "#kw"` | `Element::is_displayed` |
| `is-enabled <locator>` | `openpage is-enabled "#su"` | `Element::is_enabled` |
| `is-checked <locator>` | `openpage is-checked "#agree"` | `Element::is_checked` |
| `is-selected <locator>` | `openpage is-selected "option[selected]"` | `Element::is_selected` |
| `is-alive <locator>` | `openpage is-alive "#panel"` | `Element::is_alive` |
| `is-in-viewport <locator>` | `openpage is-in-viewport "#hero"` | `Element::is_in_viewport` |
| `is-whole-in-viewport <locator>` | `openpage is-whole-in-viewport "#hero"` | `Element::is_whole_in_viewport` |
| `is-covered <locator>` | `openpage is-covered "#btn"` | `Element::is_covered` |
| `is-clickable <locator>` | `openpage is-clickable "#btn"` | `Element::is_clickable` |
| `has-rect <locator>` | `openpage has-rect "#hero"` | `Element::has_rect` |
| `find <locator>` | `openpage find "#kw"` | `Page::find` |
| `value <locator>` | `openpage value "#kw"` | `Element::value` |
| `raw-text <locator>` | `openpage raw-text "#article"` | `Element::raw_text` |
| `link <locator>` | `openpage link "a.more"` | `Element::link` |
| `open-link <locator>` | `openpage open-link "a.more" --background` | `Element::link` + `tab.new` |
| `child-count <locator>` | `openpage child-count "#root"` | `Element::child_count` |
| `css-path <locator>` | `openpage css-path "#hero"` | `Element::css_path` |
| `xpath <locator>` | `openpage xpath "#hero"` | `Element::xpath` |
| `find-in-page <text>` | `openpage find-in-page "Beta"` | `window.find()` + JS text scan |
| `find-all <locator>` | `openpage find-all ".item"` | `Page::find_all` |
| `count <locator>` | `openpage count ".item"` | `Page::find_all().len()` |
| `selected-text` | `openpage selected-text` | `window.getSelection()` / active input selection |
| `active-element` | `openpage active-element` | `Page::active_element` |
| `user-agent` | `openpage user-agent` | `Page::user_agent` |
| `status-code` | `openpage status-code` | `Page::status_code` |
| `ready-state` | `openpage ready-state` | `Page::ready_state` |
| `is-loading` | `openpage is-loading` | `Page::is_loading` |
| `is-headless` | `openpage is-headless` | `Page::is_headless` |
| `wait-visible <locator>` | `openpage wait-visible "#late"` | `Element::wait_until_displayed` |
| `wait-hidden <locator>` | `openpage wait-hidden "#late"` | `Element::wait_until_hidden` |
| `wait-enabled <locator>` | `openpage wait-enabled "#late"` | `Element::wait_until_enabled` |
| `wait-disabled <locator>` | `openpage wait-disabled "#late"` | `Element::wait_until_disabled` |
| `wait-deleted <locator>` | `openpage wait-deleted "#late"` | `Element::wait_until_deleted` |
| `wait-clickable <locator>` | `openpage wait-clickable "#late"` | `Element::wait_until_clickable` |
| `wait-has-rect <locator>` | `openpage wait-has-rect "#late"` | `Element::wait_until_has_rect` |
| `wait-covered <locator>` | `openpage wait-covered "#late"` | `Element::wait_until_covered` |
| `wait-not-covered <locator>` | `openpage wait-not-covered "#late"` | `Element::wait_until_not_covered` |
| `wait-stop-moving <locator>` | `openpage wait-stop-moving "#late"` | `Element::wait_until_stop_moving` |
| `storage get/set` | `openpage storage get token --scope local` | `Page::local_storage / set_local_storage / session_storage / set_session_storage` |
| `key-down <key>` | `openpage key-down Shift` | `Page::actions().key_down` |
| `key-up <key>` | `openpage key-up Shift` | `Page::actions().key_up` |
| `shortcut <keys...>` | `openpage shortcut Meta a` | `Page::actions().type_keys` |
| `select-all` | `openpage select-all` | `Page::actions().type_keys(Keys::CTRL_A)` |
| `copy` | `openpage copy` | `Page::actions().type_keys(Keys::CTRL_C)` |
| `cut` | `openpage cut` | `Page::actions().type_keys(Keys::CTRL_X)` |
| `paste` | `openpage paste` | `Page::actions().type_keys(Keys::CTRL_V)` |
| `undo` | `openpage undo` | `Page::actions().type_keys(Keys::CTRL_Z)` |
| `redo` | `openpage redo` | `Page::actions().type_keys(Keys::CTRL_Y)` |
| `input <text>` | `openpage input "hello"` | `Page::actions().input` |
| `type <text>` | `openpage type "abc"` | `Page::actions().type` |
| `type-with-interval <text> --interval <secs>` | `openpage type-with-interval "abc" --interval 0.12` | `Page::actions().type_with_interval` |
| `reload --ignore-cache` | `openpage reload --ignore-cache` | `Page::refresh(true)` |
| `stop-loading` | `openpage stop-loading` | `Page::stop_loading` |
| `click-at <locator> [--x --y --button --count]` | `openpage click-at "#canvas" --x 24 --y 12` | `Element::click_at` |
| `wait-for-url <text>` | `openpage wait-for-url "baidu"` | `Page::wait_for_url_change` |
| `wait-for-title <text>` | `openpage wait-for-title "百度"` | `Page::wait_for_title_change` |
| `wait-for-new-tab` | `openpage wait-for-new-tab --timeout 5000` | `Page::wait_for_new_tab` |
| `wait-for-download-begin` | `openpage wait-for-download-begin --timeout 5000` | `Page::wait_for_download_begin` |
| `wait-for-downloads-done` | `openpage wait-for-downloads-done --timeout 5000` | `Page::wait_for_downloads_done` |
| `wait-for-alert-closed` | `openpage wait-for-alert-closed --timeout 5000` | `Page::wait_for_alert_closed` |
| `alert has` | `openpage alert has` | `Page::has_alert` |
| `alert accept --prompt-text` | `openpage alert accept --prompt-text "Alice"` | `Page::handle_alert(..., prompt_text, ...)` |
| `wait-for-load-start` | `openpage wait-for-load-start --timeout 5000` | `Page::wait_for_load_start` |
| `save <output>` | `openpage save page.mhtml` | `Page::save(..., as_pdf=false)` |
| `pdf <output>` | `openpage pdf out.pdf` | `Page::save_pdf` |
| `cookies get/set/clear` | `openpage cookies set name value --url ...` | `Page::cookies/set_cookie/clear_cookies` |
| `click-to-download <locator>` | `openpage click-to-download "#export"` | `Element::clicker().to_download(...)` |
| `downloads list/last/cancel/open/reveal/path/set-path/mode/set-mode/wait` | `openpage downloads open`, `openpage downloads reveal`, `openpage downloads set-path /tmp` | `Browser::download_missions/last_download` + `Page::download_*` + CLI OS shell open/reveal |
| `click-to-upload <locator> <files...>` | `openpage click-to-upload "#picker" foo.txt` | `Element::clicker().to_upload(...)` |
| `click-for-new-tab <locator>` | `openpage click-for-new-tab "a[target=_blank]"` | `Element::clicker().for_new_tab(...)` |
| `history list/go/clear` | `openpage history list`, `openpage history go 2`, `openpage history clear` | CDP `GetNavigationHistory` / `NavigateToHistoryEntry` / `ResetNavigationHistory` |
| `tab new/duplicate/reopen/close/list/switch` | `openpage tab close`, `openpage tab reopen`, `openpage tab duplicate --index 1 --background`, `openpage tab switch 1` | `Browser::new_tab/close_tabs/pages/activate_tab` + CLI-level URL duplication / recently-closed stack |
| `frame list/switch` | `openpage frame list`, `openpage frame switch 1`, `openpage frame switch main` | `Page::get_frames/get_frame_context` + session-level frame context |
| `browser activate/list` | `openpage browser activate`, `openpage browser list` | `webpage.activate` + daemon inventory |
| `browser start --incognito` / `browser is-incognito` | `openpage browser start --incognito`, `openpage browser is-incognito` | `LaunchOptions.incognito` / `Page::is_incognito` |
| `screenshot-element <locator> <output>` | `openpage screenshot-element "#card" /tmp/card.png` | `Element::save_screenshot` |
| `window state/location/size/move` | `openpage window state`, `openpage window move 80 60` | `Page::window_*` |
| `window max/min/fullscreen/normal/hide/show` | `openpage window normal`, `openpage window hide` | `Page::window_*` |
| `window list/switch/close` | `openpage window list`, `openpage window switch 2`, `openpage window close --index 2` | 浏览器窗口聚合 + `Page::window_id` + tab-level activate/close |
| `zoom in/out/get/set/reset` | `openpage zoom in --step 0.2`, `openpage zoom out`, `openpage zoom reset` | `Page::zoom_factor / set_zoom_factor / reset_zoom_factor` |
| `clear-cache` | `openpage clear-cache --cache --cookies --local-storage` | `Page::clear_cache` |
| `clipboard read/write` | `openpage clipboard write "hello"`, `openpage clipboard read` | `Page::clipboard_write_text/read_text` |
| `permissions set/reset` | `openpage permissions set clipboard-read granted` | `Browser.setPermission/resetPermissions` + `Page::set_permission/reset_permissions` |
| `batch` | `openpage batch --bail 'goto https://example.com' 'title'` | CLI 顺序执行并汇总结果 |

---

## 四、已补齐（底层有能力或简单轮询实现）

| 类别 | 对方命令 | OpenPage CLI 命令 | 状态 |
|------|---------|-------------------|------|
| **等待** | `wait --fn` (waitforfunction) | `wait-for-function <script>` | ✅ 轮询 `run_js` 直到返回 true |
| **等待** | `wait --text` (waitfortext) | `wait-for-text <locator> <text>` | ✅ 轮询检查元素文本 |
| **等待** | `wait --new-tab` | `wait-for-new-tab` | ✅ |
| **等待** | `wait --download-begin` | `wait-for-download-begin` | ✅ |
| **等待** | `wait --downloads-done` | `wait-for-downloads-done` | ✅ |
| **等待** | `wait --alert-closed` | `wait-for-alert-closed` | ✅ |
| **等待** | `wait --load-start` | `wait-for-load-start` | ✅ |
| **Cookie** | `cookies delete <name>` | `cookies delete <name> [--url]` | ✅ 底层 `Page::remove_cookie` |
| **鼠标** | `click-at` | `click-at <locator> [--x --y --button --count]` | ✅ |

## 五、确认无法补齐（底层能力不足）

| 类别 | 对方命令 | OpenPage 现状 | 原因 |
|------|---------|--------------|------|
| **网络** | `network route`, `network mock` | ⚠️ 只有基础启停 | `Interceptor` 只有 `start/stop/pause/resume/continue/fail/fulfill`，无自动路由/模拟规则配置能力。需底层开发 |

## 六、此前列出的“底层已有但尚未暴露”缺口已收口

经 `cargo run -q -p openpage_rs -- --help`、`browser --help`、`downloads --help`、`clear-cache --help` 复核，之前列出的这些项现在都已经是 CLI 正式能力：

- `save`
- `browser start --incognito`
- `browser is-incognito`
- `downloads cancel`
- `downloads path / set-path`
- `downloads mode / set-mode`
- `clear-cache`
- `batch`

## 七、底层有能力，但当前不宜直接暴露为分步 CLI

| 类别 | 候选命令 | 当前问题 |
|------|---------|---------|
| **鼠标分步动作** | `move-to`, `move-by`, `mouse-down`, `mouse-up`, `right-down`, `right-up`, `middle-down`, `middle-up` | `Page::actions()` 在当前 CLI / daemon RPC 中是按次新建，不具备跨命令持久状态。直接暴露会让“先按下再移动再松开”这类真人语义失真 |

## 八、完全缺失（需底层先开发）

| 类别 | 对方命令 | 缺失原因 |
|------|---------|---------|
| **录制** | `record start`, `record stop` | 底层无录制能力 |
| **认证** | `auth save`, `auth login` | 底层无认证管理 |
| **状态** | `state save`, `state load` | 底层无 page state 序列化 |

---

## 九、结论

- **本轮继续补齐了一批更像真人浏览器操作的 CLI 命令**：focus、clear、submit、check、uncheck、right-click、middle-click、double-click、active-element、selected-text、storage get/set、drag、drag-to、drag-to-point、drag-in、key-down、key-up、shortcut、select-all、copy、cut、paste、undo、redo、input、type、type-with-interval、stop-loading、click-at、find-in-page、select-text、select-range、click-to-download、`downloads list/last/cancel/open/reveal/path/set-path/mode/set-mode/wait`、click-to-upload、click-for-new-tab、history list/go/clear、`alert has`、`alert accept --prompt-text`、`browser activate/list`、`browser start --incognito` / `browser is-incognito`、`clear-cache`、`save`、`screenshot-element`，以及 `window list/switch/close` 在内的更完整 tab/window 操作。
- **本轮又补上了两类高频真人能力**：`clipboard read/write`，以及面向常见站点权限的 `permissions set/reset`（clipboard、geolocation、notifications、camera、microphone）。
- **这轮继续补了一个高频浏览器壳层动作**：`tab duplicate`，支持复制当前 tab 或按 `--index/--target` 复制指定 tab，并可配合 `--background/--window`。
- **这轮继续补了一个高频浏览器壳层动作**：`tab reopen`，可恢复当前 session 中最近一次由 OpenPage CLI 关闭的 tab（按 URL 重新打开，不依赖不稳定的浏览器快捷键 restore）。
- **这轮又把下载壳层动作补完整了一步**：`downloads open` / `downloads reveal`，支持直接打开已下载文件，或在系统文件管理器里定位它。
- **这轮顺手把 `tab close` 的真人默认语义也补对了**：不再强制要求 `--target/--index`，现在 `openpage tab close` 默认关闭当前活动 tab。
- **这轮还把缩放补到更贴近日常浏览器习惯**：除了 `zoom get/set/reset`，现在也支持 `zoom in/out` 相对缩放。
- **这轮又补了两个高频读取能力**：`value`（读取输入框 / select 当前值）和 `raw-text`（读取元素原始文本内容）。
- **这轮还补了一个典型真人壳层动作**：先读出元素实际链接目标（`link`），再像浏览器右键菜单一样“在新标签页打开链接”（`open-link`）。
- **这轮又补了三个结构定位读取能力**：`child-count`、`css-path`、`xpath`，方便把“这个元素在 DOM 里的位置”暴露给 CLI。
- **这轮又补了一个更接近真人鼠标使用的动作**：`hover-at`，可以在元素内部按偏移位置悬停，适合菜单热区、画布、地图、图表这类不是“悬停元素中心”就够用的场景。
- **这轮还补了一个高频容器交互能力**：`scroll-element` / `scroll-element-position`，可以直接操作聊天面板、侧栏、下拉列表、虚拟表格这类内部滚动区域，不再只能滚整个页面。
- **这轮还把两个已有 RPC 状态查询正式暴露到了 CLI**：`ready-state` 和 `is-loading`。
- **这轮继续补了一组元素状态/等待能力**：`has-rect`、`wait-has-rect`、`wait-covered`、`wait-not-covered`、`wait-stop-moving`。
- **这轮又补了一组现成页面信息 getter**：`user-agent`、`status-code`、`is-headless`。
- **这轮还补了一个高频真人刷新动作**：`reload --ignore-cache`，对应浏览器里的 hard reload / 忽略缓存刷新。
- **并顺手修复了底层 `Element::submit()` 的返回值问题**，避免 CLI 暴露一个不稳定能力。
- **network route/mock** 因底层 `Interceptor` 无规则配置能力，无法补齐。
- **batch** 已补齐，当前源码中已提供 `openpage batch`，可在一次调用里顺序执行多条 CLI 命令。
- **从“像真人日常用浏览器”视角看，网页内交互已覆盖得很深；剩余提升空间主要转向浏览器壳层能力**，例如标签页固定/重排，以及书签/阅读列表。`zoom`、`clipboard`、常见站点权限控制、窗口 list/switch/close、tab duplicate、tab reopen 现已通过运行时测试或 CLI 冒烟验证。
- **鼠标 `move/down/up` 这一类分步动作虽然底层有接口，但当前 CLI / RPC 调用模型不保留 pointer 状态，不应直接暴露成 top-level 命令。**
- **第 8 节（recording/auth/state）** 仍需底层开发，暂不纳入 CLI 范围。

## 十、按“正常人日常用浏览器”视角的剩余缺口

> 这里刻意只看“像人在用浏览器”的交互，不把网络 mock、批处理编排这类自动化工程能力混进来。结论是：**网页内操作大多已经够用，主要缺口转向浏览器外壳层。**

| 优先级 | 缺口 | 当前证据 | 判断 |
|------|------|---------|------|
| P2 | 浏览器原生 restore 最近关闭标签页 | 现有 `openpage tab reopen` 可恢复**当前 session 中由 OpenPage CLI 关闭**的最近 tab URL；仍不是浏览器原生会话级 restore | **部分覆盖** |
| P1 | 固定 / 重排标签页 | `tab --help` 只有 `new / duplicate / reopen / close / list / switch`；源码和 `chromiumoxide_cdp` 中也没有 pin / reorder 对应命令证据 | **浏览器壳层缺口** |
| P2 | 运行时静音 / 取消静音 | `browser start --help` 只有启动期 `--mute`；源码搜索只看到 `browser.rs` 在启动参数里拼接 `--mute-audio`，没有运行时 toggle 命令 | **浏览器壳层缺口** |
| P2 | 书签 / 阅读列表 | CLI help 无相关命令；源码搜索与 `chromiumoxide_cdp` 协议类型里也没有 bookmark / reading-list 操作证据 | **产品层能力缺口** |
| P3 | 保存密码 / 自动填充 | 仓库里只看到 `credentials_enable_service` 启动偏好项，没有 CLI 命令，也没有运行时密码库 / 自动填充操作 API | **产品层能力缺口** |
| P3 | 清空下载记录 | `downloads --help` 只有 `list / last / cancel / open / reveal / path / set-path / mode / set-mode / wait`；`clear-cache --help` 也只覆盖 cache/storage/cookies，不覆盖下载记录 | **浏览器壳层缺口** |
| P3 | 浏览器原生式自由鼠标手势 | 已有 `drag/drag-to/drag-to-point`、`click-at`、`select-text`，但没有跨多条命令保持 pointer 状态的 `mouse-down -> move -> up` | **复杂拖拽/画布/任意自由选区仍不完整** |

补充证据：

- `tab --help` 当前只暴露：`new / duplicate / reopen / close / list / switch`。
- `browser start --help` 当前只暴露启动期 `--mute`，没有 `mute` / `unmute` 子命令。
- `history --help` 当前暴露 `list / go / clear`；`downloads --help` 当前只暴露 `list / last / cancel / open / reveal / path / set-path / mode / set-mode / wait`。
- `clear-cache --help` 当前只覆盖 `--session-storage / --local-storage / --cache / --cookies`，不覆盖浏览历史或下载记录。
- `chromiumoxide_cdp` 当前可见的 browser-level 能力主要是 `Browser.setPermission`、`Browser.setDownloadBehavior`、`Browser.setWindowBounds` 这类 CDP 命令；未检出 pin tab、reorder tab、bookmark、reading list、runtime audio mute 对应命令。
- `page.rs` 底层确实有 action builder 的 `move_to` / `move_by`，但当前 CLI / daemon 调用模型每次都是新建 `page.actions()`，因此不适合直接暴露成跨命令持久的鼠标按下/移动/松开语义。
