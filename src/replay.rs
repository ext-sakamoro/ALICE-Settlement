// ALICE-Settlement — Deterministic journal replay and verification
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Moroya Sakamoto

use crate::fnv1a;
use crate::journal::{JournalEvent, SettlementJournal};

// ── Types ──────────────────────────────────────────────────────────────

/// A single step in a replay log, capturing the deterministic fingerprint
/// of a journal entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayStep {
    /// Journal sequence number.
    pub sequence: u64,
    /// Timestamp from the journal entry.
    pub timestamp_ns: u64,
    /// Event kind discriminant.
    pub event_kind: u8,
    /// Deterministic content hash of this step.
    pub content_hash: u64,
}

/// A discrepancy found during replay verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayDiscrepancy {
    /// Sequence number where the mismatch occurred.
    pub sequence: u64,
    /// Hash from the expected log.
    pub expected_hash: u64,
    /// Hash from the actual log.
    pub actual_hash: u64,
}

/// Result of verifying two replay logs against each other.
#[derive(Debug, Clone)]
pub struct ReplayResult {
    /// Number of steps successfully verified before a mismatch or end.
    pub steps_verified: usize,
    /// All discrepancies found (empty if logs match).
    pub discrepancies: Vec<ReplayDiscrepancy>,
    /// True if all steps match and lengths are equal.
    pub success: bool,
    /// Deterministic content hash of the entire result.
    pub content_hash: u64,
}

// ── Replay Verifier ────────────────────────────────────────────────────

/// Deterministic journal replay and verification engine.
///
/// Builds content-hashed replay logs from settlement journals and
/// verifies that two logs are identical, enabling replay-based
/// auditing and disaster recovery validation.
pub struct ReplayVerifier;

impl ReplayVerifier {
    /// Build a replay log from a settlement journal.
    ///
    /// Each journal entry is mapped to a `ReplayStep` with a deterministic
    /// content hash derived from the entry's sequence, timestamp, and event
    /// kind/payload.
    pub fn build_replay_log(journal: &SettlementJournal) -> Vec<ReplayStep> {
        journal
            .entries()
            .iter()
            .map(|entry| {
                let event_kind = Self::event_kind_byte(&entry.event);
                let event_payload = Self::event_payload(&entry.event);
                let hash = Self::step_hash(
                    entry.sequence,
                    entry.timestamp_ns,
                    event_kind,
                    event_payload,
                );
                ReplayStep {
                    sequence: entry.sequence,
                    timestamp_ns: entry.timestamp_ns,
                    event_kind,
                    content_hash: hash,
                }
            })
            .collect()
    }

    /// Verify that two replay logs are identical.
    ///
    /// Compares step-by-step, recording all discrepancies.  A length
    /// mismatch is reported as a discrepancy at the shorter log's length.
    pub fn verify(expected: &[ReplayStep], actual: &[ReplayStep]) -> ReplayResult {
        let mut discrepancies = Vec::new();
        let min_len = expected.len().min(actual.len());
        let mut verified = 0;

        for i in 0..min_len {
            if expected[i].content_hash != actual[i].content_hash {
                discrepancies.push(ReplayDiscrepancy {
                    sequence: expected[i].sequence,
                    expected_hash: expected[i].content_hash,
                    actual_hash: actual[i].content_hash,
                });
            } else {
                verified += 1;
            }
        }

        // Length mismatch
        if expected.len() != actual.len() {
            let seq = if min_len > 0 {
                expected
                    .get(min_len - 1)
                    .or(actual.get(min_len - 1))
                    .map(|s| s.sequence + 1)
                    .unwrap_or(1)
            } else {
                1
            };
            discrepancies.push(ReplayDiscrepancy {
                sequence: seq,
                expected_hash: expected.len() as u64,
                actual_hash: actual.len() as u64,
            });
        }

        let success = discrepancies.is_empty();
        let result_hash = Self::result_hash(verified, discrepancies.len());

        ReplayResult {
            steps_verified: verified,
            discrepancies,
            success,
            content_hash: result_hash,
        }
    }

