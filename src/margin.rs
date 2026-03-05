/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! Margin requirement calculation for leveraged positions.
//!
//! All monetary values are expressed in the same tick unit used throughout
//! ALICE-Ledger.  Integer arithmetic with i128 intermediates is used to
//! prevent overflow when multiplying large prices by large quantities.

// Reciprocal constant retained for documentation purposes; actual integer
// division uses the i128 path below.
#[allow(dead_code)]
const RCP_BPS: f64 = 1.0 / 10000.0;

// ---------------------------------------------------------------------------
// MarginParams
// ---------------------------------------------------------------------------

/// Margin rate configuration expressed in basis points (bps).
///
/// One basis point equals 0.01%, so 1000 bps = 10%.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarginParams {
    /// Initial margin rate in basis points (e.g., 1000 = 10%).
    pub initial_margin_bps: u32,
    /// Maintenance margin rate in basis points (e.g., 500 = 5%).
    pub maintenance_margin_bps: u32,
}

impl Default for MarginParams {
    fn default() -> Self {
        Self {
            initial_margin_bps: 1000,    // 10%
            maintenance_margin_bps: 500, // 5%
        }
    }
}

// ---------------------------------------------------------------------------
// MarginCalculator
// ---------------------------------------------------------------------------

/// Computes initial and maintenance margin requirements.
pub struct MarginCalculator {
    params: MarginParams,
}

impl MarginCalculator {
    /// Create a new margin calculator with the given parameters.
    #[inline(always)]
    #[must_use]
    pub const fn new(params: MarginParams) -> Self {
        Self { params }
    }

    /// Compute the initial margin required to open a position.
    ///
    /// Formula: `price * quantity * initial_margin_bps / 10000`
    ///
    /// Uses an i128 intermediate to prevent overflow on large values.
    #[inline(always)]
    #[must_use]
    pub fn initial_margin(&self, price: i64, quantity: u64) -> i64 {
        let numerator = (price as i128)
            .saturating_mul(quantity as i128)
            .saturating_mul(self.params.initial_margin_bps as i128);
        (numerator / 10_000).min(i64::MAX as i128) as i64
    }

    /// Compute the maintenance margin required to hold an open position.
    ///
    /// Formula: `price * quantity * maintenance_margin_bps / 10000`
    ///
    /// Uses an i128 intermediate to prevent overflow on large values.
    #[inline(always)]
    #[must_use]
    pub fn maintenance_margin(&self, price: i64, quantity: u64) -> i64 {
        let numerator = (price as i128)
            .saturating_mul(quantity as i128)
            .saturating_mul(self.params.maintenance_margin_bps as i128);
        (numerator / 10_000).min(i64::MAX as i128) as i64
    }

    /// Return `true` when `account_equity` is below the maintenance margin.
    ///
    /// A margin call is triggered when the account can no longer sustain the
    /// current position at the prevailing mark price.
    #[inline(always)]
    #[must_use]
    pub fn is_margin_call(&self, price: i64, position_qty: u64, account_equity: i64) -> bool {
        account_equity < self.maintenance_margin(price, position_qty)
    }

