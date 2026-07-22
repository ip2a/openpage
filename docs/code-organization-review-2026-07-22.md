# 代码整理审查记录（2026-07-22）

## 1. 审查目的

本记录用于回答两个问题：

1. 本次 `Page` / `WebPage` 代码拆分是否降低了后续维护成本；
2. 现有证据是否足以证明业务能力、公开 API 和函数没有丢失。

审查流程：`Critical Thinking → Fetch → Deep Thinking → Review`。

本次只审查和记录，不修改业务代码，不把下载监听问题混入结构整理。

## 2. 审查结论

**结论：结构整理可以接受，但最终验收只能标记为“有条件通过”，不能标记为“全部通过”。**

已确认：

- `page/mod.rs` 已降至 2,793 行；
- `webpage/mod.rs` 已降至 416 行；
- Page 公开符号为 `590 / 590`，函数签名为 `946 / 946`；
- WebPage 公开符号为 `817 / 817`，函数签名为 `985 / 985`；
- Rust 编译、格式检查和 944 个测试已通过；
- MCP、Python 兼容测试、Python 导入、NPM CLI、桌面构建均已有通过记录；
- 每个主要拆分里程碑均有独立 Git 提交，可单独定位和回滚；
- 当前没有继续拆分大文件的必要，继续按行数拆分会增加模块数量和维护跳转成本。

尚不能确认：

- Python 完整集成测试稳定全绿；
- 所有迁移方法的函数体已通过自动化逐项等价比较；
- 下载开始监听的间歇性失败已被直接证明与本次整理无关。

因此，`docs/code-organization-refactor-contract.md` 中“全部结构拆分与跨表面验收通过”“完成标准全部满足”等表述证据不足，需要后续单独修正。

## 3. 已落地的结构变化

### 3.1 Page

`page/mod.rs` 的职责已拆入：

```text
page/actions.rs
page/cookies.rs
page/dialogs.rs
page/frame.rs
page/interaction.rs
page/lifecycle.rs
page/navigation.rs
page/operations.rs
page/screenshot.rs
page/settings.rs
page/tabs.rs
page/tests.rs
```

### 3.2 WebPage

`webpage/mod.rs` 的职责已拆入：

```text
webpage/assets.rs
webpage/cookies.rs
webpage/element.rs
webpage/extraction.rs
webpage/frame.rs
webpage/html.rs
webpage/operations.rs
webpage/parsing.rs
webpage/request.rs
webpage/response.rs
webpage/settings.rs
webpage/tests.rs
```

这次整理的实际收益是让入口文件主要承担类型、构造、导出和模块组织职责，业务方法按职责定位。没有引入 service、factory、adapter、facade 或兼容层。

## 4. 能力不丢失的现有证据

### 4.1 符号基线

基线文件：

```text
refactor/page-api-baseline.txt
refactor/webpage-api-baseline.txt
```

最终对比记录：

| 范围 | 公开符号 | 函数签名 | 结果 |
|---|---:|---:|---|
| Page | 590 / 590 | 946 / 946 | 未发现新增或丢失 |
| WebPage | 817 / 817 | 985 / 985 | 未发现新增或丢失 |

该证据可以证明名称和签名集合没有丢失，但不能单独证明函数体、调用顺序、错误消息和副作用完全一致。

### 4.2 编译与测试

已有验收记录：

```text
cargo fmt --check                         通过
cargo check                               通过
cargo test                                738 + 206 = 944 通过
bash scripts/test/mcp_smoke_test.sh       通过
bash scripts/dev/dev_install.sh           通过
tests/python/test_compat_download_wait.py 89 通过
Python openpage/openpage_rs 导入          通过
npm run build --prefix desktop/openpage   通过
NPM CLI --help                            exit 0
CLI invalid command                       exit 2, kind=invalid_input
```

这些检查覆盖主要交付表面，但 Python 完整集成测试仍存在下载监听时序波动，不能写成完整套件已稳定通过。

### 4.3 提交隔离

主要结构提交包括：