    /// Compute a single deterministic hash for an entire journal.
    ///
    /// Chains all entry hashes together, producing a cumulative fingerprint
    /// suitable for integrity verification.
    pub fn compute_journal_hash(journal: &SettlementJournal) -> u64 {
        let mut cumulative: u64 = 0xcbf29ce484222325; // FNV offset basis
        for entry in journal.entries() {
            let kind = Self::event_kind_byte(&entry.event);
            let payload = Self::event_payload(&entry.event);
            let step_h = Self::step_hash(entry.sequence, entry.timestamp_ns, kind, payload);
            // Chain: fold step hash into cumulative
            let mut data = [0u8; 16];
            data[0..8].copy_from_slice(&cumulative.to_le_bytes());
            data[8..16].copy_from_slice(&step_h.to_le_bytes());
            cumulative = fnv1a(&data);
        }
        cumulative
    }

    /// Map event variants to a discriminant byte.
    fn event_kind_byte(event: &JournalEvent) -> u8 {
        match event {
            JournalEvent::TradeReceived { .. } => 0,
            JournalEvent::NettingCompleted { .. } => 1,
            JournalEvent::ClearingAttempted { .. } => 2,
            JournalEvent::SettlementCompleted { .. } => 3,
            JournalEvent::SettlementFailed { .. } => 4,
        }
    }

    /// Extract a numeric payload from an event for hashing.
    fn event_payload(event: &JournalEvent) -> u64 {
        match event {
            JournalEvent::TradeReceived { trade_id } => *trade_id,
            JournalEvent::NettingCompleted { obligation_count } => *obligation_count as u64,
            JournalEvent::ClearingAttempted {
                obligation_count,
                success_count,
                fail_count,
            } => {
                (*obligation_count as u64) << 32
                    | (*success_count as u64) << 16
                    | (*fail_count as u64)
            }
            JournalEvent::SettlementCompleted { trade_count } => *trade_count as u64,
            JournalEvent::SettlementFailed { trade_id, reason } => {
                let reason_hash = fnv1a(reason.as_bytes());
                *trade_id ^ reason_hash
            }
        }
    }

    fn step_hash(sequence: u64, timestamp_ns: u64, kind: u8, payload: u64) -> u64 {
        let mut data = [0u8; 25];
        data[0..8].copy_from_slice(&sequence.to_le_bytes());
        data[8..16].copy_from_slice(&timestamp_ns.to_le_bytes());
        data[16] = kind;
        data[17..25].copy_from_slice(&payload.to_le_bytes());
        fnv1a(&data)
    }

