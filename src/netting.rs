/*
    ALICE-Settlement
    Copyright (C) 2026 Moroya Sakamoto
*/

use std::collections::{HashMap, HashSet};

use crate::trade::Trade;

/// Net obligation between two counterparties for a single symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetObligation {
    /// Symbol hash.
    pub symbol_hash: u64,
    /// Account that owes delivery (net seller).
    pub deliverer_id: u64,
    /// Account that receives delivery (net buyer).
    pub receiver_id: u64,
    /// Net quantity to deliver.
    pub net_quantity: u64,
    /// Net payment amount (price * quantity sum).
    pub net_payment: i64,
    /// Number of original trades netted into this obligation.
    pub trade_count: u32,
}

/// Key for grouping bilateral trade flows per symbol.
/// Always stored as (min_id, max_id) to unify both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NettingKey {
    symbol_hash: u64,
    lo_id: u64,
    hi_id: u64,
}

/// Per-key accumulator tracking net position.
#[derive(Debug, Default)]
struct NettingAccumulator {
    /// Signed quantity: positive means lo_id is net buyer from hi_id.
    /// Each buy by lo_id adds qty; each sell by lo_id subtracts qty.
    net_quantity_signed: i128,
    /// Net payment signed (positive means lo_id pays hi_id).
    net_payment_signed: i128,
    trade_count: u32,
}

/// Bilateral netting engine.
///
/// Accumulates trades within a netting cycle, then computes net obligations
/// across all counterparty pairs. Supports multi-symbol and multi-party netting.
pub struct NettingEngine {
    accumulators: HashMap<NettingKey, NettingAccumulator>,
}

impl NettingEngine {
    /// Create a new, empty netting engine.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            accumulators: HashMap::new(),
        }
    }

    /// Accumulate a trade into the netting state.
    ///
    /// For the canonical (lo_id, hi_id) pair, a trade where lo_id is buyer
    /// adds to the signed quantity; a trade where lo_id is seller subtracts.
    pub fn add_trade(&mut self, trade: &Trade) {
        let (lo_id, hi_id) = canonical_pair(trade.buyer_id, trade.seller_id);
        let key = NettingKey {
            symbol_hash: trade.symbol_hash,
            lo_id,
            hi_id,
        };

        let acc = self.accumulators.entry(key).or_default();
        acc.trade_count += 1;

        let qty = trade.quantity as i128;
        let payment = (trade.price as i128) * qty;

        if trade.buyer_id == lo_id {
            // lo_id is buying: positive direction
            acc.net_quantity_signed += qty;
            acc.net_payment_signed += payment;
        } else {
            // lo_id is selling: negative direction
            acc.net_quantity_signed -= qty;
            acc.net_payment_signed -= payment;
        }
    }

    /// Compute all bilateral net obligations from accumulated trades.
    ///
    /// Returns one `NetObligation` per (symbol, counterparty-pair) where the
    /// net quantity is non-zero. Pairs with perfectly offsetting trades produce
    /// no obligation.
    pub fn compute_net(&self) -> Vec<NetObligation> {
        let mut obligations = Vec::with_capacity(self.accumulators.len());

        for (key, acc) in &self.accumulators {
            if acc.net_quantity_signed == 0 {
                continue;
            }

            // Determine delivery direction from sign of net quantity.
            // Positive: lo_id is net buyer, hi_id is net deliverer.
            // Negative: hi_id is net buyer, lo_id is net deliverer.
            let (deliverer_id, receiver_id, net_quantity, net_payment) =
                if acc.net_quantity_signed > 0 {
                    (
                        key.hi_id,
                        key.lo_id,
                        acc.net_quantity_signed as u64,
                        saturating_i128_to_i64(acc.net_payment_signed),
                    )
                } else {
                    (
                        key.lo_id,
                        key.hi_id,
                        (-acc.net_quantity_signed) as u64,
                        saturating_i128_to_i64(-acc.net_payment_signed),
                    )
                };

            obligations.push(NetObligation {
                symbol_hash: key.symbol_hash,
                deliverer_id,
                receiver_id,
                net_quantity,
                net_payment,
                trade_count: acc.trade_count,
            });
        }

        obligations
    }

    /// Reset the engine for the next netting cycle.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.accumulators.clear();
    }
}

