/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! Pre-trade risk checks enforced before order submission.
//!
//! [`PreTradeChecker`] runs a sequence of limit checks on every incoming order.
//! Any breach immediately returns a [`RiskReject`] variant describing the
//! violation; if all checks pass, `Ok(())` is returned and the order may proceed
//! to the matching engine.

use alice_ledger::{Order, Position, Side};

use crate::limit::RiskLimits;

// ---------------------------------------------------------------------------
// RiskReject
// ---------------------------------------------------------------------------

/// Reason an order was rejected by the pre-trade risk engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskReject {
    /// Net position after this order would exceed the configured limit.
    PositionLimitBreached {
        /// Current net position before the order.
        current: i64,
        /// Net position that would result if the order were filled.
        after: i64,
        /// Configured maximum absolute position in lots.
        limit: u64,
    },
    /// Order quantity exceeds the per-order size limit.
    OrderSizeTooLarge {
        /// Requested order quantity.
        size: u64,
        /// Configured maximum order size in lots.
        limit: u64,
    },
    /// Notional value of the order exceeds the configured ceiling.
    NotionalExceeded {
        /// Computed notional (price * quantity) for this order.
        notional: i64,
        /// Configured maximum notional in ticks.
        limit: i64,
    },
    /// Number of open orders has reached the configured maximum.
    MaxOpenOrdersReached {
        /// Current open order count.
        count: u32,
        /// Configured maximum open orders.
        limit: u32,
    },
    /// Daily loss has reached or exceeded the configured kill-switch threshold.
    DailyLossLimitHit {
        /// Current daily P&L (negative indicates a loss).
        loss: i64,
        /// Configured maximum daily loss (negative value).
        limit: i64,
    },
    /// A circuit breaker has been manually tripped; all orders are blocked.
    CircuitBreakerTripped,
}

// ---------------------------------------------------------------------------
// PreTradeChecker
// ---------------------------------------------------------------------------

/// Stateful pre-trade risk engine.
///
/// Holds running counters (daily P&L, open order count, circuit breaker state)
/// and evaluates each incoming order against the configured [`RiskLimits`].
pub struct PreTradeChecker {
    limits: RiskLimits,
    /// Accumulated P&L for the current trading day (may be negative).
    daily_pnl: i64,
    /// Number of orders currently resting on the book.
    open_order_count: u32,
    /// When `true`, all new orders are rejected until explicitly reset.
    circuit_breaker_tripped: bool,
}

impl PreTradeChecker {
    /// Create a new checker with the given risk limits.
    #[inline(always)]
    pub fn new(limits: RiskLimits) -> Self {
        Self {
            limits,
            daily_pnl: 0,
            open_order_count: 0,
            circuit_breaker_tripped: false,
        }
    }

    /// Run all pre-trade risk checks for `order` against the optional current
    /// `position`.
    ///
    /// Checks are applied in the following order:
    /// 1. Circuit breaker
    /// 2. Order size
    /// 3. Resulting position size
    /// 4. Notional value
    /// 5. Open order count
    /// 6. Daily loss limit
    ///
    /// Returns `Ok(())` if every check passes, or the first [`RiskReject`]
    /// variant that fires.
    pub fn check_order(
        &self,
        order: &Order,
        position: Option<&Position>,
    ) -> Result<(), RiskReject> {
        // 1. Circuit breaker takes priority over all other checks.
        if self.circuit_breaker_tripped {
            return Err(RiskReject::CircuitBreakerTripped);
        }

        // 2. Order size check.
        if order.quantity > self.limits.max_order_size {
            return Err(RiskReject::OrderSizeTooLarge {
                size: order.quantity,
                limit: self.limits.max_order_size,
            });
        }

        // 3. Position limit check — compute net position after this order.
        let current_net: i64 = position.map(|p| p.net_quantity).unwrap_or(0);
        let signed_delta: i64 = match order.side {
            Side::Bid => order.quantity as i64,
            Side::Ask => -(order.quantity as i64),
        };
        let after_net: i64 = current_net.saturating_add(signed_delta);
        if after_net.unsigned_abs() > self.limits.max_position {
            return Err(RiskReject::PositionLimitBreached {
                current: current_net,
                after: after_net,
                limit: self.limits.max_position,
            });
        }

        // 4. Notional value check.  Use i128 to avoid overflow during
        //    multiplication, then saturate back to i64 for comparison.
        let notional: i64 = {
            let n = (order.price as i128).saturating_mul(order.quantity as i128);
            n.min(i64::MAX as i128) as i64
        };
        if notional > self.limits.max_notional {
            return Err(RiskReject::NotionalExceeded {
                notional,
                limit: self.limits.max_notional,
            });
        }

        // 5. Open order count check.
        if self.open_order_count >= self.limits.max_open_orders {
            return Err(RiskReject::MaxOpenOrdersReached {
                count: self.open_order_count,
                limit: self.limits.max_open_orders,
            });
        }

        // 6. Daily loss limit check.
        if self.daily_pnl <= self.limits.max_daily_loss {
            return Err(RiskReject::DailyLossLimitHit {
                loss: self.daily_pnl,
                limit: self.limits.max_daily_loss,
            });
        }

        Ok(())
    }

