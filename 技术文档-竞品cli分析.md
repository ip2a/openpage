# 竞品 CLI 设计分析：agent-browser

> 分析对象：`参考项目/agent-browser-main/cli/`（Vercel Labs 的 browser automation CLI）
> 分析维度：架构、配置、命令解析、Daemon 生命周期、输出处理、特殊功能

---

## 1. 整体架构：Client-Daemon 模式

CLI 不是 monolithic 设计，而是 **thin-client + persistent daemon** 架构：

| 层级 | 职责 | 通信方式 |
|---|---|---|
| **CLI (Rust binary)** | 解析命令行 → 序列化为 JSON → 发送给 daemon | Unix Domain Socket / TCP |
| **Daemon (Rust async)** | 实际操控浏览器 (CDP/WebSocket) | Chrome DevTools Protocol |

### 设计收益

- CLI 启动极快（无浏览器启动开销）
- Session 可持久化（daemon 后台存活）
- 多 CLI 实例可复用同一 session
- 批量命令 (`batch`) 只需一次 daemon 连接

---

## 2. 入口与模式判断 (`main.rs`)

`main()` 采用 **多级模式分支** 设计：

```
AGENT_BROWSER_DAEMON=1     → run_daemon()          [后台守护进程]
AGENT_BROWSER_DASHBOARD=1  → run_dashboard_server() [WebSocket 流媒体服务]
else                       → 正常 CLI 模式
```

### CLI 模式下的核心流程

1. `--help` / `--version` 短路返回
2. 特殊命令短路（`install`, `upgrade`, `doctor`, `dashboard`, `profiles`, `skills`, `session`）——这些不需要 daemon
3. `parse_flags()` → 加载三层配置
4. `parse_command()` → 将 CLI 字符串转为 JSON action
5. `ensure_daemon()` → 按需启动/连接 daemon
6. `send_command()` → IPC 通信
7. `print_response_with_opts()` → 格式化输出

---

## 3. 配置系统：三层优先级 (`flags.rs`)

配置加载采用 **显式覆盖** 策略（后覆盖前）：

```
1. 用户级: ~/.agent-browser/config.json
2. 项目级: ./agent-browser.json   ← 优先级更高
3. 环境变量: AGENT_BROWSER_*       ← 优先级更高
4. CLI flags: --headed, --proxy   ← 最高优先级
```

### 关键设计

- `Config::merge()` 实现 Option 级联（`other.or(self)`）
- 布尔值环境变量支持 `0/false/no` 显式禁用
- 空闲超时支持人类友好格式：`10s`, `3m`, `1h`
- 追踪 CLI 显式传入的 flag（`cli_headed`, `cli_proxy` 等），用于在 daemon 已运行时给出 "flag ignored" 警告

---

## 4. 命令解析：字符串 → JSON Action (`commands.rs`)

这是 CLI 最核心的设计。**所有 CLI 命令最终都被解析为一个 JSON 对象**，形如：

```json
{ "id": "r123456", "action": "click", "selector": "@e1" }
```

### 命令分类体系

| 类别 | 命令示例 | 映射的 action |
|---|---|---|
| **导航** | `open`, `goto`, `back`, `reload` | `navigate`, `back`, `forward` |
| **核心交互** | `click`, `fill`, `type`, `hover`, `select`, `upload` | 同名 action |
| **键盘** | `press`, `keydown`, `keyup`, `keyboard type` | `press`, `keydown`, `keyboard` |
| **滚动** | `scroll`, `scrollintoview` | `scroll`, `scrollintoview` |
| **等待** | `wait`, `wait --url`, `wait --fn`, `wait --text` | `wait`, `waitforurl`, `waitforfunction` |
| **快照** | `snapshot -i -c -d 3` | `snapshot` |
| **截图/PDF** | `screenshot`, `pdf` | `screenshot`, `pdf` |
| **查询** | `get title`, `get url`, `is visible` | `get_title`, `get_url` |
| **查找** | `find role button` | `find` |
| **网络** | `network route`, `network mock` | `network_route` |
| **存储** | `storage get`, `storage set` | `storage_get`, `storage_set` |
| **Cookie** | `cookies set`, `cookies clear` | `cookies_set`, `cookies_clear` |
| **标签页** | `tab new`, `tab close`, `tab list` | `tab_new`, `tab_close` |
| **窗口/框架** | `window new`, `frame main` | `window_new`, `mainframe` |
| **对话框** | `dialog accept`, `dialog dismiss` | `dialog` |
| **录制** | `record start`, `record stop` | `recording_start`, `recording_stop` |
| **认证** | `auth save`, `auth login` | `auth_save`, `auth_login` |
| **状态** | `state save`, `state load` | `state_save`, `state_load` |
| **流** | `stream enable`, `stream disable` | `stream_enable`, `stream_disable` |
| **剪贴板** | `clipboard read`, `clipboard write` | `clipboard` |
| **执行** | `eval`, `batch` | `evaluate`, `batch` |
| **关闭** | `close`, `quit`, `exit` | `close` |

