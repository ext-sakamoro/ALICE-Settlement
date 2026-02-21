/*
    ALICE-Settlement
    Copyright (C) 2026 Moroya Sakamoto
*/

/// A settlement journal entry for audit purposes.
#[derive(Debug, Clone)]
pub struct JournalEntry {
    /// Sequential entry number.
    pub sequence: u64,
    /// Timestamp of the journal entry.
    pub timestamp_ns: u64,
    /// Type of event.
    pub event: JournalEvent,
}

/// Events that can be recorded in the settlement journal.
#[derive(Debug, Clone)]
pub enum JournalEvent {
    TradeReceived {
        trade_id: u64,
    },
    NettingCompleted {
        obligation_count: usize,
    },
    ClearingAttempted {
        obligation_count: usize,
        success_count: usize,
        fail_count: usize,
    },
    SettlementCompleted {
        trade_count: usize,
    },
    SettlementFailed {
        trade_id: u64,
        reason: String,
    },
}

/// Append-only settlement journal for audit trail.
///
/// Sequence numbers start at 1 and increment monotonically with each recorded
/// event. The journal never removes entries.
pub struct SettlementJournal {
    entries: Vec<JournalEntry>,
    next_seq: u64,
}

impl SettlementJournal {
    /// Create a new, empty journal. The first recorded entry will have sequence 1.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_seq: 1,
        }
    }

    /// Append an event to the journal.
    pub fn record(&mut self, timestamp_ns: u64, event: JournalEvent) {
        let sequence = self.next_seq;
        self.next_seq += 1;
        self.entries.push(JournalEntry {
            sequence,
            timestamp_ns,
            event,
        });
    }

    /// Return a slice of all journal entries in order.
    #[inline(always)]
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Return the number of entries in the journal.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true when the journal contains no entries.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return a reference to the most recent entry, or `None` if the journal
    /// is empty.
    #[inline(always)]
    pub fn last_entry(&self) -> Option<&JournalEntry> {
        self.entries.last()
    }
}

impl Default for SettlementJournal {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_record() {
        let mut journal = SettlementJournal::new();
        assert!(journal.is_empty());
        assert_eq!(journal.len(), 0);
        assert!(journal.last_entry().is_none());

        journal.record(1_000, JournalEvent::TradeReceived { trade_id: 42 });
        assert_eq!(journal.len(), 1);

        let entry = &journal.entries()[0];
        assert_eq!(entry.sequence, 1);
        assert_eq!(entry.timestamp_ns, 1_000);
        matches!(entry.event, JournalEvent::TradeReceived { trade_id: 42 });

        journal.record(
            2_000,
            JournalEvent::NettingCompleted {
                obligation_count: 3,
            },
        );
        assert_eq!(journal.len(), 2);
    }

    #[test]
    fn test_journal_sequence_increments() {
        let mut journal = SettlementJournal::new();

        for i in 0..10u64 {
            journal.record(
                i * 1_000,
                JournalEvent::TradeReceived { trade_id: i },
            );
        }

        assert_eq!(journal.len(), 10);

        for (idx, entry) in journal.entries().iter().enumerate() {
            // Sequences are 1-based and monotonically increasing
            assert_eq!(entry.sequence, (idx as u64) + 1);
        }

        // Confirm first and last
        assert_eq!(journal.entries()[0].sequence, 1);
        assert_eq!(journal.entries()[9].sequence, 10);
    }

    #[test]
    fn test_journal_last_entry() {
        let mut journal = SettlementJournal::new();
        assert!(journal.last_entry().is_none());

        journal.record(100, JournalEvent::TradeReceived { trade_id: 1 });
        let last = journal.last_entry().unwrap();
        assert_eq!(last.sequence, 1);

        journal.record(
            200,
            JournalEvent::ClearingAttempted {
                obligation_count: 5,
                success_count: 4,
                fail_count: 1,
            },
        );
        let last = journal.last_entry().unwrap();
        assert_eq!(last.sequence, 2);
        assert_eq!(last.timestamp_ns, 200);
        matches!(
            last.event,
            JournalEvent::ClearingAttempted {
                obligation_count: 5,
                success_count: 4,
                fail_count: 1,
            }
        );

        journal.record(
            300,
            JournalEvent::SettlementFailed {
                trade_id: 99,
                reason: "insufficient funds".to_string(),
            },
        );
        let last = journal.last_entry().unwrap();
        assert_eq!(last.sequence, 3);
        assert_eq!(last.timestamp_ns, 300);
    }
}
