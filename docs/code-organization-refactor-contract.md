# 代码整理契约：能力与函数不丢失

- 状态：执行中（基线已建立，准备拆分 page）
- 日期：2026-07-22
- 适用范围：`rust/crates/openpage/src/page/`、`rust/crates/openpage/src/webpage/`
- 目标：只整理代码位置，保证业务行为、公开函数和跨语言接口不发生意外变化

## 1. 核心承诺

本次工作是**结构性重排**，不是功能重构。

必须保持不变：

- 公共类型、函数名、参数、返回值和错误类型；
- CLI 命令、参数、退出码和 JSON 错误语义；
- Python/NPM 暴露的调用方式；
- 浏览器导航、交互、Cookie、截图、弹窗、Tab 等业务能力；
- 超时、重试、并发、资源释放和事件顺序；
- 现有测试和 smoke test 的行为。

如果整理过程中发现业务 bug，必须单独提交修复，不能混入整理提交。

## 2. 允许的变化

只允许以下变化：

- 将同一 `impl` 中的方法移动到职责对应的子模块；
- 增加或调整 `mod` 声明和 `use` 导入；
- 为保持可见性而使用 `pub(super)`、`pub(crate)`；
- 将测试从实现文件移动到对应的 `tests.rs`；
- 删除由移动产生的无用导入、无用局部变量和重复模块声明。

不允许在本轮同时进行：

- API 改名；
- 错误类型或错误消息重写；
- 异步模型改造；
- trait、factory、service、facade 等新抽象；
- 性能优化；
- 业务逻辑重写；
- 跨目录大规模重命名。

## 3. 整理前的能力清单

实施任何移动前，先生成并提交一份基线清单：

```text
refactor/page-api-baseline.txt
refactor/webpage-api-baseline.txt
```

清单至少包含：

- `pub` / `pub(crate)` 类型、函数、方法、常量；
- 所有 `impl` 块中的方法签名；
- CLI 对应的 page/webpage 调用入口；
- Python bindings 直接调用的 Rust 方法；
- 现有测试函数名称；
- 每个能力对应的原始文件和行号。

示例记录：

```text
capability: page.navigation.goto
symbol: OxPage::goto
before: rust/crates/openpage/src/page/mod.rs:<line>
after:  rust/crates/openpage/src/page/navigation.rs
public: yes
behavior_change: none
covered_by: <test names>
```

行号只是定位信息，不是永久契约；符号名和签名才是主要校验依据。

## 4. 目标目录与能力映射

### 4.1 Page

| 能力 | 目标文件 | 保护重点 |
|---|---|---|
| 导航、刷新、前进后退、等待导航 | `page/navigation.rs` | URL、等待条件、超时、navigation token |
| 点击、输入、聚焦、滚动、键盘 | `page/interaction.rs` | 元素定位、事件顺序、错误映射 |
| Tab 创建、关闭、切换、枚举 | `page/tabs.rs` | session/page 归属、生命周期 |
| Cookie 读写和清理 | `page/cookies.rs` | domain、path、敏感数据处理 |
| 截图和图片输出 | `page/screenshot.rs` | 格式、尺寸、输出路径 |
| alert、confirm、prompt | `page/dialogs.rs` | 监听注册、响应时序、超时 |
| 创建、关闭、销毁、状态检查 | `page/lifecycle.rs` | 资源释放、重复关闭、连接状态 |
| 类型、字段、构造函数、模块导出 | `page/mod.rs` | 可见性和公共 API |

### 4.2 WebPage

