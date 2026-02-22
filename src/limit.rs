/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! Per-instrument and per-account risk limit configuration.

// ---------------------------------------------------------------------------
// RiskLimits
// ---------------------------------------------------------------------------

/// Per-instrument and per-account risk limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskLimits {
    /// Maximum net position size (absolute value) in lots.
    pub max_position: u64,
    /// Maximum single order quantity in lots.
    pub max_order_size: u64,
    /// Maximum notional value (price * quantity) in ticks.
    pub max_notional: i64,
    /// Maximum number of open orders.
    pub max_open_orders: u32,
    /// Maximum daily loss (realized + unrealized) before kill switch triggers.
    pub max_daily_loss: i64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position: 1000,
            max_order_size: 100,
            max_notional: 100_000_000,
            max_open_orders: 500,
            max_daily_loss: -500_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let limits = RiskLimits::default();
        assert_eq!(limits.max_position, 1000);
        assert_eq!(limits.max_order_size, 100);
        assert_eq!(limits.max_notional, 100_000_000);
        assert_eq!(limits.max_open_orders, 500);
        assert_eq!(limits.max_daily_loss, -500_000);
    }

    #[test]
    fn test_custom_limits() {
        let limits = RiskLimits {
            max_position: 50,
            max_order_size: 10,
            max_notional: 5_000_000,
            max_open_orders: 20,
            max_daily_loss: -10_000,
        };
        assert_eq!(limits.max_position, 50);
        assert_eq!(limits.max_order_size, 10);
        assert_eq!(limits.max_notional, 5_000_000);
        assert_eq!(limits.max_open_orders, 20);
        assert_eq!(limits.max_daily_loss, -10_000);
    }

    #[test]
    fn test_clone_preserves_all_fields() {
        let original = RiskLimits {
            max_position: 42,
            max_order_size: 7,
            max_notional: 999_999,
            max_open_orders: 3,
            max_daily_loss: -77,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_equality_same_values() {
        let a = RiskLimits::default();
        let b = RiskLimits::default();
        assert_eq!(a, b);
    }

    #[test]
    fn test_inequality_different_position() {
        let a = RiskLimits::default();
        let b = RiskLimits {
            max_position: 999,
            ..RiskLimits::default()
        };
        assert_ne!(a, b);
    }

    #[test]
    fn test_zero_limits() {
        let limits = RiskLimits {
            max_position: 0,
            max_order_size: 0,
            max_notional: 0,
            max_open_orders: 0,
            max_daily_loss: 0,
        };
        assert_eq!(limits.max_position, 0);
        assert_eq!(limits.max_order_size, 0);
        assert_eq!(limits.max_notional, 0);
        assert_eq!(limits.max_open_orders, 0);
        assert_eq!(limits.max_daily_loss, 0);
    }

    #[test]
    fn test_extreme_limits() {
        let limits = RiskLimits {
            max_position: u64::MAX,
            max_order_size: u64::MAX,
            max_notional: i64::MAX,
            max_open_orders: u32::MAX,
            max_daily_loss: i64::MIN,
        };
        assert_eq!(limits.max_position, u64::MAX);
        assert_eq!(limits.max_order_size, u64::MAX);
        assert_eq!(limits.max_notional, i64::MAX);
        assert_eq!(limits.max_open_orders, u32::MAX);
        assert_eq!(limits.max_daily_loss, i64::MIN);
    }

    #[test]
    fn test_debug_format() {
        let limits = RiskLimits::default();
        let debug_str = format!("{:?}", limits);
        assert!(debug_str.contains("RiskLimits"));
        assert!(debug_str.contains("1000"));
    }
}
