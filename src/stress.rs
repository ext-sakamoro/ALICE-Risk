/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! ストレステストシナリオ。
//!
//! 仮想的な市場変動シナリオをポートフォリオに適用し、
//! 想定損益を算出する。

// ---------------------------------------------------------------------------
// StressScenario
// ---------------------------------------------------------------------------

/// ストレステストシナリオ定義。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StressScenario {
    /// シナリオ名。
    pub name: &'static str,
    /// 価格変動（ticks）。正 = 上昇、負 = 下落。
    pub price_shock: i64,
    /// ボラティリティ変動（basis points）。正 = 上昇。
    pub vol_shock_bps: i32,
}

impl StressScenario {
    /// 新規シナリオ作成。
    #[must_use]
    pub const fn new(name: &'static str, price_shock: i64, vol_shock_bps: i32) -> Self {
        Self {
            name,
            price_shock,
            vol_shock_bps,
        }
    }

    /// 2008 年金融危機シナリオ。
    #[must_use]
    pub const fn crisis_2008() -> Self {
        Self::new("Crisis 2008", -5000, 10_000) // -50 ticks, vol +100%
    }

    /// フラッシュクラッシュシナリオ。
    #[must_use]
    pub const fn flash_crash() -> Self {
        Self::new("Flash Crash", -10_000, 20_000) // -100 ticks, vol +200%
    }

    /// 緩やかな上昇シナリオ。
    #[must_use]
    pub const fn gradual_rally() -> Self {
        Self::new("Gradual Rally", 3000, -2000) // +30 ticks, vol -20%
    }
}

// ---------------------------------------------------------------------------
// PortfolioPosition
// ---------------------------------------------------------------------------

/// ストレステスト用ポジション。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StressPosition {
    /// ポジション量（正 = ロング、負 = ショート）。
    pub quantity: i64,
    /// 現在価格（ticks）。
    pub current_price: i64,
    /// Vega 感度（1 basis point あたりの損益変動、ticks）。
    pub vega_per_bp: i64,
}

// ---------------------------------------------------------------------------
// StressResult
// ---------------------------------------------------------------------------

/// ストレステスト結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StressResult {
    /// 適用されたシナリオ名。
    pub scenario_name: &'static str,
    /// 価格変動による損益（ticks）。
    pub price_pnl: i64,
    /// ボラティリティ変動による損益（ticks）。
    pub vol_pnl: i64,
    /// 合計損益（ticks）。
    pub total_pnl: i64,
}

// ---------------------------------------------------------------------------
// apply_scenario
// ---------------------------------------------------------------------------

/// シナリオをポジションに適用し、想定損益を算出する。
///
/// - 価格損益: `quantity * price_shock`
/// - ボラ損益: `vega_per_bp * vol_shock_bps`
#[must_use]
pub fn apply_scenario(position: &StressPosition, scenario: &StressScenario) -> StressResult {
    let price_pnl = (position.quantity as i128)
        .saturating_mul(scenario.price_shock as i128)
        .min(i64::MAX as i128)
        .max(i64::MIN as i128) as i64;

    let vol_pnl = (position.vega_per_bp as i128)
        .saturating_mul(scenario.vol_shock_bps as i128)
        .min(i64::MAX as i128)
        .max(i64::MIN as i128) as i64;

    let total_pnl = price_pnl.saturating_add(vol_pnl);

    StressResult {
        scenario_name: scenario.name,
        price_pnl,
        vol_pnl,
        total_pnl,
    }
}

/// 複数ポジションのポートフォリオに対してシナリオを適用する。
#[must_use]
pub fn apply_scenario_portfolio(
    positions: &[StressPosition],
    scenario: &StressScenario,
) -> StressResult {
    let mut total_price_pnl: i128 = 0;
    let mut total_vol_pnl: i128 = 0;

    for pos in positions {
        total_price_pnl = total_price_pnl
            .saturating_add((pos.quantity as i128).saturating_mul(scenario.price_shock as i128));
        total_vol_pnl = total_vol_pnl.saturating_add(
            (pos.vega_per_bp as i128).saturating_mul(scenario.vol_shock_bps as i128),
        );
    }

    let price_pnl = total_price_pnl.min(i64::MAX as i128).max(i64::MIN as i128) as i64;
    let vol_pnl = total_vol_pnl.min(i64::MAX as i128).max(i64::MIN as i128) as i64;

    StressResult {
        scenario_name: scenario.name,
        price_pnl,
        vol_pnl,
        total_pnl: price_pnl.saturating_add(vol_pnl),
    }
}

/// 複数シナリオを一括でポートフォリオに適用する。
#[must_use]
pub fn stress_test_portfolio(
    positions: &[StressPosition],
    scenarios: &[StressScenario],
) -> Vec<StressResult> {
    scenarios
        .iter()
        .map(|s| apply_scenario_portfolio(positions, s))
        .collect()
}