| 能力 | 目标文件 | 保护重点 |
|---|---|---|
| 请求构造和发送 | `webpage/request.rs` | URL、headers、超时、重试 |
| response 状态和 body | `webpage/response.rs` | 状态码、编码、错误处理 |
| HTML 获取和转换 | `webpage/html.rs` | 编码和空响应 |
| DOM/selector 解析 | `webpage/parsing.rs` | selector 语义、节点顺序 |
| 文本、链接、表格等提取 | `webpage/extraction.rs` | 返回结构和空值语义 |
| Cookie jar 和 Cookie header | `webpage/cookies.rs` | domain、过期和隔离 |
| 图片、脚本、样式等资源 | `webpage/assets.rs` | URL 解析、下载错误 |
| 类型、构造函数、导出 | `webpage/mod.rs` | 可见性和公共 API |

## 5. 每个拆分提交的强制流程

每次只移动一个能力模块：

1. 更新能力映射表；
2. 移动完整方法，不手工重写逻辑；
3. 运行 `cargo fmt`；
4. 运行 `cargo check --manifest-path rust/Cargo.toml`；
5. 运行受影响模块的测试；
6. 运行完整 Rust 测试；
7. 检查 API 清单差异；
8. 检查 `git diff --stat` 和 `git diff --check`；
9. 确认没有产生业务代码差异；
10. 单独提交。

提交信息格式：

```text
refactor(page): move navigation methods without behavior changes
```

## 6. 函数不丢失的自动检查

每次整理后都要比较整理前后的符号清单：

```bash
rg -n '^\s*(pub\s+)?(async\s+)?fn\s+' \
  rust/crates/openpage/src/page \
  > refactor/page-functions-after.txt

rg -n '^\s*(pub\s+)?(async\s+)?fn\s+' \
  rust/crates/openpage/src/webpage \
  > refactor/webpage-functions-after.txt
```

检查规则：

- 原有函数不能无故消失；
- 函数签名不能改变；
- 仅允许文件路径变化；
- 测试函数移动不算丢失；
- 删除函数必须有单独说明和调用方搜索证据。

调用方检查：

```bash
rg -n 'SymbolName|TypeName::method_name' \
  rust python npm desktop tests examples
```

## 7. 行为不丢失的验证层级

### 第一级：编译验证

```bash
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo check --manifest-path rust/Cargo.toml
```

### 第二级：Rust 单元测试

```bash
cargo test --manifest-path rust/Cargo.toml -- --test-threads=1
```

### 第三级：跨表面验证

```bash
bash scripts/test/mcp_smoke_test.sh
python/.venv/bin/python tests/python/test_compat_download_wait.py
python/.venv/bin/python tests/python/test_openpage.py
npm run build --prefix desktop/openpage
```

### 第四级：契约验证

重点确认：

- CLI help 和错误 JSON 没变；
- `navigation_token` 相关流程仍然成立；
- Python `openpage` 和 `openpage_rs` 仍可导入；
- NPM CLI 仍能启动；
- page/webpage 的公开符号清单无减少。

## 8. 失败处理

任何一个检查失败：

- 停止继续拆分；
- 保留当前提交，方便回滚；
- 判断是导入/可见性问题还是行为变化；
- 不通过增加兼容层来掩盖问题；
- 修复后重新执行本次提交的全部检查。

如果无法证明某个函数仍然存在，默认视为整理失败，不进入下一模块。

## 9. 完成标准

本轮整理完成必须满足：

- `page/mod.rs` 主要保留类型、字段、构造函数和导出；
- `webpage/mod.rs` 主要保留类型、构造函数和导出；
- 所有基线符号都有目标位置；
- 没有未经说明的公开 API 变化；
- Rust、Python、MCP、桌面构建验证通过；
- 每个拆分提交都可以独立回滚；
- 文档中的能力映射和实际目录一致。

## 10. 第一批执行范围

第一批只做：

```text
page/navigation.rs
page/tabs.rs
page/interaction.rs
```

暂不处理：

```text
webpage 全量拆分
element/mod.rs
element_list/mod.rs
公共 helper 抽取
错误系统重构
```

原因：先验证“移动代码而不改变行为”的流程，成功后再扩大范围。


## 11. 执行进度