    /// Update the running daily P&L tracker.
    ///
    /// `pnl` is added to the accumulated total; a negative value represents
    /// a loss. When the total reaches `max_daily_loss`, subsequent orders
    /// will be rejected by [`Self::check_order`].
    #[inline(always)]
    pub fn update_daily_pnl(&mut self, pnl: i64) {
        self.daily_pnl = self.daily_pnl.saturating_add(pnl);
    }

    /// Record that a new order has been placed on the book.
    #[inline(always)]
    pub fn increment_open_orders(&mut self) {
        self.open_order_count = self.open_order_count.saturating_add(1);
    }

    /// Record that an open order has been cancelled or fully filled.
    #[inline(always)]
    pub fn decrement_open_orders(&mut self) {
        self.open_order_count = self.open_order_count.saturating_sub(1);
    }

    /// Trip the circuit breaker, blocking all further order submissions until
    /// [`Self::reset_circuit_breaker`] is called.
    #[inline(always)]
    pub fn trip_circuit_breaker(&mut self) {
        self.circuit_breaker_tripped = true;
    }

    /// Clear the circuit breaker, allowing order submissions to resume.
    #[inline(always)]
    pub fn reset_circuit_breaker(&mut self) {
        self.circuit_breaker_tripped = false;
    }

    /// Perform end-of-day reset: clears daily P&L and open order count.
    ///
    /// The circuit breaker state is intentionally preserved across daily
    /// resets; it must be explicitly cleared with [`Self::reset_circuit_breaker`].
    #[inline(always)]
    pub fn reset_daily(&mut self) {
        self.daily_pnl = 0;
        self.open_order_count = 0;
    }

    /// Return the current daily P&L value.
    #[inline(always)]
    pub fn daily_pnl(&self) -> i64 {
        self.daily_pnl
    }

    /// Return the current open order count.
    #[inline(always)]
    pub fn open_order_count(&self) -> u32 {
        self.open_order_count
    }

    /// Return whether the circuit breaker is currently tripped.
    #[inline(always)]
    pub fn is_circuit_breaker_tripped(&self) -> bool {
        self.circuit_breaker_tripped
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alice_ledger::{OrderId, OrderType, TimeInForce};

    fn make_order(side: Side, price: i64, quantity: u64) -> Order {
        Order {
            id: OrderId(1),
            side,
            order_type: OrderType::Limit,
            price,
            quantity,
            filled_quantity: 0,
            timestamp_ns: 0,
            time_in_force: TimeInForce::GTC,
        }
    }

    fn make_position(net_quantity: i64) -> Position {
        Position {
            symbol_hash: 0xDEAD_BEEF,
            net_quantity,
            avg_entry_price: 1000,
            realized_pnl: 0,
            unrealized_pnl: 0,
            trade_count: 0,
        }
    }

    fn default_checker() -> PreTradeChecker {
        PreTradeChecker::new(RiskLimits::default())
    }

    // -----------------------------------------------------------------------
    // Happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_order_passes_all_checks() {
        let checker = default_checker();
        let order = make_order(Side::Bid, 1000, 10);
        assert!(checker.check_order(&order, None).is_ok());
    }

    // -----------------------------------------------------------------------
    // Order size
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_order_size() {
        let checker = default_checker();
        // max_order_size = 100; quantity = 101 should fail.
        let order = make_order(Side::Bid, 1000, 101);
        let result = checker.check_order(&order, None);
        assert!(
            matches!(result, Err(RiskReject::OrderSizeTooLarge { .. })),
            "expected OrderSizeTooLarge, got {:?}",
            result
        );
        if let Err(RiskReject::OrderSizeTooLarge { size, limit }) = result {
            assert_eq!(size, 101);
            assert_eq!(limit, 100);
        }
    }