```text
34f29cb refactor: remove empty implementation shells
c7c7f54 refactor(webpage): move core operations without behavior changes
b9418b2 refactor(webpage): move setting wrappers without behavior changes
7946300 refactor(webpage): move element methods without behavior changes
9c3d2d2 refactor(webpage): move frame methods without behavior changes
511aef1 refactor(page): move core operations without behavior changes
380e269 refactor(page): move setting wrappers without behavior changes
cec5e8d refactor(page): move actions methods without behavior changes
0a74029 refactor(page): move frame methods without behavior changes
```

独立提交降低了审查和回滚成本，也避免了把业务修复伪装成结构移动。

## 5. 审查发现

### P1：最终验收文档结论过度

完整运行 `tests/python/test_openpage.py` 时，下载开始监听相关测试出现过间歇性失败：

```text
OpenPageIntegrationTest.test_page_wait_download_begin_cancel_it_returns_info_dict
OpenPageIntegrationTest.test_page_waits_for_download_begin_and_completion
OpenPageIntegrationTest.test_webpage_waits_for_download_begin_and_completion
```

两次完整运行分别出现 2 个和 3 个同类失败；个别目标用例重跑曾通过，但之后仍可复现波动。

因此不能把“目标用例偶尔重跑通过”当作“完整跨表面验收通过”。正确状态应是：

> 结构整理完成；最终验收存在已知下载监听时序波动。

### P2：函数体等价尚未自动证明

当前基线审计验证了公开符号和函数签名集合，没有逐项比较迁移前后的函数体。

剩余的最小补证方式：

1. 从重构前提交读取原始 `page/mod.rs` 和 `webpage/mod.rs`；
2. 从当前职责模块提取 inherent methods；
3. 按“类型名 + 归一化签名”匹配；
4. 比较归一化函数体哈希；
5. 输出新增、丢失、重复和函数体变化清单；
6. 临时审计脚本放在 `/tmp`，不提交到仓库，只把结果写入契约。

在该比较完成前，只能说“未发现能力丢失”，不能绝对声称“所有函数体完全等价”。

### P3：大职责文件是后续债，不是本轮阻塞项

当前较大的职责文件：

| 文件 | 行数 |
|---|---:|
| `webpage/element.rs` | 2,770 |
| `page/operations.rs` | 2,332 |
| `page/frame.rs` | 2,020 |
| `webpage/operations.rs` | 1,975 |
| `webpage/frame.rs` | 1,479 |

这些文件虽然仍大，但职责边界基本明确。本轮不应仅为了降低行数继续拆分；只有出现高频修改冲突、职责混杂或审查困难时，再开启第二阶段整理。

## 6. 后续验收门槛

只有以下条件全部满足，才允许把状态改为“最终验收通过”：

- [ ] 修正现有契约中的过度结论；
- [ ] 完成 Page / WebPage 函数体等价审计并保留结果；
- [ ] 在重构前基线提交上运行相同下载监听测试，直接确认时序波动是否原先存在；
- [ ] 如果波动只在当前版本出现，按独立业务 bug 处理，不混入结构整理；
- [ ] 重新运行 Rust、MCP、Python、NPM CLI 和桌面构建验收；
- [ ] `git diff --check` 通过，生成产物未被跟踪，工作树干净；
- [ ] 文档结论与真实测试结果一致。

## 7. 维护规则

后续继续整理时必须遵守：

1. 原样移动，不顺手改业务逻辑；
2. 每个里程碑先验证符号和测试，再提交；
3. 业务 bug 使用独立提交；
4. 不增加无必要的 helper、adapter、factory、兼容层或 fallback；
5. 不以文件行数作为唯一拆分理由；
6. 任一验收失败都必须如实记录，禁止用局部重跑通过覆盖完整套件失败。

## 8. 本次审查状态

```text
结构整理：完成
符号/签名保持：通过
Rust 验收：通过
主要跨表面 smoke：通过
Python 完整集成测试：存在间歇性失败
函数体自动等价审计：待完成
最终结论：有条件通过
```
