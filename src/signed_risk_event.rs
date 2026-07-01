//! `signed_risk_event` — tamper-evident risk event trail for post-trade audit.
//!
//! Every risk violation (limit breach, circuit breaker trip, margin call,
//! greeks limit trip, VaR excess, counterparty reject) is packaged into a
//! [`RiskEvent`] and signed with an `Ed25519` key managed by
//! `alice-blockchain`. Verifiers can then replay the event log and confirm
//! that no record has been forged or altered post-hoc.
//!
//! Basel III BCBS 239 §22-27 (principles for effective risk data aggregation
//! and risk reporting) and MiFID-II RTS 6 §14 (algorithmic trading controls)
//! both mandate an immutable audit trail — this module provides the
//! cryptographic primitive layer for it.
//!
//! # Layout
//!
//! ```text
//! RiskEvent { seq, kind, timestamp, actor, instrument, quantity, detail,
//!             prev_hash }
//!   ↓ canonical bytes
//! FNV-1a hash → alice_blockchain::signature::KeyPair::sign() → Ed25519 signature
//! ```
//!
//! The hash is chained (`prev_hash → hash`) so a `SignedRiskLog` doubles as
//! a hash chain — even without the signature the log is structurally
//! tamper-evident, and with the signature it is cryptographically bound to
//! the signer.

#![allow(
    clippy::doc_markdown,
    clippy::missing_panics_doc,
    clippy::too_many_arguments,
    clippy::cast_possible_wrap
)]

use alice_blockchain::signature::{KeyPair, PublicKey, Signature};

// ---------------------------------------------------------------------------
// RiskEventKind
// ---------------------------------------------------------------------------

/// The kind of risk event being recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskEventKind {
    /// Pre-trade limit breached.
    LimitBreach,
    /// Circuit breaker was tripped.
    CircuitTrip,
    /// Margin call issued to a counterparty.
    MarginCall,
    /// Greeks exposure limit breached.
    GreeksBreach,
    /// VaR limit exceeded.
    VarExcess,
    /// Counterparty exposure limit hit.
    CounterpartyReject,
    /// Stress test scenario flagged the portfolio.
    StressFail,
}

impl RiskEventKind {
    /// Short code used in canonical serialization.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::LimitBreach => "LIMIT",
            Self::CircuitTrip => "CIRCUIT",
            Self::MarginCall => "MARGIN",
            Self::GreeksBreach => "GREEKS",
            Self::VarExcess => "VAR",
            Self::CounterpartyReject => "CPTY",
            Self::StressFail => "STRESS",
        }
    }
}

// ---------------------------------------------------------------------------
// RiskEvent
// ---------------------------------------------------------------------------

/// A single risk event ready to be signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskEvent {
    /// Monotonic sequence number.
    pub seq: u64,
    /// The kind of event.
    pub kind: RiskEventKind,
    /// Unix nanosecond timestamp.
    pub timestamp_ns: u64,
    /// The account or desk responsible for the position.
    pub actor: String,
    /// Instrument identifier (`AAPL`, `BTC-USD`, `JGB-30Y`, ...).
    pub instrument: String,
    /// Signed quantity — positive for long exposure, negative for short.
    pub quantity: i64,
    /// Human-readable detail of the breach.
    pub detail: String,
    /// Hash of the previous event (0 for the first record).
    pub prev_hash: u64,
}

impl RiskEvent {
    /// Canonical byte layout used for hashing and signing.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.extend_from_slice(self.kind.code().as_bytes());
        buf.push(0);
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.extend_from_slice(self.actor.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.instrument.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&self.quantity.to_le_bytes());
        buf.extend_from_slice(self.detail.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&self.prev_hash.to_le_bytes());
        buf
    }

    /// Compute the `FNV-1a` hash of the canonical byte layout.
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &self.canonical_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}

// ---------------------------------------------------------------------------
// SignedRiskEvent
// ---------------------------------------------------------------------------

