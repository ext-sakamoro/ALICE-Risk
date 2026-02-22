# Contributing to ALICE-Risk

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

- **Pre-trade only**: all checks run *before* order submission — no post-trade corrections.
- **Integer arithmetic**: prices and quantities are `i64` ticks, matching ALICE-Ledger.
- **Deterministic**: identical inputs produce identical accept/reject decisions on all platforms.
- **Circuit breaker isolation**: tripped state persists across daily resets — must be explicitly cleared.
- **No external dependencies**: only ALICE-Ledger types.