    // -----------------------------------------------------------------------
    // Position limit
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_position_limit_long() {
        let checker = default_checker();
        // Current long = 990, order adds 100 → net 1090 > max 1000.
        let position = make_position(990);
        let order = make_order(Side::Bid, 1000, 100);
        let result = checker.check_order(&order, Some(&position));
        assert!(
            matches!(result, Err(RiskReject::PositionLimitBreached { .. })),
            "expected PositionLimitBreached, got {:?}",
            result
        );
        if let Err(RiskReject::PositionLimitBreached {
            current,
            after,
            limit,
        }) = result
        {
            assert_eq!(current, 990);
            assert_eq!(after, 1090);
            assert_eq!(limit, 1000);
        }
    }

    #[test]
    fn test_reject_position_limit_short() {
        let checker = default_checker();
        // Current short = -990, order sells 100 → net -1090; abs > 1000.
        let position = make_position(-990);
        let order = make_order(Side::Ask, 1000, 100);
        let result = checker.check_order(&order, Some(&position));
        assert!(
            matches!(result, Err(RiskReject::PositionLimitBreached { .. })),
            "expected PositionLimitBreached, got {:?}",
            result
        );
        if let Err(RiskReject::PositionLimitBreached {
            current,
            after,
            limit,
        }) = result
        {
            assert_eq!(current, -990);
            assert_eq!(after, -1090);
            assert_eq!(limit, 1000);
        }
    }

    // -----------------------------------------------------------------------
    // Notional
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_notional() {
        let checker = default_checker();
        // price 10_000_000 * quantity 100 = 1_000_000_000 > max_notional 100_000_000.
        let order = make_order(Side::Bid, 10_000_000, 100);
        let result = checker.check_order(&order, None);
        assert!(
            matches!(result, Err(RiskReject::NotionalExceeded { .. })),
            "expected NotionalExceeded, got {:?}",
            result
        );
        if let Err(RiskReject::NotionalExceeded { notional, limit }) = result {
            assert_eq!(notional, 1_000_000_000);
            assert_eq!(limit, 100_000_000);
        }
    }

