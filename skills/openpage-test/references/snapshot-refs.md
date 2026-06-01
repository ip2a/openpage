# Snapshot and Refs

How to use OpenPage's agent-facing snapshot contract efficiently and safely.

## Why this exists

OpenPage's `snapshot` command is the highest-signal read surface for agent loops:

- it returns compact interactive-element structure
- it assigns stable-in-that-moment `eN` refs
- it also returns `text` and `refs` summaries that are easier to pass through an LLM than raw HTML

This is the intended workflow for agent-driven page interaction. It is better than asking the model to parse full HTML and invent CSS selectors every turn.

## The core loop

```bash
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session agent --headless https://example.com
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- snapshot --session agent
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- click @e1 --session agent
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- snapshot --session agent
```

Read the page with `snapshot`, act on `@eN`, then re-snapshot after page state changes.

## Snapshot output contract

Current OpenPage snapshot output includes:

- `snapshot` — structured interactive-element array
- `text` — compact ref-oriented summary
- `refs` — object keyed by `eN`
- `label` / `checked` / `selected` / `disabled` metadata when available
- `origin` — best-effort page origin
- `title` — best-effort page title
- `interactive_count` — count of interactive entries

Typical shape:

```json
{
  "ok": true,
  "result": {
    "origin": "https://example.com",
    "title": "Example",
    "interactive_count": 3,
    "text": "@e1 [a] \"Docs\"\n@e2 [input] placeholder=\"Search\"\n@e3 [button] \"Submit\"",
    "refs": {
      "e1": { "tag": "a", "text": "Docs" },
      "e2": { "tag": "input", "label": "Search", "placeholder": "Search" },
      "e3": { "tag": "button", "text": "Submit", "disabled": true }
    },
    "snapshot": [
      { "ref": "e1", "tag": "a", "text": "Docs" },
      { "ref": "e2", "tag": "input", "label": "Search", "placeholder": "Search" },
      { "ref": "e3", "tag": "button", "text": "Submit", "disabled": true }
    ]
  }
}
```

Important detail:

- follow-up action commands use `@e1`
- the `refs` object itself is keyed as `e1`

## Ref lifecycle

Refs are only valid for the current rendered state.

Re-snapshot after:

- navigation
- form submit
- opening a modal or dropdown
- `click-for-new-tab`
- `tab switch`
- `frame switch`
- major dynamic re-render

If the page changes, assume the old ref map is stale.

OpenPage now clears previously assigned `data-op-ref` attributes at the start
of every new `snapshot` pass before minting the next ref set. That does not
make old refs safe to reuse — you should still treat them as stale — but it
does reduce the chance of dynamic pages keeping leftover ref markers on
elements that are no longer in the current interactive snapshot.

## Best practices

### 1. Prefer refs over selectors in agent loops

For LLM-driven control, prefer:

```bash
openpage snapshot --session agent
openpage click @e3 --session agent
```

over:

```bash
openpage click '#complicated-generated-selector' --session agent
```

Use selectors when you already know the target precisely or are debugging a ref mismatch.

### 2. Re-snapshot after every meaningful UI change

Do not chain multiple `@eN` actions across a page transition without refreshing the snapshot first.

### 3. Use output boundaries when the snapshot text is going through a model

```bash
OPENPAGE_CONTENT_BOUNDARIES=1 \
OPENPAGE_MAX_OUTPUT_CHARS=2000 \
cargo run --manifest-path rust/Cargo.toml --bin openpage -- snapshot --session agent
```

This helps separate page content from surrounding tool chatter. It does **not** make page content trustworthy.

When OpenPage knows the current page origin, boundary metadata may also include
`_boundary.origin`, and the wrapped `text` payload may include `origin=...` in
the marker.

### 4. Use `text` for fast reasoning, `snapshot` / `refs` for exact follow-up

- `text` is the cheapest high-level summary
- `refs` is better for direct keyed lookup
- `snapshot` is the fuller structured source when you need more detail

## Troubleshooting

### `Ref not found`

The page changed. Run `snapshot` again and use the new refs.

### The target is not in the snapshot

Try:

```bash
openpage scroll down --session agent
openpage snapshot --session agent
```

or wait for the page to settle first:

```bash
openpage wait-for-load-start --session agent
openpage snapshot --session agent
```

### The snapshot is too large

Use:

- `OPENPAGE_MAX_OUTPUT_CHARS`
- a more focused session state before snapshotting
- a follow-up selector query only when you already know the area you need

## Practical rule

For agent control, treat:

```text
snapshot -> reason over text/refs -> act on @eN -> re-snapshot
```

as the default OpenPage interaction loop.
