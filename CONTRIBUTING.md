# Contributing to ALICE-Settlement

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Lint

```bash
cargo clippy -- -W clippy::all
cargo fmt -- --check
cargo doc --no-deps 2>&1 | grep warning
```

## Design Constraints

- **Integer arithmetic**: prices and quantities are `i64` / `u64` ticks — no floating-point.
- **Deterministic netting**: bilateral key is always `(min(a,b), max(a,b))` for canonical ordering.
- **Hash-chained journal**: each entry includes a hash of the previous entry for tamper detection.
- **Replay verification**: replaying the journal from scratch must produce identical state or flag discrepancies.
- **Waterfall cascade**: loss absorption layers are applied in order; each layer absorbs up to its limit before passing to the next.
