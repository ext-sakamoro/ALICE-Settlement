//! `SHA-256` + `Ed25519` merkle-anchored settlement audit trail.
//!
//! Post-trade settlement events (novation, netting, clearing, collateral
//! movement) are recorded as [`SettlementEvent`]s, hash-chained by
//! `SHA-256`, and periodically committed to a Merkle root that can be
//! anchored into an external ledger for immutability. Each event may
//! optionally carry an `Ed25519` signature from the operator responsible.
//!
//! Replaces the crate's earlier `FNV-1a` hash which is unsuitable for
//! financial audit under `EMIR` / `MiFID-II` reporting rules.

use alice_blockchain::{hash_data, Hash, KeyPair, MerkleTree, PublicKey, Signature};

// ---------------------------------------------------------------------------
// SettlementEvent
// ---------------------------------------------------------------------------

/// Category of settlement event.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementEventKind {
    /// A new trade booked for settlement.
    TradeBooked,
    /// Bilateral novation to a `CCP`.
    Novation,
    /// Multilateral netting cycle result.
    Netting,
    /// Cash / securities settlement.
    Settlement,
    /// Collateral posted or returned.
    CollateralMovement,
    /// Default management event (e.g. auction).
    DefaultAuction,
}

impl SettlementEventKind {
    /// Byte tag used inside canonical byte layouts.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::TradeBooked => 1,
            Self::Novation => 2,
            Self::Netting => 3,
            Self::Settlement => 4,
            Self::CollateralMovement => 5,
            Self::DefaultAuction => 6,
        }
    }
}

/// One settlement audit event.
#[derive(Debug, Clone)]
pub struct SettlementEvent {
    pub sequence: u64,
    pub kind: SettlementEventKind,
    pub trade_id: String,
    pub counterparty_id: String,
    pub notional_micros: i128,
    pub timestamp_unix: u64,
    /// Optional signature by the responsible operator.
    pub signature: Option<Signature>,
    pub signer: Option<PublicKey>,
}

impl SettlementEvent {
    /// Convenience constructor.
    #[must_use]
    pub fn new(
        sequence: u64,
        kind: SettlementEventKind,
        trade_id: impl Into<String>,
        counterparty_id: impl Into<String>,
        notional_micros: i128,
        timestamp_unix: u64,
    ) -> Self {
        Self {
            sequence,
            kind,
            trade_id: trade_id.into(),
            counterparty_id: counterparty_id.into(),
            notional_micros,
            timestamp_unix,
            signature: None,
            signer: None,
        }
    }

    /// Canonical byte serialisation used for hashing and signing.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        buf.push(self.kind.tag());
        push_len(&mut buf, self.trade_id.as_bytes());
        push_len(&mut buf, self.counterparty_id.as_bytes());
        buf.extend_from_slice(&self.notional_micros.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_unix.to_le_bytes());
        buf
    }

    /// `SHA-256` digest of the canonical byte layout.
    #[must_use]
    pub fn digest(&self) -> Hash {
        hash_data(&self.canonical_bytes())
    }

    /// Attach a signature from `kp`.
    pub fn sign(&mut self, kp: &KeyPair) {
        let sig = kp.sign(&self.canonical_bytes());
        self.signature = Some(sig);
        self.signer = Some(kp.public());
    }

    /// Verify signature if attached; unsigned events pass trivially.
    #[must_use]
    pub fn verify_signature(&self) -> bool {
        match (self.signature.as_ref(), self.signer.as_ref()) {
            (None, None) => true,
            (Some(sig), Some(pk)) => pk.verify(&self.canonical_bytes(), sig),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// SettlementTrail
// ---------------------------------------------------------------------------

/// Append-only hash-chain of settlement events.
#[derive(Debug, Clone)]
pub struct SettlementTrail {
    events: Vec<SettlementEvent>,
    chain: Vec<Hash>,
}

impl SettlementTrail {
    /// Empty trail.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: Vec::new(),
            chain: Vec::new(),
        }
    }

    /// Append an event and update the hash chain head.
    pub fn append(&mut self, event: SettlementEvent) {
        let prev = self.chain.last().copied().unwrap_or_else(Hash::zero);
        let mut linked = Vec::with_capacity(64);
        linked.extend_from_slice(&prev.0);
        linked.extend_from_slice(&event.digest().0);
        let head = hash_data(&linked);
        self.events.push(event);
        self.chain.push(head);
    }

    /// Chain head, or the zero hash for an empty trail.
    #[must_use]
    pub fn head(&self) -> Hash {
        self.chain.last().copied().unwrap_or_else(Hash::zero)
    }

    /// Read events.
    #[must_use]
    pub fn events(&self) -> &[SettlementEvent] {
        &self.events
    }

    /// Number of events.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether no events have been recorded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Verify chain integrity and every attached signature.
    #[must_use]
    pub fn verify(&self) -> bool {
        let mut prev = Hash::zero();
        for (e, expected) in self.events.iter().zip(self.chain.iter()) {
            if !e.verify_signature() {
                return false;
            }
            let mut linked = Vec::with_capacity(64);
            linked.extend_from_slice(&prev.0);
            linked.extend_from_slice(&e.digest().0);
            let head = hash_data(&linked);
            if head != *expected {
                return false;
            }
            prev = head;
        }
        true
    }

    /// Compute a Merkle root over every event digest, suitable for external
    /// ledger anchoring (`EMIR` `TR` daily reporting cadence).
    #[must_use]
    pub fn merkle_root(&self) -> Option<Hash> {
        if self.events.is_empty() {
            return None;
        }
        let leaves: Vec<Hash> = self.events.iter().map(SettlementEvent::digest).collect();
        Some(MerkleTree::build(&leaves).root())
    }
}

