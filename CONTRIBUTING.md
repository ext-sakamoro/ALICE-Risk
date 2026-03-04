# Contributing to ALICE-Risk

## Prerequisites

- Rust 1.70+
- `alice-ledger` crate (path dependency `../ALICE-Ledger`)

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

## Code Style

- `cargo fmt` — 必須
- `cargo clippy --tests -- -W clippy::all -W clippy::pedantic` — 警告ゼロ
- `cargo doc --no-deps` — 警告ゼロ
- コメント・コミットメッセージは日本語
- 作成者: Moroya Sakamoto

## Design Constraints

- **Pre-trade only**: all checks run *before* order submission — no post-trade corrections.
- **Integer arithmetic**: prices and quantities are `i64` ticks, matching ALICE-Ledger.
- **Deterministic**: identical inputs produce identical accept/reject decisions on all platforms.
- **Circuit breaker isolation**: tripped state persists across daily resets — must be explicitly cleared.
- **No external dependencies**: only ALICE-Ledger types.
