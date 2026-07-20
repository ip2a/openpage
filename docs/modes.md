# OpenPage Modes

`openpage` is the product name. Modes describe how the same capability is
started or embedded.

## Default Agent Mode

The intended default for package runners is MCP mode:

```bash
npx openpage
uvx openpage
openpage
```

The command can also expose MCP explicitly:

```bash
openpage mcp
openpage mcp serve
```

## CLI Mode

CLI commands remain subcommands of the same binary:

```bash
openpage browser start
openpage goto https://example.com
openpage snapshot
openpage doctor
```

## Daemon Mode

The TCP daemon is an implementation mode behind the same command:

```bash
openpage serve
```

## Library Mode

Rust and Python users import `openpage` from their language ecosystem without
using a separate product name.
