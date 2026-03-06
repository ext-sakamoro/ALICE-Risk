/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! Value-at-Risk (`VaR`) 計算。
//!
//! ポートフォリオレベルのリスク制限に使用する。
//! ヒストリカル法とパラメトリック法（正規分布仮定）の 2 手法を提供。

// ---------------------------------------------------------------------------
// HistoricalVaR
// ---------------------------------------------------------------------------

/// ヒストリカル `VaR` 計算器。
///
/// 過去リターン分布のパーセンタイルを直接参照して `VaR` を算出する。
/// 分布の仮定を置かないため、テールリスクをより正確に反映できる。
pub struct HistoricalVaR {
    /// 過去リターン（ticks 単位）。
    returns: Vec<i64>,
    /// ソート済みフラグ。
    sorted: bool,
}

impl HistoricalVaR {
    /// 新規作成。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            returns: Vec::new(),
            sorted: false,
        }
    }

    /// 指定キャパシティで作成。
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            returns: Vec::with_capacity(capacity),
            sorted: false,
        }
    }

    /// リターン値を追加。
    pub fn add_return(&mut self, r: i64) {
        self.returns.push(r);
        self.sorted = false;
    }

    /// 複数リターンを一括追加。
    pub fn add_returns(&mut self, rs: &[i64]) {
        self.returns.extend_from_slice(rs);
        self.sorted = false;
    }

    /// サンプル数。
    #[must_use]
    pub const fn count(&self) -> usize {
        self.returns.len()
    }

    /// 指定信頼水準での `VaR` を計算する。
    ///
    /// `confidence` は 0.0〜1.0（例: 0.95 = 95% `VaR`）。
    /// 左テール（損失側）のパーセンタイル値を返す。
    /// 正の値は損失額を意味する（符号反転）。
    ///
    /// サンプル数が不足している場合は `None`。
    #[must_use]
    pub fn var_at_confidence(&mut self, confidence: f64) -> Option<i64> {
        if self.returns.is_empty() || !(0.0..=1.0).contains(&confidence) {
            return None;
        }

        if !self.sorted {
            self.returns.sort_unstable();
            self.sorted = true;
        }

        // 左テールのインデックス: (1 - confidence) * n
        let n = self.returns.len();
        let idx = ((1.0 - confidence) * n as f64).floor() as usize;
        let idx = idx.min(n - 1);

        // 損失を正の値で返す
        Some(-self.returns[idx])
    }

    /// 全リターンをクリア。
    pub fn clear(&mut self) {
        self.returns.clear();
        self.sorted = false;
    }
}

impl Default for HistoricalVaR {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ParametricVaR
// ---------------------------------------------------------------------------

/// パラメトリック VaR（正規分布仮定）。
///
/// 平均と標準偏差から分析的に `VaR` を算出する。
/// 計算が軽いが、テールリスクを過小評価する傾向がある。
pub struct ParametricVaR {
    /// リターンの合計。
    sum: i128,
    /// リターンの二乗合計。
    sum_sq: i128,
    /// サンプル数。
    n: u64,
}

impl ParametricVaR {
    /// 新規作成。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sum: 0,
            sum_sq: 0,
            n: 0,
        }
    }

    /// リターン値を追加。
    pub const fn add_return(&mut self, r: i64) {
        self.sum += r as i128;
        self.sum_sq += (r as i128) * (r as i128);
        self.n += 1;
    }

    /// サンプル数。
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.n
    }

    /// 平均リターン（ticks）。
    #[must_use]
    pub fn mean(&self) -> Option<f64> {
        if self.n == 0 {
            return None;
        }
        Some(self.sum as f64 / self.n as f64)
    }

    /// 標準偏差。
    #[must_use]
    pub fn std_dev(&self) -> Option<f64> {
        if self.n < 2 {
            return None;
        }
        let mean = self.sum as f64 / self.n as f64;
        let variance = (-mean).mul_add(mean, self.sum_sq as f64 / self.n as f64);
        if variance < 0.0 {
            return Some(0.0);
        }
        Some(variance.sqrt())
    }

    /// 指定信頼水準での VaR（正規分布仮定）。
    ///
    /// `VaR = -(mean - z * sigma)` ここで z は信頼水準に対応する z スコア。
    /// 正の値は損失額を意味する。
    #[must_use]
    pub fn var_at_confidence(&self, confidence: f64) -> Option<f64> {
        let mean = self.mean()?;
        let sigma = self.std_dev()?;
        let z = z_score(confidence)?;
        // VaR = -(mean - z * sigma) = z * sigma - mean
        Some(z.mul_add(sigma, -mean))
    }

    /// リセット。
    pub const fn clear(&mut self) {
        self.sum = 0;
        self.sum_sq = 0;
        self.n = 0;
    }
}

