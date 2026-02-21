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
            initial_margin_bps: 1000,  // 10%
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
    pub fn new(params: MarginParams) -> Self {
        Self { params }
    }

    /// Compute the initial margin required to open a position.
    ///
    /// Formula: `price * quantity * initial_margin_bps / 10000`
    ///
    /// Uses an i128 intermediate to prevent overflow on large values.
    #[inline(always)]
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
        let denominator = (quantity as i128)
            .saturating_mul(self.params.maintenance_margin_bps as i128);
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
}
