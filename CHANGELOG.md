# Changelog

All notable changes to ALICE-Risk will be documented in this file.

## [0.1.0] - 2026-02-23

### Added
- `limit` — `RiskLimits` configuration (max order size, position limit, notional cap, daily loss limit, open order cap)
- `check` — `PreTradeChecker` enforcing all limits before order submission
- `margin` — `MarginCalculator` for initial / maintenance margin with `MarginParams`
- `circuit` — `CircuitBreaker` halting trading on anomalous price moves or fill rates
- `RiskReject` enum — typed rejection reasons (OrderSizeExceeded, PositionLimitBreached, NotionalExceeded, DailyLossExceeded, TooManyOpenOrders, CircuitBreakerTripped)
- Daily reset, circuit breaker trip/reset, P&L tracking
- Integration with ALICE-Ledger order types
- 85 tests (84 unit + 1 doc-test)