    // -----------------------------------------------------------------------
    // Max open orders
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_max_orders() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_open_orders: 2,
            ..RiskLimits::default()
        });
        checker.increment_open_orders();
        checker.increment_open_orders();

        let order = make_order(Side::Bid, 1000, 1);
        let result = checker.check_order(&order, None);
        assert!(
            matches!(result, Err(RiskReject::MaxOpenOrdersReached { .. })),
            "expected MaxOpenOrdersReached, got {:?}",
            result
        );
        if let Err(RiskReject::MaxOpenOrdersReached { count, limit }) = result {
            assert_eq!(count, 2);
            assert_eq!(limit, 2);
        }
    }

    // -----------------------------------------------------------------------
    // Daily loss
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_daily_loss() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_daily_loss: -1000,
            ..RiskLimits::default()
        });
        checker.update_daily_pnl(-1000);

        let order = make_order(Side::Bid, 1000, 1);
        let result = checker.check_order(&order, None);
        assert!(
            matches!(result, Err(RiskReject::DailyLossLimitHit { .. })),
            "expected DailyLossLimitHit, got {:?}",
            result
        );
        if let Err(RiskReject::DailyLossLimitHit { loss, limit }) = result {
            assert_eq!(loss, -1000);
            assert_eq!(limit, -1000);
        }
    }

    // -----------------------------------------------------------------------
    // Circuit breaker
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_circuit_breaker() {
        let mut checker = default_checker();
        checker.trip_circuit_breaker();

        let order = make_order(Side::Bid, 1000, 1);
        assert_eq!(
            checker.check_order(&order, None),
            Err(RiskReject::CircuitBreakerTripped)
        );

        // Reset should allow orders again.
        checker.reset_circuit_breaker();
        assert!(checker.check_order(&order, None).is_ok());
    }

    // -----------------------------------------------------------------------
    // Daily reset
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_daily() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_daily_loss: -500,
            max_open_orders: 2,
            ..RiskLimits::default()
        });
        checker.update_daily_pnl(-500);
        checker.increment_open_orders();
        checker.increment_open_orders();

        // Both counters should be at their limits.
        let order = make_order(Side::Bid, 1000, 1);
        assert!(checker.check_order(&order, None).is_err());

        // After daily reset, both counters are cleared.
        checker.reset_daily();
        assert!(checker.check_order(&order, None).is_ok());

        // Circuit breaker is NOT cleared by reset_daily.
        checker.trip_circuit_breaker();
        checker.reset_daily();
        assert_eq!(
            checker.check_order(&order, None),
            Err(RiskReject::CircuitBreakerTripped)
        );
    }

    // -------------------------------------------------------------------
    // Order size boundary
    // -------------------------------------------------------------------

    #[test]
    fn test_order_size_at_exact_limit_passes() {
        let checker = default_checker();
        // max_order_size = 100; exactly 100 should pass (<= not <).
        let order = make_order(Side::Bid, 1000, 100);
        assert!(checker.check_order(&order, None).is_ok());
    }

    // -------------------------------------------------------------------
    // Position limit boundary
    // -------------------------------------------------------------------

    #[test]
    fn test_position_at_exact_limit_passes() {
        let checker = default_checker();
        // Current = 900, bid +100 → net 1000 = max_position: should pass.
        let position = make_position(900);
        let order = make_order(Side::Bid, 1000, 100);
        assert!(checker.check_order(&order, Some(&position)).is_ok());
    }

    #[test]
    fn test_position_one_over_limit_rejects() {
        let checker = default_checker();
        // Current = 901, bid +100 → net 1001 > 1000: reject.
        let position = make_position(901);
        let order = make_order(Side::Bid, 1000, 100);
        let result = checker.check_order(&order, Some(&position));
        assert!(matches!(
            result,
            Err(RiskReject::PositionLimitBreached { .. })
        ));
    }

    #[test]
    fn test_bid_reduces_short_position_within_limit() {
        let checker = default_checker();
        // Current short = -950, bid +100 → net = -850; abs = 850 < 1000: pass.
        let position = make_position(-950);
        let order = make_order(Side::Bid, 1000, 100);
        assert!(checker.check_order(&order, Some(&position)).is_ok());
    }

    #[test]
    fn test_ask_reduces_long_position_within_limit() {
        let checker = default_checker();
        // Current long = 950, ask -50 → net = 900; abs = 900 < 1000: pass.
        let position = make_position(950);
        let order = make_order(Side::Ask, 1000, 50);
        assert!(checker.check_order(&order, Some(&position)).is_ok());
    }

    // -------------------------------------------------------------------
    // Notional boundary
    // -------------------------------------------------------------------

    #[test]
    fn test_notional_at_exact_limit_passes() {
        let checker = default_checker();
        // max_notional = 100_000_000; price=1_000_000, qty=100 → notional = 100_000_000: pass.
        let order = make_order(Side::Bid, 1_000_000, 100);
        assert!(checker.check_order(&order, None).is_ok());
    }

    #[test]
    fn test_notional_one_over_limit_rejects() {
        let checker = default_checker();
        // notional = 100_000_001 > 100_000_000: reject.
        let order = make_order(Side::Bid, 100_000_001, 1);
        let result = checker.check_order(&order, None);
        assert!(matches!(result, Err(RiskReject::NotionalExceeded { .. })));
    }

    // -------------------------------------------------------------------
    // Open orders boundary
    // -------------------------------------------------------------------

    #[test]
    fn test_open_orders_one_below_limit_passes() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_open_orders: 3,
            ..RiskLimits::default()
        });
        checker.increment_open_orders();
        checker.increment_open_orders();
        // count=2 < limit=3: pass.
        let order = make_order(Side::Bid, 1000, 1);
        assert!(checker.check_order(&order, None).is_ok());
    }

    // -------------------------------------------------------------------
    // Daily loss boundary
    // -------------------------------------------------------------------

    #[test]
    fn test_daily_loss_just_above_limit_passes() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_daily_loss: -1000,
            ..RiskLimits::default()
        });
        checker.update_daily_pnl(-999);
        // pnl=-999 > max_daily_loss=-1000: pass.
        let order = make_order(Side::Bid, 1000, 1);
        assert!(checker.check_order(&order, None).is_ok());
    }

    #[test]
    fn test_daily_loss_below_limit_rejects() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_daily_loss: -1000,
            ..RiskLimits::default()
        });
        checker.update_daily_pnl(-1001);
        // pnl=-1001 <= -1000: reject.
        let order = make_order(Side::Bid, 1000, 1);
        assert!(matches!(
            checker.check_order(&order, None),
            Err(RiskReject::DailyLossLimitHit { .. })
        ));
    }

    // -------------------------------------------------------------------
    // State management
    // -------------------------------------------------------------------

    #[test]
    fn test_increment_decrement_open_orders() {
        let mut checker = default_checker();
        assert_eq!(checker.open_order_count(), 0);
        checker.increment_open_orders();
        checker.increment_open_orders();
        assert_eq!(checker.open_order_count(), 2);
        checker.decrement_open_orders();
        assert_eq!(checker.open_order_count(), 1);
    }

    #[test]
    fn test_decrement_open_orders_saturates_at_zero() {
        let mut checker = default_checker();
        checker.decrement_open_orders();
        // Should saturate to 0, not underflow.
        assert_eq!(checker.open_order_count(), 0);
    }

    #[test]
    fn test_update_daily_pnl_accumulates() {
        let mut checker = default_checker();
        checker.update_daily_pnl(100);
        checker.update_daily_pnl(-50);
        checker.update_daily_pnl(25);
        assert_eq!(checker.daily_pnl(), 75);
    }

    #[test]
    fn test_is_circuit_breaker_tripped_accessor() {
        let mut checker = default_checker();
        assert!(!checker.is_circuit_breaker_tripped());
        checker.trip_circuit_breaker();
        assert!(checker.is_circuit_breaker_tripped());
        checker.reset_circuit_breaker();
        assert!(!checker.is_circuit_breaker_tripped());
    }

    // -------------------------------------------------------------------
    // Check ordering: circuit breaker has highest priority
    // -------------------------------------------------------------------

    #[test]
    fn test_circuit_breaker_priority_over_other_rejects() {
        let mut checker = PreTradeChecker::new(RiskLimits {
            max_order_size: 1,
            max_daily_loss: 0,
            ..RiskLimits::default()
        });
        // Multiple violations: order too large, daily loss hit, AND circuit breaker.
        checker.update_daily_pnl(-1);
        checker.trip_circuit_breaker();

        let order = make_order(Side::Bid, 1000, 100);
        // Circuit breaker should be returned, not OrderSizeTooLarge.
        assert_eq!(
            checker.check_order(&order, None),
            Err(RiskReject::CircuitBreakerTripped)
        );
    }

    // -------------------------------------------------------------------
    // No position: defaults to zero net
    // -------------------------------------------------------------------

    #[test]
    fn test_no_position_large_bid_within_limit() {
        let checker = default_checker();
        // No position (None) → net=0, bid+100 → net=100, abs < 1000: pass.
        let order = make_order(Side::Bid, 500, 100);
        assert!(checker.check_order(&order, None).is_ok());
    }

    #[test]
    fn test_no_position_large_ask_within_limit() {
        let checker = default_checker();
        // No position, ask-100 → net=-100, abs < 1000: pass.
        let order = make_order(Side::Ask, 500, 100);
        assert!(checker.check_order(&order, None).is_ok());
    }

    // -------------------------------------------------------------------
    // RiskReject equality and debug
    // -------------------------------------------------------------------

    #[test]
    fn test_risk_reject_equality() {
        let a = RiskReject::CircuitBreakerTripped;
        let b = RiskReject::CircuitBreakerTripped;
        assert_eq!(a, b);

        let c = RiskReject::OrderSizeTooLarge { size: 10, limit: 5 };
        let d = RiskReject::OrderSizeTooLarge { size: 10, limit: 5 };
        assert_eq!(c, d);
    }

    #[test]
    fn test_risk_reject_debug_format() {
        let reject = RiskReject::PositionLimitBreached {
            current: 900,
            after: 1100,
            limit: 1000,
        };
        let debug = format!("{:?}", reject);
        assert!(debug.contains("PositionLimitBreached"));
        assert!(debug.contains("900"));
        assert!(debug.contains("1100"));
    }

    #[test]
    fn test_risk_reject_clone() {
        let original = RiskReject::NotionalExceeded {
            notional: 500,
            limit: 100,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