    fn result_hash(verified: usize, discrepancy_count: usize) -> u64 {
        let mut data = [0u8; 16];
        data[0..8].copy_from_slice(&(verified as u64).to_le_bytes());
        data[8..16].copy_from_slice(&(discrepancy_count as u64).to_le_bytes());
        fnv1a(&data)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_journal(events: &[(u64, JournalEvent)]) -> SettlementJournal {
        let mut journal = SettlementJournal::new();
        for (ts, event) in events {
            journal.record(*ts, event.clone());
        }
        journal
    }

    #[test]
    fn empty_journal_replay() {
        let journal = SettlementJournal::new();
        let log = ReplayVerifier::build_replay_log(&journal);
        assert!(log.is_empty());
    }

    #[test]
    fn single_entry_replay() {
        let journal = make_journal(&[(1000, JournalEvent::TradeReceived { trade_id: 42 })]);
        let log = ReplayVerifier::build_replay_log(&journal);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].sequence, 1);
        assert_eq!(log[0].timestamp_ns, 1000);
        assert_eq!(log[0].event_kind, 0);
        assert_ne!(log[0].content_hash, 0);
    }

    #[test]
    fn replay_step_ordering() {
        let journal = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
            (200, JournalEvent::TradeReceived { trade_id: 2 }),
            (
                300,
                JournalEvent::NettingCompleted {
                    obligation_count: 5,
                },
            ),
        ]);
        let log = ReplayVerifier::build_replay_log(&journal);
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].sequence, 1);
        assert_eq!(log[1].sequence, 2);
        assert_eq!(log[2].sequence, 3);
        assert_eq!(log[2].event_kind, 1); // NettingCompleted
    }

    #[test]
    fn matching_logs_verify() {
        let journal = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
            (200, JournalEvent::TradeReceived { trade_id: 2 }),
        ]);
        let log1 = ReplayVerifier::build_replay_log(&journal);
        let log2 = ReplayVerifier::build_replay_log(&journal);

        let result = ReplayVerifier::verify(&log1, &log2);
        assert!(result.success);
        assert_eq!(result.steps_verified, 2);
        assert!(result.discrepancies.is_empty());
    }

    #[test]
    fn mismatched_logs_detect_discrepancy() {
        let j1 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let j2 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 999 })]);
        let log1 = ReplayVerifier::build_replay_log(&j1);
        let log2 = ReplayVerifier::build_replay_log(&j2);

        let result = ReplayVerifier::verify(&log1, &log2);
        assert!(!result.success);
        assert_eq!(result.discrepancies.len(), 1);
        assert_eq!(result.discrepancies[0].sequence, 1);
        assert_ne!(
            result.discrepancies[0].expected_hash,
            result.discrepancies[0].actual_hash
        );
    }

    #[test]
    fn different_length_logs() {
        let j1 = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
            (200, JournalEvent::TradeReceived { trade_id: 2 }),
        ]);
        let j2 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let log1 = ReplayVerifier::build_replay_log(&j1);
        let log2 = ReplayVerifier::build_replay_log(&j2);

        let result = ReplayVerifier::verify(&log1, &log2);
        assert!(!result.success);
        // Length mismatch creates a discrepancy
        assert!(!result.discrepancies.is_empty());
    }

    #[test]
    fn multiple_discrepancies() {
        let j1 = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
            (200, JournalEvent::TradeReceived { trade_id: 2 }),
            (300, JournalEvent::TradeReceived { trade_id: 3 }),
        ]);
        let j2 = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 99 }),
            (200, JournalEvent::TradeReceived { trade_id: 2 }), // matches
            (300, JournalEvent::TradeReceived { trade_id: 98 }),
        ]);
        let log1 = ReplayVerifier::build_replay_log(&j1);
        let log2 = ReplayVerifier::build_replay_log(&j2);

        let result = ReplayVerifier::verify(&log1, &log2);
        assert!(!result.success);
        assert_eq!(result.steps_verified, 1); // only step 2 matches
        assert_eq!(result.discrepancies.len(), 2);
    }

    #[test]
    fn journal_hash_deterministic() {
        let journal = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
            (
                200,
                JournalEvent::NettingCompleted {
                    obligation_count: 3,
                },
            ),
        ]);
        let h1 = ReplayVerifier::compute_journal_hash(&journal);
        let h2 = ReplayVerifier::compute_journal_hash(&journal);
        assert_eq!(h1, h2);
        assert_ne!(h1, 0);
    }

    #[test]
    fn journal_hash_changes_with_content() {
        let j1 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let j2 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 2 })]);
        let h1 = ReplayVerifier::compute_journal_hash(&j1);
        let h2 = ReplayVerifier::compute_journal_hash(&j2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn journal_hash_changes_with_timestamp() {
        let j1 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let j2 = make_journal(&[(200, JournalEvent::TradeReceived { trade_id: 1 })]);
        let h1 = ReplayVerifier::compute_journal_hash(&j1);
        let h2 = ReplayVerifier::compute_journal_hash(&j2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn empty_journal_hash() {
        let journal = SettlementJournal::new();
        let h = ReplayVerifier::compute_journal_hash(&journal);
        // Empty journal returns the FNV offset basis
        assert_eq!(h, 0xcbf29ce484222325);
    }

    #[test]
    fn all_event_kinds_have_distinct_discriminants() {
        let events = vec![
            JournalEvent::TradeReceived { trade_id: 1 },
            JournalEvent::NettingCompleted {
                obligation_count: 1,
            },
            JournalEvent::ClearingAttempted {
                obligation_count: 1,
                success_count: 1,
                fail_count: 0,
            },
            JournalEvent::SettlementCompleted { trade_count: 1 },
            JournalEvent::SettlementFailed {
                trade_id: 1,
                reason: "test".to_string(),
            },
        ];
        let kinds: Vec<u8> = events
            .iter()
            .map(|e| ReplayVerifier::event_kind_byte(e))
            .collect();
        // All discriminants must be unique
        for i in 0..kinds.len() {
            for j in (i + 1)..kinds.len() {
                assert_ne!(kinds[i], kinds[j], "kinds {} and {} collide", i, j);
            }
        }
    }

    #[test]
    fn verify_result_content_hash_deterministic() {
        let j = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let log = ReplayVerifier::build_replay_log(&j);
        let r1 = ReplayVerifier::verify(&log, &log);
        let r2 = ReplayVerifier::verify(&log, &log);
        assert_eq!(r1.content_hash, r2.content_hash);
        assert_ne!(r1.content_hash, 0);
    }

    #[test]
    fn large_journal_replay() {
        let events: Vec<(u64, JournalEvent)> = (0..100)
            .map(|i| (i * 1_000_000, JournalEvent::TradeReceived { trade_id: i }))
            .collect();
        let journal = make_journal(&events);
        let log = ReplayVerifier::build_replay_log(&journal);
        assert_eq!(log.len(), 100);

        // Self-verify
        let result = ReplayVerifier::verify(&log, &log);
        assert!(result.success);
        assert_eq!(result.steps_verified, 100);
    }

    #[test]
    fn verify_both_empty_logs_succeed() {
        let result = ReplayVerifier::verify(&[], &[]);
        assert!(result.success);
        assert_eq!(result.steps_verified, 0);
        assert!(result.discrepancies.is_empty());
    }

    #[test]
    fn verify_expected_empty_actual_nonempty() {
        let j = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let log = ReplayVerifier::build_replay_log(&j);
        let result = ReplayVerifier::verify(&[], &log);
        assert!(!result.success);
        assert!(!result.discrepancies.is_empty());
    }

    #[test]
    fn step_hash_differs_for_different_sequences() {
        let j1 = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        // Manually build a journal that starts at a different sequence by
        // inserting a dummy entry first.
        let j2 = make_journal(&[
            (50, JournalEvent::TradeReceived { trade_id: 0 }),
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
        ]);
        let log1 = ReplayVerifier::build_replay_log(&j1);
        let log2 = ReplayVerifier::build_replay_log(&j2);
        // The TradeReceived{trade_id:1} entry is at seq 1 in log1 and seq 2 in log2.
        assert_ne!(log1[0].content_hash, log2[1].content_hash);
    }

    #[test]
    fn event_kind_bytes_cover_all_variants() {
        let kinds = [
            ReplayVerifier::event_kind_byte(&JournalEvent::TradeReceived { trade_id: 0 }),
            ReplayVerifier::event_kind_byte(&JournalEvent::NettingCompleted {
                obligation_count: 0,
            }),
            ReplayVerifier::event_kind_byte(&JournalEvent::ClearingAttempted {
                obligation_count: 0,
                success_count: 0,
                fail_count: 0,
            }),
            ReplayVerifier::event_kind_byte(&JournalEvent::SettlementCompleted { trade_count: 0 }),
            ReplayVerifier::event_kind_byte(&JournalEvent::SettlementFailed {
                trade_id: 0,
                reason: String::new(),
            }),
        ];
        // Must be exactly 0..4
        let mut sorted = kinds;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn journal_hash_empty_is_fnv_basis() {
        let journal = SettlementJournal::new();
        let h = ReplayVerifier::compute_journal_hash(&journal);
        assert_eq!(h, 0xcbf29ce484222325u64);
    }

    #[test]
    fn journal_hash_order_matters() {
        // Same events in different order produce different hashes.
        let j1 = make_journal(&[
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
            (
                200,
                JournalEvent::NettingCompleted {
                    obligation_count: 3,
                },
            ),
        ]);
        let j2 = make_journal(&[
            (
                200,
                JournalEvent::NettingCompleted {
                    obligation_count: 3,
                },
            ),
            (100, JournalEvent::TradeReceived { trade_id: 1 }),
        ]);
        let h1 = ReplayVerifier::compute_journal_hash(&j1);
        let h2 = ReplayVerifier::compute_journal_hash(&j2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn replay_result_content_hash_differs_on_failure() {
        let j_ok = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 1 })]);
        let j_bad = make_journal(&[(100, JournalEvent::TradeReceived { trade_id: 999 })]);
        let log_ok = ReplayVerifier::build_replay_log(&j_ok);
        let log_bad = ReplayVerifier::build_replay_log(&j_bad);

        let r_match = ReplayVerifier::verify(&log_ok, &log_ok);
        let r_mismatch = ReplayVerifier::verify(&log_ok, &log_bad);
        // A match and a mismatch produce different result hashes.
        assert_ne!(r_match.content_hash, r_mismatch.content_hash);
    }
}