/// A [`RiskEvent`] plus the `Ed25519` signature over its canonical bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRiskEvent {
    /// The wrapped event.
    pub event: RiskEvent,
    /// `FNV-1a` hash of the event's canonical bytes.
    pub hash: u64,
    /// `Ed25519` signature over the canonical bytes.
    pub signature: Signature,
    /// Signer's `Ed25519` public key (32-byte payload).
    pub signer: PublicKey,
}

impl SignedRiskEvent {
    /// Verify the signature and recompute the hash.
    ///
    /// Returns `true` iff the signature is valid **and** the hash matches
    /// the canonical bytes.
    #[must_use]
    pub fn verify(&self) -> bool {
        if self.hash != self.event.hash() {
            return false;
        }
        self.signer
            .verify(&self.event.canonical_bytes(), &self.signature)
    }
}

// ---------------------------------------------------------------------------
// SignedRiskLog
// ---------------------------------------------------------------------------

/// Append-only log of [`SignedRiskEvent`] records.
#[derive(Debug, Clone, Default)]
pub struct SignedRiskLog {
    entries: Vec<SignedRiskEvent>,
}

impl SignedRiskLog {
    /// Construct an empty log.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read-only view.
    #[must_use]
    pub fn entries(&self) -> &[SignedRiskEvent] {
        &self.entries
    }

    /// Hash of the last event (0 for an empty log).
    #[must_use]
    pub fn tail_hash(&self) -> u64 {
        self.entries.last().map_or(0, |e| e.hash)
    }

    /// Append a new event signed with the provided key pair.
    ///
    /// The `seq` and `prev_hash` are assigned automatically.
    pub fn append(
        &mut self,
        keypair: &KeyPair,
        kind: RiskEventKind,
        timestamp_ns: u64,
        actor: impl Into<String>,
        instrument: impl Into<String>,
        quantity: i64,
        detail: impl Into<String>,
    ) -> &SignedRiskEvent {
        let seq = self.entries.len() as u64;
        let prev_hash = self.tail_hash();
        let event = RiskEvent {
            seq,
            kind,
            timestamp_ns,
            actor: actor.into(),
            instrument: instrument.into(),
            quantity,
            detail: detail.into(),
            prev_hash,
        };
        let bytes = event.canonical_bytes();
        let hash = event.hash();
        let signature = keypair.sign(&bytes);
        let signer = keypair.public();
        self.entries.push(SignedRiskEvent {
            event,
            hash,
            signature,
            signer,
        });
        self.entries.last().expect("entry was just pushed")
    }

    /// Verify signature and chain integrity end-to-end.
    ///
    /// Returns the 0-based index of the first invalid entry, or `None` if
    /// the log is intact.
    #[must_use]
    pub fn find_first_tamper(&self) -> Option<usize> {
        let mut expected_prev: u64 = 0;
        for (i, e) in self.entries.iter().enumerate() {
            if e.event.seq as usize != i {
                return Some(i);
            }
            if e.event.prev_hash != expected_prev {
                return Some(i);
            }
            if !e.verify() {
                return Some(i);
            }
            expected_prev = e.hash;
        }
        None
    }

