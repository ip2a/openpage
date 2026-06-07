# browser daemon 契约盘点 v1

范围：

- `browser status --session ...`
- `browser logs --session ...`
- `browser list`

目标：把 daemon 控制面的浏览器相关 JSON 输出收成单独契约，方便脚本和 agent 直接消费。

## 1. 共同主题

这三个命令当前都围绕 daemon session 运行态展开，已对齐的机器字段核心是：

- `kind="daemon_session"`
- `state`
- `reasons`
- `fix`
- `log_path`
- `log_exists`

## 2. browser list

返回结构：

```json
{
  "summary": { "...": "..." },
  "sessions": [ ... ],
  "incomplete": [ ... ],
  "cleaned": [ ... ]
}
```

### 2.1 summary

稳定字段：

- `healthy`
- `incompatible`
- `incomplete`
- `cleaned`
- `total`

### 2.2 sessions[]

稳定字段：

- `kind="daemon_session"`
- `session`
- `port`
- `pid`
- `version`
- `version_matches_current_cli`
- `alive`
- `ready`
- `log_path`
- `log_exists`
- `state`
- `reasons`
- `fix?`

当前 `state`：

- `healthy`
- `incompatible`

### 2.3 incomplete[]

稳定字段：

- `kind="daemon_session"`
- `session`
- `pid_present`
- `port_present`
- `version_present`
- `pid_valid`
- `port_valid`
- `alive`
- `ready`
- `log_path`
- `log_exists`
- `state="incomplete"`
- `reasons`
- `fix`

### 2.4 cleaned[]

稳定字段：

- `kind="daemon_session"`
- `session`
- `reason`（人类摘要）
- `reasons`（机器原因码）
- `log_path`
- `log_exists`
- `state="cleaned"`
- `fix`

## 3. browser status

返回当前 session 的 daemon 视图。

稳定字段：

- `kind="daemon_session"`
- `session`
- `port?`
- `pid?`
- `version?`
- `alive`
- `ready`
- `log_path`
- `log_exists`
- `state`
- `fix?`

按状态额外字段：

### 3.1 healthy / incompatible

- `version_matches_current_cli`
- `reasons?`

### 3.2 incomplete

顶层额外稳定字段：

- `reasons`

并带嵌套：

- `incomplete.kind="daemon_session"`
- `incomplete.session`
- `incomplete.pid_present`
- `incomplete.port_present`
- `incomplete.version_present`
- `incomplete.pid_valid`
- `incomplete.port_valid`
- `incomplete.alive`
- `incomplete.ready`
- `incomplete.log_path`
- `incomplete.log_exists`

### 3.3 inactive

稳定字段：

- `kind="daemon_session"`
- `state="inactive"`
- `fix?`

## 4. browser logs

返回 daemon 日志视图，建立在 `browser status` payload 之上。

稳定字段：

- `kind="daemon_session"`
- `state`
- `reasons?`
- `fix?`
- `log_path`
- `log_exists`
- `path`（兼容别名，等于 `log_path`）
- `exists`（兼容别名，等于 `log_exists`）
- `tail`
- `content`

关键语义：

- `browser logs` 保留 `browser status` 的 daemon 机器字段
- 如果上游传入旧 shape 且没有 `kind`，当前实现会回填 `kind="daemon_session"`
- `exists` 仍保留，但它只是 `log_exists` 的兼容别名

## 5. 当前稳定 state

当前浏览器 daemon 相关稳定状态值：

- `healthy`
- `incompatible`
- `incomplete`
- `inactive`
- `cleaned`

注意：

- `cleaned` 只出现在 `browser list` 的 `cleaned[]`
- `inactive` 只出现在 `browser status` / `browser logs` 顶层 session 视图

## 6. 当前稳定 reasons

当前稳定原因码来自 daemon 控制面，共享于这些 browser surface：

- `version_mismatch`
- `missing_pid`
- `invalid_pid`
- `missing_port`
- `invalid_port`
- `missing_version`
- `daemon_not_ready`
- `not_alive`

## 7. 当前稳定 kind

当前这三个 browser daemon 控制面已统一：

- `kind="daemon_session"`

## 8. 未承诺范围

以下内容不建议作为强契约依赖：

### 8.1 人类摘要文案

不要依赖：

- `reason` 的精确句式
- `fix` 的精确文案
- `content` 的具体文本内容

### 8.2 运行态数量

不要把以下值当跨机器稳定值：

- `summary.*`
- 某个 session 是否存在
- `alive/ready/pid/port` 的实际数值

### 8.3 兼容别名的长期存在形式

当前仍保留：

- `path`
- `exists`

但新代码应优先消费：

- `log_path`
- `log_exists`

## 9. 推荐消费顺序

1. 先看 `kind`
2. 再看 `state`
3. 再看 `reasons`
4. 需要动作时看 `fix`
5. 需要日志定位时看 `log_path` / `log_exists`
6. 只有人类展示时再退回 `reason` / `content`
