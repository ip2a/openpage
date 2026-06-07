# doctor 契约收口结论 v1

日期：2026-06-05

用途：给后续实现和上层调用方一个阶段性结论，明确：

- 哪些 `doctor` JSON 字段已经可以当稳定壳层契约使用
- 哪些内容仍然不应被当作强承诺

## 1. 已稳定范围

以下内容当前已经被实现、文档、测试三方共同约束：

### 1.1 顶层结构

- `ok`
- `result.summary`
- `result.checks`
- `result.fixed`
- `result.inventory`

### 1.2 summary

可稳定依赖：

- `pass`
- `warn`
- `fail`
- `info`
- `fixable`
- `total`
- `warn_ids`
- `fail_ids`
- `info_ids`
- `fixable_ids`

### 1.3 checks[]

可稳定依赖的通用字段：

- `id`
- `category`
- `status`
- `message`
- `kind`（当该检查具备机器分类价值时，当前生产路径已全覆盖）

对特定检查类型，还可稳定依赖：

- `fix`
- `auto_fixable`
- `session`
- `state`
- `reasons`
- `alive`
- `ready`
- `pid`
- `port`
- `version`
- `version_matches_current_cli`
- `log_path`
- `log_exists`
- `pid_present`
- `port_present`
- `version_present`
- `pid_valid`
- `port_valid`
- `browser_path`
- `resolved_path`
- `suggested_path`

### 1.4 当前稳定 kind 集合

当前生产代码里的稳定 kind 基线为：

- `openpage_home`
- `daemon_dir`
- `legacy_sessions`
- `daemon_sessions`
- `daemon_session`
- `browser_config`
- `browser_executable`
- `browser_launch`

这组值当前已有源码级回归测试约束。

### 1.5 fixed[]

可稳定依赖：

- `check_id`
- `message`
- `auto_fixable`
- `source`
- `reason`
- `session?`
- `path?`

当前稳定 `source`：

- `direct_fix`
- `inventory_scan`

当前稳定 `reason`：

- `legacy_session_json`
- `incompatible_daemon`
- `incomplete_unready_daemon`
- `stale_sidecars`

### 1.6 inventory

可稳定依赖：

- `summary.healthy`
- `summary.incompatible`
- `summary.incomplete`
- `summary.cleaned`
- `summary.total`
- `sessions[]`
- `incomplete[]`
- `cleaned[]`

以及这些条目中的：

- `state`
- `reasons`
- `fix`
- `log_path`
- `log_exists`

### 1.7 --fix 视图语义

当前可稳定依赖：

- `fixed[]` 表示刚刚做过的修复动作
- `summary / checks / inventory` 表示修复后的当前状态

也就是：

- `fixed[]` = applied actions
- 其余主体 = post-fix view

## 2. 未承诺范围

以下内容目前**不建议**上层作为强契约依赖：

### 2.1 message / fix 的具体文案

可以展示给人，但不应该作为机器判断依据。

不承诺：

- 精确句式
- 标点
- 示例命令顺序
- 文案中是否出现额外上下文

### 2.2 category 的分类语义强度

`category` 仍主要偏向人类阅读分组。  
机器过滤请优先使用 `kind`。

### 2.3 本机相关数量

以下是运行态依赖，不构成跨机器稳定值：

- `summary.pass/warn/fail/info/total`
- `inventory.summary.*`
- 某些 `checks[]` 是否出现

### 2.4 kind / reason / source 的未来扩展

当前集合是稳定基线，但未来**允许新增**：

- 新的 `kind`
- 新的 `reason`
- 新的 `source`

新增时应同步：

- 代码
- 测试
- `doctor-契约盘点-v1.md`

### 2.5 非 doctor 面的跨命令耦合

虽然当前 `doctor`、`browser status`、`browser logs`、`browser list` 已经做了大量对齐，
但不要假设它们所有字段名都会完全镜像复制。  
应以各自命令的当前契约为准。

## 3. 当前阶段结论

对上层自动化来说，当前 `doctor` 已经可以作为一个可消费的壳层契约使用，尤其适合：

- 判断失败点
- 识别自动可修项
- 读取 daemon 相关机器状态
- 消费 `--fix` 的 applied actions

当前阶段不建议继续无限扩字段。  
更合理的下一步是：

1. 只在真实调用方需要时再扩契约
2. 每次扩展都先定义“稳定字段”与“非承诺范围”
3. 保持代码、测试、文档三方同时收口