    /// Whether the log is intact.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.find_first_tamper().is_none()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn kp(seed: u8) -> KeyPair {
        KeyPair::from_seed([seed; 32])
    }

    #[test]
    fn kind_code_is_stable() {
        assert_eq!(RiskEventKind::LimitBreach.code(), "LIMIT");
        assert_eq!(RiskEventKind::CircuitTrip.code(), "CIRCUIT");
        assert_eq!(RiskEventKind::MarginCall.code(), "MARGIN");
        assert_eq!(RiskEventKind::GreeksBreach.code(), "GREEKS");
        assert_eq!(RiskEventKind::VarExcess.code(), "VAR");
        assert_eq!(RiskEventKind::CounterpartyReject.code(), "CPTY");
        assert_eq!(RiskEventKind::StressFail.code(), "STRESS");
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let ev = RiskEvent {
            seq: 0,
            kind: RiskEventKind::LimitBreach,
            timestamp_ns: 1_000_000,
            actor: String::from("desk-A"),
            instrument: String::from("AAPL"),
            quantity: 100,
            detail: String::from("size > 50"),
            prev_hash: 0,
        };
        assert_eq!(ev.canonical_bytes(), ev.canonical_bytes());
    }

    #[test]
    fn hash_differs_for_different_events() {
        let a = RiskEvent {
            seq: 0,
            kind: RiskEventKind::LimitBreach,
            timestamp_ns: 1,
            actor: String::from("A"),
            instrument: String::from("AAPL"),
            quantity: 1,
            detail: String::new(),
            prev_hash: 0,
        };
        let mut b = a.clone();
        b.quantity = 2;
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn signed_event_verifies_after_appending() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        log.append(
            &k,
            RiskEventKind::LimitBreach,
            1000,
            "desk-A",
            "AAPL",
            100,
            "limit hit",
        );
        assert!(log.entries()[0].verify());
    }

    #[test]
    fn empty_log_tail_hash_is_zero() {
        let log = SignedRiskLog::new();
        assert_eq!(log.tail_hash(), 0);
        assert!(log.is_empty());
    }

    #[test]
    fn seq_is_monotonic() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        for i in 0..5 {
            log.append(&k, RiskEventKind::LimitBreach, i, "A", "X", i as i64, "");
        }
        for (i, e) in log.entries().iter().enumerate() {
            assert_eq!(e.event.seq as usize, i);
        }
    }

    #[test]
    fn genesis_prev_hash_is_zero() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        log.append(&k, RiskEventKind::LimitBreach, 1, "A", "X", 1, "");
        assert_eq!(log.entries()[0].event.prev_hash, 0);
    }

    #[test]
    fn chained_prev_hash_matches_predecessor() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        log.append(&k, RiskEventKind::LimitBreach, 1, "A", "X", 1, "");
        log.append(&k, RiskEventKind::CircuitTrip, 2, "B", "Y", 2, "");
        let first_hash = log.entries()[0].hash;
        assert_eq!(log.entries()[1].event.prev_hash, first_hash);
    }

    #[test]
    fn intact_log_is_valid() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        log.append(&k, RiskEventKind::LimitBreach, 1, "A", "X", 1, "");
        log.append(&k, RiskEventKind::CircuitTrip, 2, "B", "Y", 2, "");
        log.append(&k, RiskEventKind::MarginCall, 3, "C", "Z", 3, "");
        assert!(log.is_valid());
    }

    #[test]
    fn tampered_quantity_is_detected() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        log.append(&k, RiskEventKind::LimitBreach, 1, "A", "X", 1, "");
        log.append(&k, RiskEventKind::CircuitTrip, 2, "B", "Y", 2, "");
        // Attacker rewrites the first quantity but leaves the signature.
        log.entries[0].event.quantity = 999;
        assert_eq!(log.find_first_tamper(), Some(0));
    }

    #[test]
    fn tampered_detail_breaks_verify() {
        let mut log = SignedRiskLog::new();
        let k = kp(1);
        log.append(&k, RiskEventKind::LimitBreach, 1, "A", "X", 1, "original");
        log.entries[0].event.detail = String::from("modified");
        assert!(!log.entries[0].verify());
    }

    #[test]
    fn foreign_signer_is_rejected_on_reverify() {
        let mut log = SignedRiskLog::new();
        let owner = kp(1);
        let attacker = kp(2);
        log.append(
            &owner,
            RiskEventKind::LimitBreach,
            1,
            "A",
            "X",
            1,
            "genuine",
        );
        // Attacker swaps in a signature of their own on the same message.
        let bytes = log.entries[0].event.canonical_bytes();
        log.entries[0].signature = attacker.sign(&bytes);
        // Owner's key can no longer verify the message.
        assert!(!log.entries[0].verify());
    }

    #[test]
    fn different_kinds_produce_different_hashes() {
        let mk = |kind: RiskEventKind| RiskEvent {
            seq: 0,
            kind,
            timestamp_ns: 1,
            actor: String::new(),
            instrument: String::new(),
            quantity: 0,
            detail: String::new(),
            prev_hash: 0,
        };
        assert_ne!(
            mk(RiskEventKind::LimitBreach).hash(),
            mk(RiskEventKind::CircuitTrip).hash()
        );
    }
}