impl Default for NettingEngine {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// Return the canonical (lo, hi) ordering of a counterparty pair.
#[inline(always)]
fn canonical_pair(a: u64, b: u64) -> (u64, u64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Clamp an i128 value into i64 range.
#[inline(always)]
fn saturating_i128_to_i64(v: i128) -> i64 {
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::SettlementStatus;

    fn make_trade(
        trade_id: u64,
        symbol_hash: u64,
        buyer_id: u64,
        seller_id: u64,
        price: i64,
        quantity: u64,
    ) -> Trade {
        Trade {
            trade_id,
            symbol_hash,
            buyer_id,
            seller_id,
            price,
            quantity,
            timestamp_ns: 0,
            status: SettlementStatus::Pending,
        }
    }

    #[test]
    fn test_empty_netting() {
        let engine = NettingEngine::new();
        let obligations = engine.compute_net();
        assert!(obligations.is_empty());
    }

    #[test]
    fn test_single_trade_netting() {
        let mut engine = NettingEngine::new();
        let trade = make_trade(1, 0xABCD, 100, 200, 500, 10);
        engine.add_trade(&trade);

        let obligations = engine.compute_net();
        assert_eq!(obligations.len(), 1);

        let ob = &obligations[0];
        assert_eq!(ob.symbol_hash, 0xABCD);
        // Buyer 100 receives, Seller 200 delivers
        assert_eq!(ob.receiver_id, 100);
        assert_eq!(ob.deliverer_id, 200);
        assert_eq!(ob.net_quantity, 10);
        assert_eq!(ob.net_payment, 5_000); // 500 * 10
        assert_eq!(ob.trade_count, 1);
    }

    #[test]
    fn test_bilateral_netting() {
        // A (100) buys 100 from B (200), then B (200) buys 30 from A (100).
        // Net: A buys 70 from B.
        let mut engine = NettingEngine::new();

        let t1 = make_trade(1, 0xABCD, 100, 200, 100, 100); // A buys 100 @ 100
        let t2 = make_trade(2, 0xABCD, 200, 100, 120, 30);  // B buys 30 @ 120

        engine.add_trade(&t1);
        engine.add_trade(&t2);

        let obligations = engine.compute_net();
        assert_eq!(obligations.len(), 1);

        let ob = &obligations[0];
        assert_eq!(ob.symbol_hash, 0xABCD);
        // Net: A (100) is net buyer of 70, B (200) is net deliverer
        assert_eq!(ob.receiver_id, 100);
        assert_eq!(ob.deliverer_id, 200);
        assert_eq!(ob.net_quantity, 70);
        // net_payment = 100*100 - 120*30 = 10000 - 3600 = 6400
        assert_eq!(ob.net_payment, 6_400);
        assert_eq!(ob.trade_count, 2);
    }

    #[test]
    fn test_multi_symbol_netting() {
        // Same counterparties, different symbols produce separate obligations.
        let mut engine = NettingEngine::new();

        let t1 = make_trade(1, 0x0001, 100, 200, 100, 5);
        let t2 = make_trade(2, 0x0002, 100, 200, 200, 3);

        engine.add_trade(&t1);
        engine.add_trade(&t2);

        let obligations = engine.compute_net();
        assert_eq!(obligations.len(), 2);

        // Both obligations: A (100) buys from B (200)
        for ob in &obligations {
            assert_eq!(ob.receiver_id, 100);
            assert_eq!(ob.deliverer_id, 200);
        }

        let sym1 = obligations.iter().find(|o| o.symbol_hash == 0x0001).unwrap();
        assert_eq!(sym1.net_quantity, 5);
        assert_eq!(sym1.net_payment, 500);

        let sym2 = obligations.iter().find(|o| o.symbol_hash == 0x0002).unwrap();
        assert_eq!(sym2.net_quantity, 3);
        assert_eq!(sym2.net_payment, 600);
    }

    #[test]
    fn test_three_party_netting() {
        // A(100)↔B(200), B(200)↔C(300), A(100)↔C(300) — 3 separate obligations.
        let mut engine = NettingEngine::new();

        let t1 = make_trade(1, 0xFFFF, 100, 200, 50, 10); // A buys 10 from B
        let t2 = make_trade(2, 0xFFFF, 200, 300, 60, 20); // B buys 20 from C
        let t3 = make_trade(3, 0xFFFF, 100, 300, 55, 15); // A buys 15 from C

        engine.add_trade(&t1);
        engine.add_trade(&t2);
        engine.add_trade(&t3);

        let obligations = engine.compute_net();
        assert_eq!(obligations.len(), 3);

        // A↔B: A buys 10 from B
        let ab = obligations
            .iter()
            .find(|o| {
                (o.receiver_id == 100 && o.deliverer_id == 200)
                    || (o.receiver_id == 200 && o.deliverer_id == 100)
            })
            .unwrap();
        assert_eq!(ab.receiver_id, 100);
        assert_eq!(ab.deliverer_id, 200);
        assert_eq!(ab.net_quantity, 10);

        // B↔C: B buys 20 from C
        let bc = obligations
            .iter()
            .find(|o| {
                (o.receiver_id == 200 && o.deliverer_id == 300)
                    || (o.receiver_id == 300 && o.deliverer_id == 200)
            })
            .unwrap();
        assert_eq!(bc.receiver_id, 200);
        assert_eq!(bc.deliverer_id, 300);
        assert_eq!(bc.net_quantity, 20);

        // A↔C: A buys 15 from C
        let ac = obligations
            .iter()
            .find(|o| {
                (o.receiver_id == 100 && o.deliverer_id == 300)
                    || (o.receiver_id == 300 && o.deliverer_id == 100)
            })
            .unwrap();
        assert_eq!(ac.receiver_id, 100);
        assert_eq!(ac.deliverer_id, 300);
        assert_eq!(ac.net_quantity, 15);
    }
}