/// 最悪シナリオの損失額を返す。
#[must_use]
pub fn worst_case_loss(results: &[StressResult]) -> Option<i64> {
    results.iter().map(|r| r.total_pnl).min()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn long_position() -> StressPosition {
        StressPosition {
            quantity: 100,
            current_price: 50_000,
            vega_per_bp: 10,
        }
    }

    fn short_position() -> StressPosition {
        StressPosition {
            quantity: -50,
            current_price: 50_000,
            vega_per_bp: 5,
        }
    }

    #[test]
    fn apply_crisis_to_long() {
        let pos = long_position();
        let result = apply_scenario(&pos, &StressScenario::crisis_2008());
        // price_pnl = 100 * -5000 = -500_000
        assert_eq!(result.price_pnl, -500_000);
        // vol_pnl = 10 * 10_000 = 100_000
        assert_eq!(result.vol_pnl, 100_000);
        assert_eq!(result.total_pnl, -400_000);
    }

    #[test]
    fn apply_crisis_to_short() {
        let pos = short_position();
        let result = apply_scenario(&pos, &StressScenario::crisis_2008());
        // price_pnl = -50 * -5000 = 250_000
        assert_eq!(result.price_pnl, 250_000);
        // vol_pnl = 5 * 10_000 = 50_000
        assert_eq!(result.vol_pnl, 50_000);
        assert_eq!(result.total_pnl, 300_000);
    }

    #[test]
    fn apply_flash_crash() {
        let pos = long_position();
        let result = apply_scenario(&pos, &StressScenario::flash_crash());
        assert_eq!(result.price_pnl, -1_000_000);
        assert_eq!(result.vol_pnl, 200_000);
        assert_eq!(result.total_pnl, -800_000);
    }

    #[test]
    fn apply_gradual_rally() {
        let pos = long_position();
        let result = apply_scenario(&pos, &StressScenario::gradual_rally());
        assert_eq!(result.price_pnl, 300_000);
        assert_eq!(result.vol_pnl, -20_000);
        assert_eq!(result.total_pnl, 280_000);
    }

    #[test]
    fn portfolio_two_positions() {
        let positions = vec![long_position(), short_position()];
        let result = apply_scenario_portfolio(&positions, &StressScenario::crisis_2008());
        // price: 100*(-5000) + (-50)*(-5000) = -500_000 + 250_000 = -250_000
        assert_eq!(result.price_pnl, -250_000);
        // vol: 10*10_000 + 5*10_000 = 100_000 + 50_000 = 150_000
        assert_eq!(result.vol_pnl, 150_000);
        assert_eq!(result.total_pnl, -100_000);
    }

    #[test]
    fn portfolio_empty() {
        let result = apply_scenario_portfolio(&[], &StressScenario::crisis_2008());
        assert_eq!(result.price_pnl, 0);
        assert_eq!(result.vol_pnl, 0);
        assert_eq!(result.total_pnl, 0);
    }

    #[test]
    fn stress_test_multiple_scenarios() {
        let positions = vec![long_position()];
        let scenarios = vec![
            StressScenario::crisis_2008(),
            StressScenario::flash_crash(),
            StressScenario::gradual_rally(),
        ];
        let results = stress_test_portfolio(&positions, &scenarios);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].scenario_name, "Crisis 2008");
        assert_eq!(results[1].scenario_name, "Flash Crash");
        assert_eq!(results[2].scenario_name, "Gradual Rally");
    }

    #[test]
    fn worst_case_loss_basic() {
        let positions = vec![long_position()];
        let scenarios = vec![
            StressScenario::crisis_2008(),
            StressScenario::flash_crash(),
            StressScenario::gradual_rally(),
        ];
        let results = stress_test_portfolio(&positions, &scenarios);
        let worst = worst_case_loss(&results).unwrap();
        // Flash crash gives the worst loss
        assert_eq!(worst, -800_000);
    }

    #[test]
    fn worst_case_loss_empty() {
        assert!(worst_case_loss(&[]).is_none());
    }

    #[test]
    fn custom_scenario() {
        let scenario = StressScenario::new("Custom", -1000, 500);
        assert_eq!(scenario.name, "Custom");
        assert_eq!(scenario.price_shock, -1000);
        assert_eq!(scenario.vol_shock_bps, 500);
    }

    #[test]
    fn scenario_clone_eq() {
        let a = StressScenario::crisis_2008();
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn result_clone_eq() {
        let result = StressResult {
            scenario_name: "Test",
            price_pnl: -100,
            vol_pnl: 50,
            total_pnl: -50,
        };
        let cloned = result.clone();
        assert_eq!(result, cloned);
    }

    #[test]
    fn zero_position() {
        let pos = StressPosition {
            quantity: 0,
            current_price: 50_000,
            vega_per_bp: 0,
        };
        let result = apply_scenario(&pos, &StressScenario::flash_crash());
        assert_eq!(result.total_pnl, 0);
    }

    #[test]
    fn scenario_no_vol_shock() {
        let scenario = StressScenario::new("Price Only", -2000, 0);
        let pos = long_position();
        let result = apply_scenario(&pos, &scenario);
        assert_eq!(result.vol_pnl, 0);
        assert_eq!(result.total_pnl, result.price_pnl);
    }

    #[test]
    fn position_clone_eq() {
        let a = long_position();
        let b = a.clone();
        assert_eq!(a, b);
    }
}
