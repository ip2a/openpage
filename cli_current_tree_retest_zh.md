# OpenPage CLI 当前树复测纪要

日期：2026-06-06

目的：

- 不依赖旧二进制
- 直接基于当前工作树重新安装 CLI
- 再次确认哪些优化项目在真实本地使用里最值得优先做

## 本次使用的安装入口

当前仓库里的 CLI 入口仍然是 Rust crate：

- 构建检查：`cargo check --manifest-path rust/Cargo.toml`
- 本地安装：`cargo install --path rust --root /tmp/openpage-cli-current --force`
- 本次复测二进制：`/tmp/openpage-cli-current/bin/openpage`

辅助前提：

- 这台机器需要显式提供浏览器路径：
  - `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`

## 预检查结果

1. `cargo check --manifest-path rust/Cargo.toml`
   - 通过
2. `OPENPAGE_BROWSER_PATH=... /tmp/openpage-cli-current/bin/openpage doctor --quick`
   - 通过
   - 当前机上已有一个历史 session `recur`
   - 这不影响本次 fresh `OPENPAGE_HOME` 复测

## 复测环境

- fresh `OPENPAGE_HOME=/tmp/openpage-current-retest.NfQ2Wy`
- 本地延迟 HTTP 服务：
  - 监听 `127.0.0.1:54308`
  - 每次响应延迟 `45s`
- busy 场景 session：
  - `current-busy`

## 场景 1：busy 窗口内的控制面一致性

执行顺序：

1. `browser start --session current-busy --headless about:blank`
2. 并发启动：
   - `goto --session current-busy http://127.0.0.1:54308`
3. busy 窗口内查询：
   - `browser list`
   - `browser status --session current-busy`
   - `browser logs --session current-busy --tail 20`
   - `title --session current-busy`
   - `snapshot --session current-busy`

观察：

- `browser list` 很快返回：
  - session 被归入 `incomplete[]`
  - `reasons=["daemon_unresponsive"]`
- `browser status` 和 `browser logs` 没有立刻返回
  - 需要额外等待后才返回 `state="incomplete"`
  - `browser logs` 仍是空内容
- `title` 和 `snapshot` 在 busy 窗口里持续阻塞

结论：

- busy 场景下的状态故事仍未统一
- inventory/list 已经知道 session 忙了
- 但普通命令和部分控制面命令仍然表现出明显饥饿或延迟
- 这继续支撑优化项目 1

## 场景 2：busy + `--replace`

在上面的 busy 窗口内执行：

- `browser start --session current-busy --replace --headless https://example.com`

观察：

- `snapshot`
  - 返回底层错误：
    - `kind="daemon_transient"`
    - `io error: Connection reset by peer (os error 54)`
- `title`
  - 同样回落到 `daemon_transient`
- `browser start --replace`
  - 失败为 `browser_launch`
  - stderr 明确是 Chrome profile lock：
    - `SingletonLock: File exists`
    - `ProcessSingleton ... Aborting now to avoid profile corruption`

结论：

- 当前树上，busy/displaced 请求仍可能漏出底层传输错误
- 这说明“统一结构化状态故事”还没有真正收口
- 同时，`--replace` 作为恢复动作在最关键的 busy 场景下依然不可靠

## 场景 3：恢复真相与控制面真相脱节

在 `browser start --replace` 失败后继续检查：

- `browser status --session current-busy`
- `browser list`
- `browser logs --session current-busy --tail 20`
- `pgrep -af "/tmp/openpage-current-retest.NfQ2Wy/profiles/current-busy"`

观察：

- `browser status`
  - 已经是 `state="inactive"`
- `browser list`
  - 已经空了
- `browser logs`
  - 也只剩 inactive 视角
- 但 `pgrep` 仍能看到整组 Chrome 进程还活着
  - 根进程和多个 helper / renderer 都仍在

结论：

- 当前控制面已经“认为这个 session 不存在了”
- 但真实恢复条件还没满足，因为 profile 仍被旧 Chrome 占用
- 这正是优化项目 2 的核心：
  - forced-stop / broken recovery path 没有把 browser child 清干净
  - 同时 observability 过早消失

## 场景 4：`batch --bail` 可读性

执行：

- `batch --bail "browser start --session batch-check --headless about:blank" "history go 0 --session batch-check" "browser stop --session batch-check"`

观察：

- stdout 只输出两行原生命令 JSON：
  - 第一行：start 成功
  - 第二行：`history go 0` 失败
- 没有：
  - 命令序号
  - 原始 argv 回显
  - “这一行触发了 bail 停止”的显式标记

结论：

- `batch` 语义本身可用
- 但混合成功/失败时的可读性仍然差
- 这继续支撑优化项目 3

## 场景 5：命令发现性

帮助输出抽查：

- `openpage help frame switch`
- `openpage help storage get`
- `openpage help history go`
- `openpage help click`

观察：

- `frame switch`
  - 只显示 `<TARGET>`
  - 不告诉用户 `main` / `root` / `page` 这些可用 reset target
- `storage get`
  - 只显示 `--scope <SCOPE> [KEY]`
  - 第一次使用时不够直观
- `history go`
  - 只说明 `<INDEX>`
  - 没把“跳转后通常要 wait-for-navigation”这类后续动作暴露出来
- `click`
  - 只说明 `<LOCATOR>`
  - 没提示什么时候该跟 `wait-for-navigation`

结论：

- 这不是 correctness 问题
- 但第一次用时确实不够顺手
- 这继续支撑优化项目 4（低优先级 polish）

## 当前树上的最终排序

1. Busy-session 控制面 / 中断语义
2. Forced-stop 清理完整性 / recovery truthfulness
3. `batch` 可读性
4. 命令发现性 / follow-up guidance polish

## 为什么这次复测很重要

这次不是沿用旧安装结论，而是重新基于当前工作树安装并复测后，仍然得到同一排序。

而且当前树上的证据比之前更具体：

- busy + replace 时，普通命令仍会漏出 `daemon_transient`
- 控制面已经 `inactive`，但 profile 对应 Chrome 仍存活
- `batch --bail` 输出形态依旧不利于人读
- help 仍没有把几个常见 follow-up 讲清楚

## 建议起手顺序

1. 先做 `rust/src/cli/connection.rs`
   - busy / displaced 请求统一成一个结构化状态故事
2. 再做 forced cleanup
   - 把 browser child 清理纳入真实恢复路径
3. 再修 recovery guidance
   - 让 fix text 和真实恢复动作一致
4. 再做 `batch` 输出包装
5. 最后补 help / follow-up guidance