    /// Compute the mark price at which a margin call would be triggered.
    ///
    /// Solves `equity = qty * maint_bps / 10000 * liq_price` for `liq_price`.
    ///
    /// - For a **long** position the account loses value as price falls, so:
    ///   `liq_price = entry_price - (equity / (qty * maint_bps / 10000))`
    /// - For a **short** position the account loses value as price rises, so:
    ///   `liq_price = entry_price + (equity / (qty * maint_bps / 10000))`
    ///
    /// If `quantity` is zero, `entry_price` is returned unchanged.
    #[inline(always)]
    #[must_use]
    pub fn liquidation_price(
        &self,
        entry_price: i64,
        quantity: u64,
        equity: i64,
        is_long: bool,
    ) -> i64 {
        if quantity == 0 {
            return entry_price;
        }
        // margin_per_lot = maint_bps / 10000 (applied as integer division)
        // distance = equity / (quantity * margin_per_lot)
        //          = equity * 10000 / (quantity * maint_bps)
        let denominator =
            (quantity as i128).saturating_mul(self.params.maintenance_margin_bps as i128);
        if denominator == 0 {
            return entry_price;
        }
        let distance = ((equity as i128).saturating_mul(10_000)) / denominator;
        let distance_i64 = distance.min(i64::MAX as i128).max(i64::MIN as i128) as i64;
        if is_long {
            entry_price.saturating_sub(distance_i64)
        } else {
            entry_price.saturating_add(distance_i64)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_calc() -> MarginCalculator {
        MarginCalculator::new(MarginParams::default())
    }

    // -----------------------------------------------------------------------
    // Initial margin
    // -----------------------------------------------------------------------

    #[test]
    fn test_initial_margin() {
        let calc = default_calc();
        // price=10_000, qty=10, bps=1000 → 10_000 * 10 * 1000 / 10000 = 10_000
        assert_eq!(calc.initial_margin(10_000, 10), 10_000);
    }

    #[test]
    fn test_initial_margin_zero_quantity() {
        let calc = default_calc();
        assert_eq!(calc.initial_margin(50_000, 0), 0);
    }

    #[test]
    fn test_initial_margin_zero_price() {
        let calc = default_calc();
        assert_eq!(calc.initial_margin(0, 100), 0);
    }

    // -----------------------------------------------------------------------
    // Maintenance margin
    // -----------------------------------------------------------------------

    #[test]
    fn test_maintenance_margin() {
        let calc = default_calc();
        // price=10_000, qty=10, bps=500 → 10_000 * 10 * 500 / 10000 = 5_000
        assert_eq!(calc.maintenance_margin(10_000, 10), 5_000);
    }

    #[test]
    fn test_maintenance_is_less_than_initial() {
        let calc = default_calc();
        let price = 20_000;
        let qty = 5;
        assert!(calc.maintenance_margin(price, qty) < calc.initial_margin(price, qty));
    }

    // -----------------------------------------------------------------------
    // Margin call
    // -----------------------------------------------------------------------

    #[test]
    fn test_margin_call_true() {
        let calc = default_calc();
        // maintenance_margin(10_000, 10) = 5_000; equity=4_999 triggers call.
        assert!(calc.is_margin_call(10_000, 10, 4_999));
    }

    #[test]
    fn test_margin_call_false() {
        let calc = default_calc();
        // equity exactly at maintenance: no margin call (< not <=).
        assert!(!calc.is_margin_call(10_000, 10, 5_000));
    }

    #[test]
    fn test_margin_call_above_maintenance() {
        let calc = default_calc();
        assert!(!calc.is_margin_call(10_000, 10, 10_000));
    }

    // -----------------------------------------------------------------------
    // Liquidation price
    // -----------------------------------------------------------------------

    #[test]
    fn test_liquidation_price_long() {
        let calc = default_calc();
        // entry=10_000, qty=10, equity=5_000, maint_bps=500
        // distance = 5_000 * 10_000 / (10 * 500) = 50_000_000 / 5_000 = 10_000
        // liq = 10_000 - 10_000 = 0
        let liq = calc.liquidation_price(10_000, 10, 5_000, true);
        assert_eq!(liq, 0);
    }

    #[test]
    fn test_liquidation_price_short() {
        let calc = default_calc();
        // entry=10_000, qty=10, equity=5_000, maint_bps=500
        // distance = 10_000; liq = 10_000 + 10_000 = 20_000
        let liq = calc.liquidation_price(10_000, 10, 5_000, false);
        assert_eq!(liq, 20_000);
    }

    #[test]
    fn test_liquidation_price_zero_quantity() {
        let calc = default_calc();
        // Should return entry_price unchanged.
        let liq = calc.liquidation_price(10_000, 0, 5_000, true);
        assert_eq!(liq, 10_000);
    }

    // -------------------------------------------------------------------
    // MarginParams tests
    // -------------------------------------------------------------------

    #[test]
    fn test_margin_params_default() {
        let params = MarginParams::default();
        assert_eq!(params.initial_margin_bps, 1000);
        assert_eq!(params.maintenance_margin_bps, 500);
    }

    #[test]
    fn test_margin_params_custom() {
        let params = MarginParams {
            initial_margin_bps: 2000,
            maintenance_margin_bps: 1000,
        };
        assert_eq!(params.initial_margin_bps, 2000);
        assert_eq!(params.maintenance_margin_bps, 1000);
    }

    #[test]
    fn test_margin_params_clone_eq() {
        let a = MarginParams::default();
        let b = a.clone();
        assert_eq!(a, b);
    }

    // -------------------------------------------------------------------
    // Initial margin edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_initial_margin_large_values() {
        let calc = default_calc();
        // price=1_000_000_000, qty=1_000_000, bps=1000
        // = 1e9 * 1e6 * 1000 / 10000 = 1e14
        let result = calc.initial_margin(1_000_000_000, 1_000_000);
        assert_eq!(result, 100_000_000_000_000);
    }

    #[test]
    fn test_initial_margin_negative_price() {
        let calc = default_calc();
        // Negative price (spreads/differences can be negative).
        // -10_000 * 10 * 1000 / 10000 = -10_000
        let result = calc.initial_margin(-10_000, 10);
        assert_eq!(result, -10_000);
    }

    #[test]
    fn test_initial_margin_unit_values() {
        let calc = default_calc();
        // price=1, qty=1, bps=1000 → 1*1*1000/10000 = 0 (integer division)
        assert_eq!(calc.initial_margin(1, 1), 0);
    }

    #[test]
    fn test_initial_margin_bps_10000_means_100_percent() {
        let calc = MarginCalculator::new(MarginParams {
            initial_margin_bps: 10_000, // 100%
            maintenance_margin_bps: 500,
        });
        // 100% of notional: price * qty
        assert_eq!(calc.initial_margin(5000, 10), 50_000);
    }

    // -------------------------------------------------------------------
    // Maintenance margin edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_maintenance_margin_zero_bps() {
        let calc = MarginCalculator::new(MarginParams {
            initial_margin_bps: 1000,
            maintenance_margin_bps: 0,
        });
        // 0 bps means zero maintenance margin.
        assert_eq!(calc.maintenance_margin(50_000, 100), 0);
    }