impl Default for SettlementTrail {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn push_len(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    buf.extend_from_slice(bytes);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: u64, kind: SettlementEventKind, trade: &str) -> SettlementEvent {
        SettlementEvent::new(seq, kind, trade, "CP-01", 1_000_000_000, 1_720_000_000)
    }

    #[test]
    fn event_kinds_have_distinct_tags() {
        let mut seen = std::collections::HashSet::new();
        for k in [
            SettlementEventKind::TradeBooked,
            SettlementEventKind::Novation,
            SettlementEventKind::Netting,
            SettlementEventKind::Settlement,
            SettlementEventKind::CollateralMovement,
            SettlementEventKind::DefaultAuction,
        ] {
            assert!(seen.insert(k.tag()));
        }
    }

    #[test]
    fn digest_changes_when_notional_changes() {
        let a = ev(0, SettlementEventKind::TradeBooked, "T-1");
        let mut b = a.clone();
        b.notional_micros += 1;
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn unsigned_event_passes_signature_check() {
        let e = ev(0, SettlementEventKind::TradeBooked, "T-1");
        assert!(e.verify_signature());
    }

    #[test]
    fn signed_event_verifies() {
        let kp = KeyPair::from_seed([1u8; 32]);
        let mut e = ev(0, SettlementEventKind::TradeBooked, "T-1");
        e.sign(&kp);
        assert!(e.verify_signature());
    }

    #[test]
    fn tampering_signed_event_breaks_signature() {
        let kp = KeyPair::from_seed([1u8; 32]);
        let mut e = ev(0, SettlementEventKind::TradeBooked, "T-1");
        e.sign(&kp);
        e.notional_micros = 42;
        assert!(!e.verify_signature());
    }

    #[test]
    fn empty_trail_head_is_zero_hash() {
        let t = SettlementTrail::new();
        assert_eq!(t.head(), Hash::zero());
        assert!(t.is_empty());
        assert!(t.merkle_root().is_none());
        assert!(t.verify());
    }

    #[test]
    fn head_advances_with_each_append() {
        let mut t = SettlementTrail::new();
        t.append(ev(0, SettlementEventKind::TradeBooked, "T-1"));
        let h1 = t.head();
        t.append(ev(1, SettlementEventKind::Settlement, "T-1"));
        assert_ne!(h1, t.head());
        assert!(t.verify());
    }

    #[test]
    fn tampering_middle_event_breaks_chain() {
        let mut t = SettlementTrail::new();
        for i in 0..3 {
            t.append(ev(i, SettlementEventKind::TradeBooked, "T-1"));
        }
        // Simulate storage-level tampering: rewrite the middle event's notional.
        t.events[1].notional_micros = -1;
        assert!(!t.verify());
    }

    #[test]
    fn merkle_root_matches_manual_build() {
        let mut t = SettlementTrail::new();
        for i in 0..4 {
            t.append(ev(i, SettlementEventKind::Netting, "T-1"));
        }
        let root = t.merkle_root().unwrap();
        let leaves: Vec<Hash> = t.events().iter().map(SettlementEvent::digest).collect();
        assert_eq!(root, MerkleTree::build(&leaves).root());
    }

    #[test]
    fn digest_changes_when_kind_changes() {
        let a = ev(0, SettlementEventKind::TradeBooked, "T-1");
        let b = ev(0, SettlementEventKind::Netting, "T-1");
        assert_ne!(a.digest(), b.digest());
    }
}
