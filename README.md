# OpenPage

OpenPage 是一个 **Rust-first** 浏览器自动化项目。Rust 是唯一的领域模型和行为来源；Python 通过 PyO3 暴露同一套门面，CLI 通过 Rust 核心提供命令行和协议入口。

## 顶层模型

```text
Browser
└── Page

Session
└── Response
    └── Document
        └── DocumentElement
```

Python 顶层只公开三个核心类型和一个便捷入口：

```python
from openpage import Browser, Page, Session, open

browser = Browser.launch()
page = browser.new_page()
page.goto("https://example.com")

session = Session()
response = session.get("https://example.com")
document = response.document
heading = document.find("h1")
```

`Page` 提供实时页面操作：

```python
page.find("#content")
page.find_all("a")
page.click("#submit")
page.input("#name", "hello")
page.text("#title")
page.attr("#link", "href")
```

静态页面快照使用明确的 `snapshot` 入口；HTTP 响应文档使用 `document`。实时对象统一使用 `find()` 和 `find_all()`，不提供旧的复合页面模型或旧查询别名。

## 目录

- `rust/crates/openpage`：Rust 核心领域模型和行为
- `rust/apps/openpage`：CLI、daemon 和 MCP 入口
- `rust/bindings/python`：PyO3 绑定
- `python/openpage`：Python 顶层门面
- `tests/python`：Python 门面测试
- `scripts/test`：统一测试和 smoke 入口
- `scripts/release`：版本与发布元数据检查

## 本地开发

需要 Rust、Cargo 和 uv：

```bash
./scripts/dev/dev_install.sh
./scripts/test/run_checks.sh
```

Rust 检查：

```bash
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml -- --test-threads=1
```

构建 Python wheel：

```bash
uv build --wheel
```

## CLI

构建并查看帮助：

```bash
cargo run --manifest-path rust/apps/openpage/Cargo.toml --bin openpage -- --help
```

CLI 的 daemon 使用 TCP；MCP 使用独立的 stdio 入口：

```bash
cargo run --manifest-path rust/apps/openpage/Cargo.toml --bin openpage -- serve --session agent
cargo run --manifest-path rust/apps/openpage/Cargo.toml --bin openpage -- mcp --session agent
```

## 设计边界

- Rust 核心是唯一行为来源。
- Python 只负责 PyO3 类型暴露和 Python 调用方式，不复制浏览器业务逻辑。
- `Browser` 管理浏览器和页面；`Page` 表示实时浏览器页面；`Session` 表示 HTTP 会话。
- 旧的复合页面类型、兼容别名、回退导入和废弃入口不属于当前产品表面。
- 每次重构里程碑都必须有对应验证、文档记录和 Git 提交。

详细决策和执行记录见：

- `openpage重构记录.md`
- `技术文档-Python门面.md`
- `大规模重构目标文档-v1.md`
