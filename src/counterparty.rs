/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! カウンターパーティリスク管理。
//!
//! 取引先別のエクスポージャーを追跡し、
//! 特定の取引先への過度な集中を検出する。

use alloc::collections::BTreeMap;

extern crate alloc;

// ---------------------------------------------------------------------------
// CounterpartyLimits
// ---------------------------------------------------------------------------

/// カウンターパーティリスク制限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterpartyLimits {
    /// 1 取引先あたりの最大エクスポージャー（ticks）。
    pub max_single_exposure: i64,
    /// 全取引先合計の最大エクスポージャー（ticks）。
    pub max_total_exposure: i64,
    /// 集中度上限（basis points）: 1 取引先 / 全体 の比率上限。
    /// 例: 3000 = 30%。
    pub max_concentration_bps: u32,
}

impl Default for CounterpartyLimits {
    fn default() -> Self {
        Self {
            max_single_exposure: 50_000_000,
            max_total_exposure: 200_000_000,
            max_concentration_bps: 3000, // 30%
        }
    }
}

// ---------------------------------------------------------------------------
// CounterpartyReject
// ---------------------------------------------------------------------------

/// カウンターパーティリスク違反。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterpartyReject {
    /// 1 取引先の上限超過。
    SingleExposureExceeded {
        counterparty_id: u64,
        exposure: i64,
        limit: i64,
    },
    /// 全体の上限超過。
    TotalExposureExceeded { total: i64, limit: i64 },
    /// 集中度上限超過。
    ConcentrationExceeded {
        counterparty_id: u64,
        concentration_bps: u32,
        limit_bps: u32,
    },
}

// ---------------------------------------------------------------------------
// CounterpartyTracker
// ---------------------------------------------------------------------------

/// 取引先別エクスポージャー追跡器。
pub struct CounterpartyTracker {
    /// 取引先 ID → エクスポージャー（ticks）。
    exposures: BTreeMap<u64, i64>,
    /// 設定上限。
    limits: CounterpartyLimits,
}

impl CounterpartyTracker {
    /// 新規作成。
    #[must_use]
    pub const fn new(limits: CounterpartyLimits) -> Self {
        Self {
            exposures: BTreeMap::new(),
            limits,
        }
    }