    #[test]
    fn test_maintenance_margin_large_values() {
        let calc = default_calc();
        // price=1_000_000_000, qty=1_000_000, bps=500
        // = 1e9 * 1e6 * 500 / 10000 = 5e13
        assert_eq!(
            calc.maintenance_margin(1_000_000_000, 1_000_000),
            50_000_000_000_000
        );
    }

    // -------------------------------------------------------------------
    // Margin call edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_margin_call_negative_equity() {
        let calc = default_calc();
        // Negative equity should always trigger margin call.
        assert!(calc.is_margin_call(10_000, 10, -1));
    }

    #[test]
    fn test_margin_call_zero_position() {
        let calc = default_calc();
        // Zero position → maintenance margin = 0 → equity 0 is not < 0.
        assert!(!calc.is_margin_call(10_000, 0, 0));
    }

    #[test]
    fn test_margin_call_zero_price() {
        let calc = default_calc();
        // Zero price → maintenance margin = 0 → equity 0 is not < 0.
        assert!(!calc.is_margin_call(0, 100, 0));
    }

    // -------------------------------------------------------------------
    // Liquidation price edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_liquidation_price_zero_equity_long() {
        let calc = default_calc();
        // Zero equity → distance = 0 → liq_price = entry_price.
        let liq = calc.liquidation_price(10_000, 10, 0, true);
        assert_eq!(liq, 10_000);
    }

    #[test]
    fn test_liquidation_price_zero_equity_short() {
        let calc = default_calc();
        let liq = calc.liquidation_price(10_000, 10, 0, false);
        assert_eq!(liq, 10_000);
    }

    #[test]
    fn test_liquidation_price_negative_equity_long() {
        let calc = default_calc();
        // Negative equity → negative distance → liq_price is ABOVE entry for long.
        // distance = -5000 * 10000 / (10 * 500) = -50_000_000 / 5000 = -10_000
        // liq = 10_000 - (-10_000) = 20_000
        let liq = calc.liquidation_price(10_000, 10, -5_000, true);
        assert_eq!(liq, 20_000);
    }

    #[test]
    fn test_liquidation_price_negative_equity_short() {
        let calc = default_calc();
        // distance = -10_000; liq = 10_000 + (-10_000) = 0
        let liq = calc.liquidation_price(10_000, 10, -5_000, false);
        assert_eq!(liq, 0);
    }

    #[test]
    fn test_liquidation_price_zero_maint_bps() {
        // maintenance_margin_bps = 0 → denominator = qty * 0 = 0 → return entry_price.
        let calc = MarginCalculator::new(MarginParams {
            initial_margin_bps: 1000,
            maintenance_margin_bps: 0,
        });
        let liq = calc.liquidation_price(10_000, 10, 5_000, true);
        assert_eq!(liq, 10_000);
    }

    #[test]
    fn test_liquidation_price_symmetry() {
        let calc = default_calc();
        // For same equity, the distance from entry should be identical for long/short,
        // but in opposite directions.
        let entry = 50_000;
        let qty = 20;
        let equity = 10_000;
        let liq_long = calc.liquidation_price(entry, qty, equity, true);
        let liq_short = calc.liquidation_price(entry, qty, equity, false);
        // liq_long = entry - distance, liq_short = entry + distance
        // distance should be equal in magnitude:
        assert_eq!(entry - liq_long, liq_short - entry);
    }

    // -------------------------------------------------------------------
    // Custom bps: initial_margin with non-default rate
    // -------------------------------------------------------------------

    #[test]
    fn test_initial_margin_custom_bps() {
        // 500 bps = 5%; price=20_000, qty=4 → 20_000 * 4 * 500 / 10_000 = 4_000
        let calc = MarginCalculator::new(MarginParams {
            initial_margin_bps: 500,
            maintenance_margin_bps: 250,
        });
        assert_eq!(calc.initial_margin(20_000, 4), 4_000);
    }

    // -------------------------------------------------------------------
    // Maintenance margin rounds to zero for tiny qty (integer division)
    // -------------------------------------------------------------------

    #[test]
    fn test_maintenance_margin_rounds_to_zero_for_unit_qty() {
        // price=1, qty=1, bps=500 → 1 * 1 * 500 / 10_000 = 0 (integer truncation)
        let calc = default_calc();
        assert_eq!(calc.maintenance_margin(1, 1), 0);
    }

    // -------------------------------------------------------------------
    // Liquidation price: large equity pushes distance past entry → saturates
    // -------------------------------------------------------------------

    #[test]
    fn test_liquidation_price_long_large_equity_saturates() {
        let calc = default_calc();
        // equity so large that distance > entry_price → liq_price saturates at
        // entry_price - distance which may go negative (i64 saturating_sub).
        // entry=100, qty=1, equity=10_000_000, maint_bps=500
        // distance = 10_000_000 * 10_000 / (1 * 500) = 200_000_000
        // liq = 100 - 200_000_000 = -199_999_900
        let liq = calc.liquidation_price(100, 1, 10_000_000, true);
        assert_eq!(liq, 100 - 200_000_000);
    }

    // -------------------------------------------------------------------
    // is_margin_call: zero-bps maintenance, large equity — never triggers
    // -------------------------------------------------------------------

    #[test]
    fn test_margin_call_zero_maint_bps_never_triggers() {
        // maintenance margin is always 0 when bps=0, so equity >= 0 never fires.
        let calc = MarginCalculator::new(MarginParams {
            initial_margin_bps: 1000,
            maintenance_margin_bps: 0,
        });
        assert!(!calc.is_margin_call(50_000, 1_000, 0));
        assert!(!calc.is_margin_call(50_000, 1_000, i64::MAX));
    }

    // -------------------------------------------------------------------
    // Property-based tests
    // -------------------------------------------------------------------

    use proptest::prelude::*;

    proptest! {
        /// For any non-negative price and any quantity, initial_margin and
        /// maintenance_margin must both be non-negative.
        #[test]
        fn prop_margin_non_negative(
            price in 0i64..i64::MAX,
            quantity in 0u64..u64::MAX,
        ) {
            let calc = default_calc();
            prop_assert!(calc.initial_margin(price, quantity) >= 0);
            prop_assert!(calc.maintenance_margin(price, quantity) >= 0);
        }

        /// When maintenance_bps <= initial_bps, maintenance_margin must be
        /// <= initial_margin for any price and quantity.
        #[test]
        fn prop_maintenance_le_initial(
            maintenance_bps in 0u32..10_000u32,
            extra_bps in 0u32..10_000u32,
            price in 0i64..1_000_000i64,
            quantity in 0u64..1_000_000u64,
        ) {
            let initial_bps = maintenance_bps.saturating_add(extra_bps);
            let calc = MarginCalculator::new(MarginParams {
                initial_margin_bps: initial_bps,
                maintenance_margin_bps: maintenance_bps,
            });
            prop_assert!(
                calc.maintenance_margin(price, quantity) <= calc.initial_margin(price, quantity)
            );
        }

        /// is_margin_call returns true if and only if account_equity is strictly
        /// less than maintenance_margin(price, position_qty).
        #[test]
        fn prop_margin_call_consistency(
            price in 0i64..1_000_000i64,
            position_qty in 0u64..1_000_000u64,
            account_equity in i64::MIN..i64::MAX,
        ) {
            let calc = default_calc();
            let maint = calc.maintenance_margin(price, position_qty);
            let expected = account_equity < maint;
            prop_assert_eq!(
                calc.is_margin_call(price, position_qty, account_equity),
                expected
            );
        }
    }
}
