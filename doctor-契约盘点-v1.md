# doctor 契约盘点 v1

范围：`rust/src/cli/doctor.rs` 当前对外 JSON 壳层契约。

目标：让脚本、agent、上层 CLI 包装不必再依赖 `id` 前缀或人类文案来理解 `doctor` 输出。

## 1. 顶层结构

`openpage doctor [--quick] [--fix]` 返回：

```json
{
  "ok": true,
  "result": {
    "summary": { "...": "..." },
    "checks": [ ... ],
    "fixed": [ ... ],
    "inventory": { "...": "..." }
  }
}
```

说明：

- `ok`：是否存在 `fail` 级检查
- `summary`：本次报告聚合视图
- `checks[]`：当前状态检查项
- `fixed[]`：仅在 `--fix` 时出现实际修复动作
- `inventory`：daemon 侧当前运行态镜像

## 2. summary

稳定字段：

| 字段 | 含义 |
|---|---|
| `pass` | pass 数量 |
| `warn` | warn 数量 |
| `fail` | fail 数量 |
| `info` | info 数量 |
| `fixable` | `doctor --quick --fix` 真正可自动处理的检查数 |
| `total` | 检查总数 |
| `warn_ids` | warn 检查 id 列表 |
| `fail_ids` | fail 检查 id 列表 |
| `info_ids` | info 检查 id 列表 |
| `fixable_ids` | 可自动修复的检查 id 列表 |

关键语义：

- `fixable_ids` 比 `checks[].fix` 更窄
- `checks[].fix` 表示“有下一步建议”
- `fixable_ids` 表示“`doctor --quick --fix` 能直接做”

## 3. checks[]

基础公共字段：

| 字段 | 含义 |
|---|---|
| `id` | 稳定检查 id |
| `category` | 人类阅读分类 |
| `status` | `pass / warn / fail / info` |
| `message` | 人类阅读摘要 |

可选机器字段：

| 字段 | 含义 |
|---|---|
| `kind` | 稳定检查类型 |
| `fix` | 下一步建议 |
| `auto_fixable` | 是否可被 `doctor --quick --fix` 自动处理 |
| `session` | 相关 session |
| `state` | daemon session 状态 |
| `reasons` | 稳定原因码列表 |
| `alive` / `ready` | daemon 运行态 |
| `pid` / `port` / `version` | daemon 元数据 |
| `version_matches_current_cli` | daemon 版本是否匹配当前 CLI |
| `log_path` / `log_exists` | daemon log 诊断 |
| `pid_present` / `port_present` / `version_present` | sidecar 是否存在 |
| `pid_valid` / `port_valid` | sidecar 是否可解析 |
| `browser_path` / `resolved_path` / `suggested_path` | browser 配置与可执行路径诊断 |

### 3.1 当前稳定 kind

| kind | 适用检查 |
|---|---|
| `openpage_home` | `env.openpage_home` |
| `daemon_dir` | `env.daemon_dir`, `daemon.dir` |
| `legacy_sessions` | `env.legacy_sessions` |
| `daemon_sessions` | `daemon.sessions` |
| `daemon_session` | `daemon.session.*`, `daemon.incomplete.*`, `daemon.cleaned.*` |
| `browser_config` | `browser.config` |
| `browser_executable` | `browser.executable`, `browser.executable.hint` |
| `browser_launch` | `browser.launch` |

当前状态：

- `rust/src/cli/doctor.rs` 生产路径里的 `Check::new(...)` 现在都已经要求带 `with_kind(...)`
- 这一点已有源码级回归测试约束，后续新增生产检查项若遗漏 `kind` 会直接测试失败

### 3.2 daemon session checks

`kind="daemon_session"` 的检查可以稳定依赖：

- `session`
- `state`
- `reasons`
- `log_path`
- `log_exists`

必要时还会带：

- `auto_fixable`
- `fix`
- `alive`
- `ready`
- `pid`
- `port`
- `version`

## 4. fixed[]

仅在 `--fix` 模式下出现。

稳定字段：

| 字段 | 含义 |
|---|---|
| `check_id` | 对应检查 id |
| `message` | 人类阅读修复摘要 |
| `auto_fixable` | 该动作是否来自显式自动修复路径 |
| `source` | 修复来源 |
| `reason` | 稳定修复原因码 |
| `session` | 可选，关联 session |
| `path` | 可选，关联文件路径 |

### 4.1 当前稳定 source

| source | 含义 |
|---|---|
| `direct_fix` | 来自 `doctor --quick --fix` 主动修复路径 |
| `inventory_scan` | inventory 扫描期间顺带做掉的清理 |

### 4.2 当前稳定 reason

| reason | 含义 |
|---|---|
| `legacy_session_json` | 清理 legacy session JSON |
| `incompatible_daemon` | 停止版本不匹配 daemon |
| `incomplete_unready_daemon` | 停止 incomplete 且 unready daemon |
| `stale_sidecars` | 清理 stale sidecars |

## 5. inventory

`inventory` 是 daemon 运行态镜像，不是 doctor 私有结构。

当前稳定主结构：

- `summary`
  - `healthy`
  - `incompatible`
  - `incomplete`
  - `cleaned`
  - `total`
- `sessions[]`
- `incomplete[]`
- `cleaned[]`

这些条目已稳定带出：

- `state`
- `reasons`
- `fix`
- `log_path`
- `log_exists`

## 6. --fix 视图语义

`doctor --quick --fix` 当前契约：

- `fixed[]` 描述“刚刚做了什么”
- `summary` / `checks[]` / `inventory` 描述“修完之后的当前状态”

也就是：

- `fixed[]` = 历史动作
- 其余主体 = post-fix view

## 7. 推荐消费顺序

给脚本或 agent 的推荐解析顺序：

1. 先看 `ok`
2. 再看 `summary.fail_ids` / `summary.fixable_ids`
3. 对单个检查优先用 `kind`
4. daemon 相关优先看 `state` / `reasons`
5. `--fix` 结果优先看 `fixed[].source` / `fixed[].reason`
6. 只有人类展示时再退回 `message` / `fix`