    /// エクスポージャーを追加する。
    ///
    /// 正の値はリスク増加、負の値はリスク減少を示す。
    pub fn add_exposure(&mut self, counterparty_id: u64, amount: i64) {
        let entry = self.exposures.entry(counterparty_id).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// エクスポージャーを直接設定する。
    pub fn set_exposure(&mut self, counterparty_id: u64, amount: i64) {
        self.exposures.insert(counterparty_id, amount);
    }

    /// 指定取引先のエクスポージャーを取得。
    #[must_use]
    pub fn exposure(&self, counterparty_id: u64) -> i64 {
        self.exposures.get(&counterparty_id).copied().unwrap_or(0)
    }

    /// 全取引先の合計エクスポージャー（絶対値合計）。
    #[must_use]
    pub fn total_exposure(&self) -> i64 {
        self.exposures
            .values()
            .map(|v| v.unsigned_abs() as i128)
            .sum::<i128>()
            .min(i64::MAX as i128) as i64
    }

    /// 追跡中の取引先数。
    #[must_use]
    pub fn counterparty_count(&self) -> usize {
        self.exposures.len()
    }

    /// 全リスクチェックを実行。
    ///
    /// # Errors
    ///
    /// いずれかの制限に違反した場合に [`CounterpartyReject`] を返す。
    pub fn check_all(&self) -> Result<(), CounterpartyReject> {
        let total = self.total_exposure();

        // 全体エクスポージャーチェック
        if total > self.limits.max_total_exposure {
            return Err(CounterpartyReject::TotalExposureExceeded {
                total,
                limit: self.limits.max_total_exposure,
            });
        }

        // 個別チェック
        for (&id, &exp) in &self.exposures {
            let abs_exp = exp.unsigned_abs() as i64;

            // 単一取引先上限
            if abs_exp > self.limits.max_single_exposure {
                return Err(CounterpartyReject::SingleExposureExceeded {
                    counterparty_id: id,
                    exposure: exp,
                    limit: self.limits.max_single_exposure,
                });
            }

            // 集中度チェック
            if total > 0 {
                let concentration_bps = ((abs_exp as i128) * 10_000 / (total as i128)) as u32;
                if concentration_bps > self.limits.max_concentration_bps {
                    return Err(CounterpartyReject::ConcentrationExceeded {
                        counterparty_id: id,
                        concentration_bps,
                        limit_bps: self.limits.max_concentration_bps,
                    });
                }
            }
        }

        Ok(())
    }

    /// 指定取引先のエクスポージャーをクリア。
    pub fn clear_counterparty(&mut self, counterparty_id: u64) {
        self.exposures.remove(&counterparty_id);
    }

    /// 全エクスポージャーをクリア。
    pub fn clear_all(&mut self) {
        self.exposures.clear();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracker() -> CounterpartyTracker {
        CounterpartyTracker::new(CounterpartyLimits::default())
    }

    #[test]
    fn empty_tracker_passes() {
        let tracker = default_tracker();
        assert!(tracker.check_all().is_ok());
        assert_eq!(tracker.total_exposure(), 0);
        assert_eq!(tracker.counterparty_count(), 0);
    }

    #[test]
    fn add_exposure_basic() {
        let mut tracker = default_tracker();
        tracker.add_exposure(1, 1_000_000);
        assert_eq!(tracker.exposure(1), 1_000_000);
        tracker.add_exposure(1, 500_000);
        assert_eq!(tracker.exposure(1), 1_500_000);
    }

    #[test]
    fn set_exposure_overrides() {
        let mut tracker = default_tracker();
        tracker.add_exposure(1, 1_000_000);
        tracker.set_exposure(1, 500);
        assert_eq!(tracker.exposure(1), 500);
    }

    #[test]
    fn total_exposure_sums_abs() {
        let mut tracker = default_tracker();
        tracker.set_exposure(1, 1_000_000);
        tracker.set_exposure(2, -500_000);
        // |1_000_000| + |-500_000| = 1_500_000
        assert_eq!(tracker.total_exposure(), 1_500_000);
    }

    #[test]
    fn within_all_limits() {
        let mut tracker = default_tracker();
        // 4 取引先に分散: 各 25%（2500 bps < max 3000 bps）
        tracker.set_exposure(1, 10_000_000);
        tracker.set_exposure(2, 10_000_000);
        tracker.set_exposure(3, 10_000_000);
        tracker.set_exposure(4, 10_000_000);
        assert!(tracker.check_all().is_ok());
    }

    #[test]
    fn single_exposure_exceeded() {
        let mut tracker = default_tracker();
        // max_single = 50_000_000
        tracker.set_exposure(1, 50_000_001);
        let result = tracker.check_all();
        assert!(matches!(
            result,
            Err(CounterpartyReject::SingleExposureExceeded { .. })
        ));
    }

    #[test]
    fn total_exposure_exceeded() {
        let limits = CounterpartyLimits {
            max_single_exposure: i64::MAX,
            max_total_exposure: 100,
            max_concentration_bps: 10_000,
        };
        let mut tracker = CounterpartyTracker::new(limits);
        tracker.set_exposure(1, 60);
        tracker.set_exposure(2, 50);
        // total = 110 > 100
        let result = tracker.check_all();
        assert!(matches!(
            result,
            Err(CounterpartyReject::TotalExposureExceeded { .. })
        ));
    }

    #[test]
    fn concentration_exceeded() {
        let limits = CounterpartyLimits {
            max_single_exposure: i64::MAX,
            max_total_exposure: i64::MAX,
            max_concentration_bps: 5000, // 50%
        };
        let mut tracker = CounterpartyTracker::new(limits);
        // 1 取引先が 60%、もう 1 取引先が 40%
        tracker.set_exposure(1, 60);
        tracker.set_exposure(2, 40);
        // 取引先 1: 60/100 = 60% = 6000 bps > 5000
        let result = tracker.check_all();
        assert!(matches!(
            result,
            Err(CounterpartyReject::ConcentrationExceeded { .. })
        ));
    }

    #[test]
    fn concentration_within_limit() {
        let limits = CounterpartyLimits {
            max_single_exposure: i64::MAX,
            max_total_exposure: i64::MAX,
            max_concentration_bps: 5000, // 50%
        };
        let mut tracker = CounterpartyTracker::new(limits);
        tracker.set_exposure(1, 40);
        tracker.set_exposure(2, 60);
        // 取引先 1: 40/100 = 40% = 4000 bps < 5000 ✓
        // 取引先 2: 60/100 = 60% = 6000 bps > 5000 ✗
        let result = tracker.check_all();
        assert!(matches!(
            result,
            Err(CounterpartyReject::ConcentrationExceeded { .. })
        ));
    }

    #[test]
    fn clear_counterparty() {
        let mut tracker = default_tracker();
        tracker.set_exposure(1, 1_000);
        tracker.set_exposure(2, 2_000);
        tracker.clear_counterparty(1);
        assert_eq!(tracker.exposure(1), 0);
        assert_eq!(tracker.counterparty_count(), 1);
    }

    #[test]
    fn clear_all() {
        let mut tracker = default_tracker();
        tracker.set_exposure(1, 1_000);
        tracker.set_exposure(2, 2_000);
        tracker.clear_all();
        assert_eq!(tracker.counterparty_count(), 0);
        assert_eq!(tracker.total_exposure(), 0);
    }

    #[test]
    fn nonexistent_counterparty_returns_zero() {
        let tracker = default_tracker();
        assert_eq!(tracker.exposure(999), 0);
    }

    #[test]
    fn limits_default_values() {
        let limits = CounterpartyLimits::default();
        assert_eq!(limits.max_single_exposure, 50_000_000);
        assert_eq!(limits.max_total_exposure, 200_000_000);
        assert_eq!(limits.max_concentration_bps, 3000);
    }

    #[test]
    fn limits_clone_eq() {
        let a = CounterpartyLimits::default();
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn reject_clone_eq() {
        let a = CounterpartyReject::TotalExposureExceeded {
            total: 100,
            limit: 50,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn negative_exposure_abs_check() {
        let mut tracker = default_tracker();
        tracker.set_exposure(1, -50_000_001);
        let result = tracker.check_all();
        assert!(matches!(
            result,
            Err(CounterpartyReject::SingleExposureExceeded { .. })
        ));
    }
}