| 里程碑 | 状态 | 验证 | Git 提交 |
|---|---|---|---|
| 建立整理契约 | 已完成 | 文档已落盘 | 本里程碑提交 |
| 建立 page/webpage 符号基线 | 已完成 | `cargo check`、944 个 Rust 测试、MCP、Python 兼容测试、桌面构建通过；Python 集成套件 1 个下载时序用例首次失败，单独重跑通过 | 本里程碑提交 |
| 拆分 page | 进行中 | 第一批 navigation、tabs、interaction 已完成；公开符号与函数签名无丢失；各里程碑 944 个 Rust 测试通过 | 761c2bb、035948d、本里程碑提交 |
| 拆分 webpage | 未开始 | - | - |
| 全量验收 | 未开始 | - | - |

基线文件：

- `refactor/page-api-baseline.txt`：1081 个声明项；
- `refactor/webpage-api-baseline.txt`：1014 个声明项；
- 基线记录包括声明位置、所属 `impl`、可见性、测试标识和归一化签名。

### 2026-07-22 基线验证记录

- `cargo fmt --check`：通过；
- `cargo check`：通过；
- Rust：`738 + 206 = 944` 个测试通过；
- MCP smoke test：通过；
- Python 兼容测试：89 个通过；
- Python 集成测试：66 个中 1 个下载开始时序用例首次失败；该用例单独重跑通过，记录为现有时序波动，不在整理提交中修改；
- 桌面 TypeScript/Vite 生产构建：通过。

### 2026-07-22 Page navigation 里程碑

- 已新增 `page/navigation.rs`，原样迁移 15 个导航相关方法；
- `page/mod.rs` 减少 201 行；
- 公开符号对比：590 / 590，无新增、无丢失；
- 函数签名对比：946 / 946，无新增、无丢失；
- 唯一可见性调整：`navigation_page_load_timeout_ms` 从模块私有改为 `pub(super)`，仅用于维持父模块内既有调用，不构成公开 API；
- `cargo fmt --check`：通过；
- `cargo check`：通过；
- Rust：944 个测试通过。

### 2026-07-22 Page tabs 里程碑

- 已新增 `page/tabs.rs`，原样迁移 11 个 Tab 枚举、创建、激活、关闭和窗口标识方法；
- 公开符号对比：590 / 590，无新增、无丢失；
- 函数签名对比：946 / 946，无新增、无丢失；
- `cargo fmt --check`：通过；
- `cargo check`：通过；
- Rust：944 个测试通过。

### 2026-07-22 Page interaction 里程碑

- 已新增 `page/interaction.rs`，原样迁移 31 个页面交互、元素读写、滚动、上传和点击衍生方法；
- 公开符号对比：590 / 590，无新增、无丢失；
- 函数签名对比：946 / 946，无新增、无丢失；
- `cargo fmt --check`：通过；
- `cargo check`：通过；
- Rust：944 个测试通过；
- 第一批 `navigation`、`tabs`、`interaction` 已按契约完成。

### 2026-07-22 Page tests 里程碑

- 已将 `page/mod.rs` 中完整的内联测试模块原样迁移到 `page/tests.rs`，父模块仅保留 `#[cfg(test)] mod tests;`；
- `page/mod.rs` 从约 17,000 行降至 8,150 行，生产代码逻辑未改写；
- 公开符号对比：590 / 590，无新增、无丢失；
- 函数签名集合与基线一致，无新增、无丢失；
- `cargo fmt --check`：通过；
- `cargo check`：通过；
- Rust：`738 + 206 = 944` 个测试通过。

### 2026-07-22 Page cookies 里程碑

- 已新增 `page/cookies.rs`，原样迁移 `Page::cookie_header` 与 `Page::cookies`；
- 方法签名多重集合与基线一致，公开方法无新增、无丢失；
- `cargo fmt --check`：通过；
- `cargo check`：通过；
- Rust：`738 + 206 = 944` 个测试通过。