impl Default for ParametricVaR {
    fn default() -> Self {
        Self::new()
    }
}

/// 信頼水準から正規分布の z スコアを近似計算する。
///
/// Beasley-Springer-Moro 近似の簡略版。
/// 一般的な信頼水準 (90%, 95%, 99%) で十分な精度を持つ。
#[must_use]
fn z_score(confidence: f64) -> Option<f64> {
    if !(0.5..1.0).contains(&confidence) {
        return None;
    }
    // Rational approximation (Abramowitz & Stegun 26.2.23)
    let p = 1.0 - confidence;
    let t = (-2.0 * p.ln()).sqrt();
    let c0: f64 = 2.515_517;
    let c1: f64 = 0.802_853;
    let c2: f64 = 0.010_328;
    let d1: f64 = 1.432_788;
    let d2: f64 = 0.189_269;
    let d3: f64 = 0.001_308;
    let numerator = c2.mul_add(t, c1).mul_add(t, c0);
    let denominator = d3.mul_add(t, d2).mul_add(t, d1).mul_add(t, 1.0);
    let z = t - numerator / denominator;
    Some(z)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // HistoricalVaR
    // -----------------------------------------------------------------------

    #[test]
    fn historical_var_empty() {
        let mut var = HistoricalVaR::new();
        assert!(var.var_at_confidence(0.95).is_none());
        assert_eq!(var.count(), 0);
    }

    #[test]
    fn historical_var_single_return() {
        let mut var = HistoricalVaR::new();
        var.add_return(-100);
        // 95% VaR: idx = floor(0.05 * 1) = 0 → returns[0] = -100 → VaR = 100
        let v = var.var_at_confidence(0.95).unwrap();
        assert_eq!(v, 100);
    }

    #[test]
    fn historical_var_sorted_returns() {
        let mut var = HistoricalVaR::new();
        var.add_returns(&[-50, -30, -10, 0, 10, 20, 30, 40, 50, 60]);
        // 10 サンプル, 95% VaR: idx = floor(0.05 * 10) = 0 → returns[0] = -50 → VaR = 50
        let v = var.var_at_confidence(0.95).unwrap();
        assert_eq!(v, 50);
    }

    #[test]
    fn historical_var_90_percent() {
        let mut var = HistoricalVaR::new();
        // 20 サンプルで浮動小数点丸めの影響を回避
        var.add_returns(&[
            -100, -90, -80, -70, -60, -50, -40, -30, -20, -10, 0, 10, 20, 30, 40, 50, 60, 70, 80,
            90,
        ]);
        // 90% VaR: (1.0 - 0.90) は f64 で 0.0999…998 のため
        // idx = floor(0.0999…998 * 20) = floor(1.999…96) = 1
        // returns[1] = -90 → VaR = 90
        let v = var.var_at_confidence(0.90).unwrap();
        assert_eq!(v, 90);
    }

    #[test]
    fn historical_var_invalid_confidence() {
        let mut var = HistoricalVaR::new();
        var.add_return(-10);
        assert!(var.var_at_confidence(1.5).is_none());
        assert!(var.var_at_confidence(-0.1).is_none());
    }

    #[test]
    fn historical_var_with_capacity() {
        let var = HistoricalVaR::with_capacity(100);
        assert_eq!(var.count(), 0);
    }

    #[test]
    fn historical_var_clear() {
        let mut var = HistoricalVaR::new();
        var.add_return(-50);
        var.add_return(-30);
        assert_eq!(var.count(), 2);
        var.clear();
        assert_eq!(var.count(), 0);
        assert!(var.var_at_confidence(0.95).is_none());
    }

    #[test]
    fn historical_var_default() {
        let var = HistoricalVaR::default();
        assert_eq!(var.count(), 0);
    }

    #[test]
    fn historical_var_positive_returns_negative_var() {
        let mut var = HistoricalVaR::new();
        // 全リターンが正の場合、VaR は負（利益が期待される）
        var.add_returns(&[10, 20, 30, 40, 50]);
        let v = var.var_at_confidence(0.95).unwrap();
        assert!(v < 0, "All positive returns should give negative VaR: {v}");
    }

    // -----------------------------------------------------------------------
    // ParametricVaR
    // -----------------------------------------------------------------------

    #[test]
    fn parametric_var_empty() {
        let var = ParametricVaR::new();
        assert!(var.mean().is_none());
        assert!(var.std_dev().is_none());
        assert!(var.var_at_confidence(0.95).is_none());
        assert_eq!(var.count(), 0);
    }

    #[test]
    fn parametric_var_single_sample() {
        let mut var = ParametricVaR::new();
        var.add_return(100);
        assert!((var.mean().unwrap() - 100.0).abs() < f64::EPSILON);
        // std_dev は n < 2 で None
        assert!(var.std_dev().is_none());
    }

    #[test]
    fn parametric_var_mean() {
        let mut var = ParametricVaR::new();
        for v in [10, 20, 30, 40, 50] {
            var.add_return(v);
        }
        let mean = var.mean().unwrap();
        assert!((mean - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parametric_var_std_dev() {
        let mut var = ParametricVaR::new();
        // 等差数列: [0, 10, 20, 30, 40]
        for v in [0, 10, 20, 30, 40] {
            var.add_return(v);
        }
        let sigma = var.std_dev().unwrap();
        // Population stddev of [0,10,20,30,40] = sqrt(200) ≈ 14.14
        assert!((sigma - 14.142_135).abs() < 0.01, "sigma = {sigma}");
    }

    #[test]
    fn parametric_var_95() {
        let mut var = ParametricVaR::new();
        for v in [-50, -30, -10, 10, 30, 50, -20, 0, 20, 40] {
            var.add_return(v);
        }
        let v = var.var_at_confidence(0.95);
        assert!(v.is_some());
        // 正の VaR（損失）であることを確認
        let val = v.unwrap();
        assert!(val > 0.0, "95% VaR should be positive for this data: {val}");
    }

    #[test]
    fn parametric_var_clear() {
        let mut var = ParametricVaR::new();
        var.add_return(100);
        var.add_return(200);
        var.clear();
        assert_eq!(var.count(), 0);
        assert!(var.mean().is_none());
    }

    #[test]
    fn parametric_var_default() {
        let var = ParametricVaR::default();
        assert_eq!(var.count(), 0);
    }

    #[test]
    fn parametric_var_invalid_confidence() {
        let mut var = ParametricVaR::new();
        var.add_return(10);
        var.add_return(20);
        assert!(var.var_at_confidence(0.3).is_none());
        assert!(var.var_at_confidence(1.0).is_none());
    }

    // -----------------------------------------------------------------------
    // z_score
    // -----------------------------------------------------------------------

    #[test]
    fn z_score_95() {
        let z = z_score(0.95).unwrap();
        // z(0.95) ≈ 1.645
        assert!((z - 1.645).abs() < 0.01, "z(0.95) = {z}");
    }

    #[test]
    fn z_score_99() {
        let z = z_score(0.99).unwrap();
        // z(0.99) ≈ 2.326
        assert!((z - 2.326).abs() < 0.01, "z(0.99) = {z}");
    }

    #[test]
    fn z_score_invalid() {
        assert!(z_score(0.3).is_none());
        assert!(z_score(1.0).is_none());
    }
}
