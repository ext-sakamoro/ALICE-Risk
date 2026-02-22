/*
    ALICE-Risk
    Copyright (C) 2026 Moroya Sakamoto
*/

//! Circuit breaker for anomalous price movement and excessive fill rate.
//!
//! [`CircuitBreaker`] monitors each fill event.  If the price moves more than
//! `max_move` ticks from a reference price, or if `max_fills_per_window` fills
//! occur within a rolling `window_ns`-nanosecond window, the breaker trips and
//! the caller must halt order flow until an explicit [`CircuitBreaker::reset`].

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

/// Monitors fill events and trips on anomalous activity.
pub struct CircuitBreaker {
    /// Maximum price deviation (in ticks) from the reference price before tripping.
    pub max_move: i64,
    /// Maximum number of fills within the rolling window before tripping.
    pub max_fills_per_window: u32,
    /// Rolling window duration in nanoseconds.
    pub window_ns: u64,

    // Internal state
    fills_in_window: u32,
    window_start_ns: u64,
    reference_price: i64,
    tripped: bool,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    ///
    /// The breaker starts in the untripped state.  `reference_price` and the
    /// window start timestamp are both initialised to zero; call
    /// [`CircuitBreaker::reset`] with a real price and timestamp before
    /// processing live fills.
    #[inline(always)]
    pub fn new(max_move: i64, max_fills_per_window: u32, window_ns: u64) -> Self {
        Self {
            max_move,
            max_fills_per_window,
            window_ns,
            fills_in_window: 0,
            window_start_ns: 0,
            reference_price: 0,
            tripped: false,
        }
    }

    /// Process a fill event and return `true` if the circuit breaker trips.
    ///
    /// The following checks are performed in order:
    /// 1. If `timestamp_ns` is outside the current window, the window and fill
    ///    counter are reset; `reference_price` is updated to `price`.
    /// 2. The absolute price deviation from `reference_price` is compared to
    ///    `max_move`; if exceeded, the breaker trips.
    /// 3. The fill counter is incremented and checked against
    ///    `max_fills_per_window`; if exceeded, the breaker trips.
    ///
    /// Returns `true` if this call caused a trip (or if the breaker was already
    /// tripped before this call).
    pub fn on_fill(&mut self, price: i64, timestamp_ns: u64) -> bool {
        // If already tripped, short-circuit.
        if self.tripped {
            return true;
        }

        // Roll the window if we have moved past the window boundary.
        let elapsed = timestamp_ns.saturating_sub(self.window_start_ns);
        if elapsed >= self.window_ns {
            self.window_start_ns = timestamp_ns;
            self.fills_in_window = 0;
            self.reference_price = price;
        }

        // Check price deviation.
        let deviation = (price - self.reference_price).abs();
        if deviation > self.max_move {
            self.tripped = true;
            return true;
        }

        // Increment fill counter and check rate limit.
        self.fills_in_window = self.fills_in_window.saturating_add(1);
        if self.fills_in_window > self.max_fills_per_window {
            self.tripped = true;
            return true;
        }

        false
    }

    /// Return `true` if the circuit breaker is currently tripped.
    #[inline(always)]
    pub fn is_tripped(&self) -> bool {
        self.tripped
    }

    /// Reset the circuit breaker to the untripped state.
    ///
    /// Clears the trip flag, fill counter, and starts a new window anchored at
    /// `timestamp_ns` with `reference_price` as the new baseline.
    #[inline(always)]
    pub fn reset(&mut self, reference_price: i64, timestamp_ns: u64) {
        self.tripped = false;
        self.fills_in_window = 0;
        self.window_start_ns = timestamp_ns;
        self.reference_price = reference_price;
    }

