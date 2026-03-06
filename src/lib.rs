#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::module_name_repetitions,
    clippy::inline_always,
    clippy::too_many_lines
)]
#![cfg_attr(
    test,
    allow(
        clippy::uninlined_format_args,
        clippy::absurd_extreme_comparisons,
        clippy::manual_range_contains,
        unused_must_use,
    )
)]
/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! # ALICE-Risk
//!
//! Pre-trade risk management engine for the ALICE financial system.
//!
//! Provides three main subsystems:
//!
//! - [`limit`]   — [`RiskLimits`] configuration for per-account and per-instrument thresholds
//! - [`check`]   — [`PreTradeChecker`] that enforces limits before order submission
//! - [`margin`]  — [`MarginCalculator`] for initial and maintenance margin requirements
//! - [`circuit`] — [`CircuitBreaker`] that halts trading on anomalous price moves or fill rates
//!
//! ## Example
//!
//! ```rust
//! use alice_risk::{
//!     limit::RiskLimits,
//!     check::PreTradeChecker,
//!     margin::{MarginCalculator, MarginParams},
//!     circuit::CircuitBreaker,
//! };
//! use alice_ledger::{Order, OrderId, OrderType, Side, TimeInForce};
//!
//! // Configure risk limits.
//! let limits = RiskLimits::default();
//! let mut checker = PreTradeChecker::new(limits);
//!
//! // Submit an order for pre-trade check.
//! let order = Order {
//!     id: OrderId(1),
//!     side: Side::Bid,
//!     order_type: OrderType::Limit,
//!     price: 50_000,
//!     quantity: 10,
//!     filled_quantity: 0,
//!     timestamp_ns: 0,
//!     time_in_force: TimeInForce::GTC,
//! };
//!
//! assert!(checker.check_order(&order, None).is_ok());
//! ```

pub mod check;
pub mod circuit;
pub mod counterparty;
pub mod greeks;
pub mod limit;
pub mod margin;
pub mod stress;
pub mod var;

pub use check::{PreTradeChecker, RiskReject};
pub use circuit::CircuitBreaker;
pub use counterparty::{CounterpartyLimits, CounterpartyReject, CounterpartyTracker};
pub use greeks::{check_greeks, GreeksExposure, GreeksLimits, GreeksReject};
pub use limit::RiskLimits;
pub use margin::{MarginCalculator, MarginParams};
pub use stress::{apply_scenario, stress_test_portfolio, StressResult, StressScenario};
pub use var::{HistoricalVaR, ParametricVaR};

/// ALICE-Risk crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
