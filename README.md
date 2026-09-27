# ALICE-Risk

Pre-trade risk checks, margin calculation, and circuit breakers for the ALICE financial system.

## Architecture

```
Order ──► PreTradeChecker ──► Accept / RiskReject
              │
              ├── Order size check
              ├── Position limit check
              ├── Notional value check
              ├── Open order count check
              ├── Daily loss limit check
              └── Circuit breaker check

MarginCalculator ──► Initial / Maintenance margin
                 ──► Margin call detection
                 ──► Liquidation price

CircuitBreaker ──► Price deviation monitor
               ──► Fill rate monitor
```

## Features

- **PreTradeChecker** — 6-step sequential risk gate with deterministic accept/reject
- **MarginCalculator** — Initial/maintenance margin, margin call, liquidation price (i128 overflow-safe)
- **CircuitBreaker** — Price move and fill rate monitoring with rolling window
- **RiskLimits** — Per-account/per-instrument configurable thresholds
- Integer arithmetic throughout (i64 ticks, matching ALICE-Ledger)
- Property-based testing with proptest

## Quick Start

```rust
use alice_risk::{
    limit::RiskLimits,
    check::PreTradeChecker,
    margin::{MarginCalculator, MarginParams},
    circuit::CircuitBreaker,
};

let limits = RiskLimits::default();
let checker = PreTradeChecker::new(limits);

let params = MarginParams::default(); // 10% initial, 5% maintenance
let calc = MarginCalculator::new(params);

let mut cb = CircuitBreaker::new(500, 5, 1_000_000_000);
cb.reset(10_000, 0);
```

## License

`AGPL-3.0-only OR LicenseRef-Commercial` — dual-licensed. Pick either.

| Option | Terms | Use it when |
|--------|-------|-------------|
| **AGPL-3.0-only** | [LICENSE-AGPL](LICENSE-AGPL) — free, no reporting obligation | Your project is itself AGPL-compatible open source, or you are only using it internally |
| **Commercial License** | [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) — paid, removes the copyleft | Closed-source product, proprietary SaaS, edge / firmware distribution, plugin redistribution, or a platform NDA that forbids source disclosure |

AGPL is a strong copyleft: a product, firmware image, or service that links
`alice-risk` and is distributed or served to users must be released under the AGPL
as well. That is intentional for the open ecosystem, and the Commercial
License exists for the cases where it is not something you are able to do.

Commercial licence enquiries: <contact@extoria.co.jp>
