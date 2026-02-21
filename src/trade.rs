/*
    ALICE-Settlement
    Copyright (C) 2026 Moroya Sakamoto
*/

/// A confirmed trade between two counterparties, derived from matching fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    /// Unique trade identifier.
    pub trade_id: u64,
    /// Symbol hash (FNV-derived).
    pub symbol_hash: u64,
    /// Buyer account identifier.
    pub buyer_id: u64,
    /// Seller account identifier.
    pub seller_id: u64,
    /// Execution price in ticks.
    pub price: i64,
    /// Trade quantity in lots.
    pub quantity: u64,
    /// Execution timestamp (nanoseconds since Unix epoch).
    pub timestamp_ns: u64,
    /// Settlement status.
    pub status: SettlementStatus,
}

/// Settlement lifecycle state for a trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementStatus {
    /// Trade confirmed, awaiting settlement.
    Pending,
    /// Netting applied, awaiting clearing.
    Netted,
    /// Clearing house has accepted.
    Cleared,
    /// Final settlement complete.
    Settled,
    /// Settlement failed (insufficient funds, etc.).
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_creation() {
        let trade = Trade {
            trade_id: 1,
            symbol_hash: 0xdeadbeef,
            buyer_id: 100,
            seller_id: 200,
            price: 50_000,
            quantity: 10,
            timestamp_ns: 1_700_000_000_000_000_000,
            status: SettlementStatus::Pending,
        };

        assert_eq!(trade.trade_id, 1);
        assert_eq!(trade.symbol_hash, 0xdeadbeef);
        assert_eq!(trade.buyer_id, 100);
        assert_eq!(trade.seller_id, 200);
        assert_eq!(trade.price, 50_000);
        assert_eq!(trade.quantity, 10);
        assert_eq!(trade.status, SettlementStatus::Pending);
    }

    #[test]
    fn test_settlement_status_transitions() {
        let mut trade = Trade {
            trade_id: 2,
            symbol_hash: 0xabcd,
            buyer_id: 10,
            seller_id: 20,
            price: 1_000,
            quantity: 5,
            timestamp_ns: 0,
            status: SettlementStatus::Pending,
        };

        assert_eq!(trade.status, SettlementStatus::Pending);

        trade.status = SettlementStatus::Netted;
        assert_eq!(trade.status, SettlementStatus::Netted);

        trade.status = SettlementStatus::Cleared;
        assert_eq!(trade.status, SettlementStatus::Cleared);

        trade.status = SettlementStatus::Settled;
        assert_eq!(trade.status, SettlementStatus::Settled);

        // Test failed path
        let mut failed_trade = Trade {
            trade_id: 3,
            symbol_hash: 0xabcd,
            buyer_id: 10,
            seller_id: 20,
            price: 1_000,
            quantity: 5,
            timestamp_ns: 0,
            status: SettlementStatus::Pending,
        };
        failed_trade.status = SettlementStatus::Failed;
        assert_eq!(failed_trade.status, SettlementStatus::Failed);

        // Status values must be distinct
        assert_ne!(SettlementStatus::Pending, SettlementStatus::Netted);
        assert_ne!(SettlementStatus::Cleared, SettlementStatus::Settled);
        assert_ne!(SettlementStatus::Settled, SettlementStatus::Failed);
    }
}