### 错误处理设计

- 自定义 `ParseError` enum，包含 `UnknownCommand`, `MissingArguments`, `InvalidValue` 等
- 每个错误都附带 `usage` 提示
- JSON 模式下错误也结构化输出（带 `type` 字段）

---

## 5. Daemon 生命周期管理 (`connection.rs`)

### Session 模型

- 每个 session 对应一组 sidecar 文件：`{session}.pid`, `{session}.sock`, `{session}.version`
- Socket 目录优先级：`AGENT_BROWSER_SOCKET_DIR` > `XDG_RUNTIME_DIR` > `~/.agent-browser` > `/tmp`

### Daemon 启动流程 (`ensure_daemon`)

1. `walk_daemons()` 扫描 socket 目录，清理僵尸 session
2. 检查目标 session 的 `.pid` 和 `.sock` 是否存活
3. 若存活 → 复用，返回 `already_running: true`
4. 若未存活 → spawn 新进程，传入 `AGENT_BROWSER_DAEMON=1` + 所有配置 via 环境变量
5. 等待 socket 就绪（轮询连接，超时 30s）

### 跨平台 IPC

| 平台 | 机制 |
|---|---|
| Unix | Unix Domain Socket (`{session}.sock`) |
| Windows | TCP localhost (`{session}.port`) |

### 环境变量传递

Daemon 配置完全通过环境变量注入（约 30 个 `AGENT_BROWSER_*` 变量），避免命令行参数泄露敏感信息。

---

## 6. 输出处理：双模式 (`output.rs`)

### Human 模式

- 成功：`✓` 绿色标记 + 结构化文本
- 错误：`✗` 红色标记 + 错误信息
- 警告：`⚠` 黄色标记
- 内容边界：可选 `--- AGENT_BROWSER_PAGE_CONTENT nonce=... ---` 防注入标记

### JSON 模式 (`--json`)

```json
{ "success": true, "data": {...}, "error": null, "warning": null }
```

### 特殊格式化

- `dialog` → 带类型和提示文本的友好输出
- `storage` → `key: value` 列表
- `stream` → 状态摘要（端口、连接数、是否录屏）
- `cookies` → 表格或 JSON 数组

---

## 7. 特殊功能设计

### Batch 模式

- 从 stdin 读取 JSON 数组：`[["open", "https://a.com"], ["snapshot"]]`
- 支持 `--bail`：任一命令失败即终止
- 每个命令独立解析和发送

### Skills 系统 (`skills.rs`)

- 打包在 npm 包中的 `skill-data/` 和 `skills/` 目录
- 发现逻辑：从可执行文件位置向上遍历找项目根
- 支持 `skills list`, `skills get <name>`, `skills get --all`
- 用于给 AI Agent 提供环境特定的使用指南

### Install (`install.rs`)

- 从 Chrome for Testing 下载对应平台的 Chrome
- 缓存到 `~/.agent-browser/browsers/`
- 版本管理：按目录名排序取最新

### Doctor (`doctor/`)

- 自检子系统，检查环境配置
- 独立模块，不依赖 daemon

---

## 8. 设计亮点总结

| 设计点 | 说明 |
|---|---|
| **命令即 JSON** | CLI 只做解析和传输，逻辑全在 daemon，天然支持批处理 |
| **Session 隔离** | 多项目可同时运行，互不干扰 |
| **配置层级清晰** | 项目级 > 用户级 > 环境变量，开发体验好 |
| **跨平台统一** | 通过 `Connection` trait 屏蔽 Unix Socket / TCP 差异 |
| **僵尸清理** | `walk_daemons()` 自动清理崩溃残留，避免端口/文件泄漏 |
| **敏感信息保护** | 密码通过 `--password-stdin`，代理通过环境变量传 daemon |
| **AI 原生** | Snapshot + `@ref` 语义选择器，skill 系统，边界标记，全为 AI Agent 优化 |
| **渐进式连接** | 支持本地启动、CDP 连接、云 provider 三种浏览器接入模式 |

---

## 9. 对 OpenPage 的借鉴点

1. **Daemon 架构**：避免每次命令都启动浏览器，session 持久化
2. **命令 → JSON 映射**：统一的 action 协议，便于扩展和测试
3. **三层配置**：项目级 config 文件对团队开发很重要
4. **Session 管理**：显式 session 名称 + 自动清理，适合长期运行的 Agent
5. **错误信息结构化**：JSON 模式下带 `type` 字段，便于程序处理
6. **技能系统**：将使用指南打包进 CLI 分发，降低 AI 使用门槛
7. **内容边界标记**：CSPRNG 生成的 nonce 防止页面内容伪造分隔符
