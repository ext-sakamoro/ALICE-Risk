/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! グリークス（デリバティブ感度指標）のリスクキャップ。
//!
//! ポジション全体の delta/gamma/vega エクスポージャーが
//! 設定上限を超えていないか検証する。

// ---------------------------------------------------------------------------
// GreeksLimits
// ---------------------------------------------------------------------------

/// グリークス上限設定（ticks 単位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreeksLimits {
    /// Delta 絶対値上限。
    pub max_abs_delta: i64,
    /// Gamma 絶対値上限。
    pub max_abs_gamma: i64,
    /// Vega 絶対値上限。
    pub max_abs_vega: i64,
}

impl Default for GreeksLimits {
    fn default() -> Self {
        Self {
            max_abs_delta: 10_000,
            max_abs_gamma: 5_000,
            max_abs_vega: 5_000,
        }
    }
}

// ---------------------------------------------------------------------------
// GreeksExposure
// ---------------------------------------------------------------------------

/// 現在のグリークスエクスポージャー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GreeksExposure {
    /// ポートフォリオ全体の delta（ticks）。
    pub delta: i64,
    /// ポートフォリオ全体の gamma（ticks）。
    pub gamma: i64,
    /// ポートフォリオ全体の vega（ticks）。
    pub vega: i64,
}

impl GreeksExposure {
    /// ゼロエクスポージャー。
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            delta: 0,
            gamma: 0,
            vega: 0,
        }
    }

    /// 別のエクスポージャーを加算。
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        Self {
            delta: self.delta.saturating_add(other.delta),
            gamma: self.gamma.saturating_add(other.gamma),
            vega: self.vega.saturating_add(other.vega),
        }
    }
}

impl Default for GreeksExposure {
    fn default() -> Self {
        Self::zero()
    }
}

// ---------------------------------------------------------------------------
// GreeksReject
// ---------------------------------------------------------------------------

/// グリークスキャップ違反の詳細。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GreeksReject {
    /// Delta 上限超過。
    DeltaExceeded { current: i64, limit: i64 },
    /// Gamma 上限超過。
    GammaExceeded { current: i64, limit: i64 },
    /// Vega 上限超過。
    VegaExceeded { current: i64, limit: i64 },
}

// ---------------------------------------------------------------------------
// check_greeks
// ---------------------------------------------------------------------------

/// グリークスエクスポージャーが上限内か検証する。
///
/// # Errors
///
/// いずれかのグリークスが上限を超過した場合に [`GreeksReject`] を返す。
pub const fn check_greeks(
    exposure: &GreeksExposure,
    limits: &GreeksLimits,
) -> Result<(), GreeksReject> {
    if exposure.delta.abs() > limits.max_abs_delta {
        return Err(GreeksReject::DeltaExceeded {
            current: exposure.delta,
            limit: limits.max_abs_delta,
        });
    }
    if exposure.gamma.abs() > limits.max_abs_gamma {
        return Err(GreeksReject::GammaExceeded {
            current: exposure.gamma,
            limit: limits.max_abs_gamma,
        });
    }
    if exposure.vega.abs() > limits.max_abs_vega {
        return Err(GreeksReject::VegaExceeded {
            current: exposure.vega,
            limit: limits.max_abs_vega,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_limits() {
        let exposure = GreeksExposure {
            delta: 5000,
            gamma: 3000,
            vega: 2000,
        };
        assert!(check_greeks(&exposure, &GreeksLimits::default()).is_ok());
    }

    #[test]
    fn delta_exceeded() {
        let exposure = GreeksExposure {
            delta: 10_001,
            gamma: 0,
            vega: 0,
        };
        let result = check_greeks(&exposure, &GreeksLimits::default());
        assert!(matches!(result, Err(GreeksReject::DeltaExceeded { .. })));
    }

    #[test]
    fn delta_exceeded_negative() {
        let exposure = GreeksExposure {
            delta: -10_001,
            gamma: 0,
            vega: 0,
        };
        let result = check_greeks(&exposure, &GreeksLimits::default());
        assert!(matches!(result, Err(GreeksReject::DeltaExceeded { .. })));
    }

    #[test]
    fn gamma_exceeded() {
        let exposure = GreeksExposure {
            delta: 0,
            gamma: 5_001,
            vega: 0,
        };
        let result = check_greeks(&exposure, &GreeksLimits::default());
        assert!(matches!(result, Err(GreeksReject::GammaExceeded { .. })));
    }

    #[test]
    fn vega_exceeded() {
        let exposure = GreeksExposure {
            delta: 0,
            gamma: 0,
            vega: -5_001,
        };
        let result = check_greeks(&exposure, &GreeksLimits::default());
        assert!(matches!(result, Err(GreeksReject::VegaExceeded { .. })));
    }

    #[test]
    fn at_exact_limit_passes() {
        let exposure = GreeksExposure {
            delta: 10_000,
            gamma: 5_000,
            vega: 5_000,
        };
        assert!(check_greeks(&exposure, &GreeksLimits::default()).is_ok());
    }

    #[test]
    fn zero_exposure_passes() {
        let exposure = GreeksExposure::zero();
        assert!(check_greeks(&exposure, &GreeksLimits::default()).is_ok());
    }

    #[test]
    fn delta_priority_over_gamma() {
        // delta と gamma 両方違反しても delta が先に返る
        let exposure = GreeksExposure {
            delta: 20_000,
            gamma: 20_000,
            vega: 0,
        };
        let result = check_greeks(&exposure, &GreeksLimits::default());
        assert!(matches!(result, Err(GreeksReject::DeltaExceeded { .. })));
    }

    #[test]
    fn add_exposures() {
        let a = GreeksExposure {
            delta: 100,
            gamma: 200,
            vega: 300,
        };
        let b = GreeksExposure {
            delta: -50,
            gamma: 100,
            vega: -100,
        };
        let c = a.add(b);
        assert_eq!(c.delta, 50);
        assert_eq!(c.gamma, 300);
        assert_eq!(c.vega, 200);
    }

    #[test]
    fn custom_limits() {
        let limits = GreeksLimits {
            max_abs_delta: 100,
            max_abs_gamma: 50,
            max_abs_vega: 50,
        };
        let exposure = GreeksExposure {
            delta: 101,
            gamma: 0,
            vega: 0,
        };
        assert!(check_greeks(&exposure, &limits).is_err());
    }

    #[test]
    fn limits_default_values() {
        let limits = GreeksLimits::default();
        assert_eq!(limits.max_abs_delta, 10_000);
        assert_eq!(limits.max_abs_gamma, 5_000);
        assert_eq!(limits.max_abs_vega, 5_000);
    }

    #[test]
    fn limits_clone_eq() {
        let a = GreeksLimits::default();
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn exposure_default() {
        let e = GreeksExposure::default();
        assert_eq!(e, GreeksExposure::zero());
    }

    #[test]
    fn reject_eq() {
        let a = GreeksReject::DeltaExceeded {
            current: 100,
            limit: 50,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn exposure_add_saturates() {
        let a = GreeksExposure {
            delta: i64::MAX,
            gamma: 0,
            vega: 0,
        };
        let b = GreeksExposure {
            delta: 1,
            gamma: 0,
            vega: 0,
        };
        let c = a.add(b);
        assert_eq!(c.delta, i64::MAX);
    }
}