    /// Update the reference price without resetting the window or trip state.
    ///
    /// Use this to track a slowly drifting fair value while preserving the
    /// current window's fill count.
    #[inline(always)]
    pub fn set_reference_price(&mut self, price: i64) {
        self.reference_price = price;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cb() -> CircuitBreaker {
        // max_move=500, max_fills=5, window=1_000_000_000 ns (1 s)
        CircuitBreaker::new(500, 5, 1_000_000_000)
    }

    // -----------------------------------------------------------------------
    // No-trip cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_trip_within_limits() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Five fills within the window, all within the price band.
        for i in 0..5 {
            let result = cb.on_fill(10_100, i * 100_000_000);
            assert!(!result, "fill {i} should not trip");
        }
        assert!(!cb.is_tripped());
    }

    // -----------------------------------------------------------------------
    // Price-move trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_trip_on_price_move() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // A fill 501 ticks above the reference should trip the breaker.
        let tripped = cb.on_fill(10_501, 100_000_000);
        assert!(tripped);
        assert!(cb.is_tripped());
    }

    #[test]
    fn test_trip_on_price_move_downward() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        let tripped = cb.on_fill(9_499, 100_000_000);
        assert!(tripped);
        assert!(cb.is_tripped());
    }

    #[test]
    fn test_no_trip_at_exact_max_move() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Exactly at the boundary (== max_move is NOT a trip; > is required).
        let tripped = cb.on_fill(10_500, 100_000_000);
        assert!(!tripped);
        assert!(!cb.is_tripped());
    }

    // -----------------------------------------------------------------------
    // Fill-count trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_trip_on_fill_count() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Six fills exceed max_fills_per_window=5.
        for i in 0..5 {
            assert!(!cb.on_fill(10_050, i * 10_000_000));
        }
        let tripped = cb.on_fill(10_050, 6 * 10_000_000);
        assert!(tripped);
        assert!(cb.is_tripped());
    }

    // -----------------------------------------------------------------------
    // Window reset
    // -----------------------------------------------------------------------

    #[test]
    fn test_window_reset() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Fill up to the limit within the first window.
        for i in 0..5 {
            cb.on_fill(10_050, i * 10_000_000);
        }
        assert!(!cb.is_tripped());

        // A fill after one full window duration resets the counter.
        // timestamp = 1_000_000_001 ns > window_ns=1_000_000_000.
        let tripped = cb.on_fill(10_050, 1_000_000_001);
        assert!(!tripped);
        assert!(!cb.is_tripped());
    }

    // -----------------------------------------------------------------------
    // Manual reset
    // -----------------------------------------------------------------------

    #[test]
    fn test_manual_reset() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Trip the breaker via price move.
        cb.on_fill(10_600, 100_000_000);
        assert!(cb.is_tripped());

        // Manual reset should clear the trip state.
        cb.reset(10_600, 200_000_000);
        assert!(!cb.is_tripped());

        // Subsequent fills within limits should succeed.
        assert!(!cb.on_fill(10_700, 300_000_000));
    }

    // -----------------------------------------------------------------------
    // set_reference_price
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_reference_price_updates_baseline() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Advance the reference price; a fill at 10_600 is now within 500 of 10_200.
        cb.set_reference_price(10_200);
        assert!(!cb.on_fill(10_600, 100_000_000));
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_set_reference_price_does_not_clear_trip() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        cb.on_fill(10_600, 100_000_000);
        assert!(cb.is_tripped());

        // Updating the reference price must NOT clear the trip.
        cb.set_reference_price(10_600);
        assert!(cb.is_tripped());
    }

    // -------------------------------------------------------------------
    // Already-tripped behavior
    // -------------------------------------------------------------------

    #[test]
    fn test_on_fill_when_already_tripped_returns_true() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Trip via price move.
        cb.on_fill(10_600, 100_000_000);
        assert!(cb.is_tripped());

        // Subsequent fill should immediately return true without processing.
        assert!(cb.on_fill(10_000, 200_000_000));
    }

    // -------------------------------------------------------------------
    // Window rolling edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_fill_at_exact_window_boundary_resets() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Fill 5 times within window.
        for i in 0..5 {
            cb.on_fill(10_050, i * 100_000_000);
        }
        assert!(!cb.is_tripped());

        // Fill at exactly window_ns (1_000_000_000) should roll the window.
        let tripped = cb.on_fill(10_050, 1_000_000_000);
        assert!(!tripped);
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_window_roll_resets_reference_price() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Fill within window at a different price.
        cb.on_fill(10_100, 100_000_000);
        assert!(!cb.is_tripped());

        // After window rolls, the new reference price is set to the fill price.
        // A fill at the new window at 11_000 with reference updated to 11_000.
        let tripped = cb.on_fill(11_000, 2_000_000_000);
        // reference_price becomes 11_000 on window roll, so deviation = 0.
        assert!(!tripped);

        // Now a fill 501 above 11_000 should trip.
        let tripped = cb.on_fill(11_501, 2_100_000_000);
        assert!(tripped);
    }

    // -------------------------------------------------------------------
    // Fill count boundary
    // -------------------------------------------------------------------

    #[test]
    fn test_exactly_max_fills_does_not_trip() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // max_fills_per_window = 5; exactly 5 fills should NOT trip (>5 trips).
        for i in 0..5 {
            let tripped = cb.on_fill(10_050, i * 10_000_000);
            assert!(!tripped, "fill {} should not trip", i);
        }
        assert!(!cb.is_tripped());
    }

    // -------------------------------------------------------------------
    // Zero-tolerance breaker
    // -------------------------------------------------------------------

    #[test]
    fn test_zero_max_move_trips_on_any_deviation() {
        let mut cb = CircuitBreaker::new(0, 100, 1_000_000_000);
        cb.reset(10_000, 0);

        // Any price deviation > 0 should trip.
        assert!(cb.on_fill(10_001, 100_000_000));
        assert!(cb.is_tripped());
    }

    #[test]
    fn test_zero_max_move_no_trip_at_same_price() {
        let mut cb = CircuitBreaker::new(0, 100, 1_000_000_000);
        cb.reset(10_000, 0);

        // Same price as reference: deviation = 0, not > 0.
        assert!(!cb.on_fill(10_000, 100_000_000));
    }

    #[test]
    fn test_zero_max_fills_trips_on_first_fill() {
        let mut cb = CircuitBreaker::new(500, 0, 1_000_000_000);
        cb.reset(10_000, 0);

        // max_fills_per_window = 0; first fill increments count to 1 > 0.
        assert!(cb.on_fill(10_000, 100_000_000));
        assert!(cb.is_tripped());
    }

    // -------------------------------------------------------------------
    // Multiple windows
    // -------------------------------------------------------------------

    #[test]
    fn test_multiple_window_rollovers() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Fill 5 in window 1, then roll to window 2 and fill 5 more.
        for i in 0..5 {
            assert!(!cb.on_fill(10_050, i * 100_000_000));
        }

        // Window 2.
        for i in 0..5 {
            let ts = 1_000_000_001 + i * 100_000_000;
            assert!(!cb.on_fill(10_050, ts));
        }

        // Window 3.
        for i in 0..5 {
            let ts = 2_000_000_002 + i * 100_000_000;
            assert!(!cb.on_fill(10_050, ts));
        }

        assert!(!cb.is_tripped());
    }

    // -------------------------------------------------------------------
    // Reset after fill-count trip
    // -------------------------------------------------------------------

    #[test]
    fn test_reset_after_fill_count_trip() {
        let mut cb = make_cb();
        cb.reset(10_000, 0);

        // Trip via fill count.
        for i in 0..6 {
            cb.on_fill(10_050, i * 10_000_000);
        }
        assert!(cb.is_tripped());

        // Reset and verify normal operation resumes.
        cb.reset(10_050, 500_000_000);
        assert!(!cb.is_tripped());
        assert!(!cb.on_fill(10_050, 600_000_000));
    }
}
