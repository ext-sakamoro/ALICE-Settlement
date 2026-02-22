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

    /// Compute bilateral obligations, then reduce them via multilateral
    /// cycle cancellation.
    ///
    /// The resulting obligations are a strict subset of the bilateral net,
    /// with reduced gross exposure where circular flows exist.
    pub fn compute_multilateral(&self) -> Vec<NetObligation> {
        multilateral_net(self.compute_net())
    }
}

impl Default for NettingEngine {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

// ── Multilateral Netting ───────────────────────────────────────────────

/// Reduce bilateral obligations via cycle cancellation.
///
/// Given obligations A→B, B→C, C→A (a cycle within a single symbol),
/// the minimum edge weight is subtracted from all edges in the cycle,
/// reducing total gross exposure while preserving settlement correctness.
///
/// Obligations are grouped by `symbol_hash`; cycles are only cancelled
/// within the same symbol.
pub fn multilateral_net(obligations: Vec<NetObligation>) -> Vec<NetObligation> {
    // Group by symbol
    let mut by_symbol: HashMap<u64, Vec<NetObligation>> = HashMap::new();
    for ob in obligations {
        by_symbol.entry(ob.symbol_hash).or_default().push(ob);
    }

    let mut result = Vec::new();

    for (_symbol, mut obs) in by_symbol {
        // Repeatedly find and cancel cycles until none remain
        loop {
            match find_cycle(&obs) {
                Some(cycle_indices) => cancel_cycle(&mut obs, &cycle_indices),
                None => break,
            }
        }
        // Remove obligations reduced to zero
        obs.retain(|ob| ob.net_quantity > 0);
        result.extend(obs);
    }

    result
}

/// Find a cycle in the obligation graph (directed: deliverer → receiver).
///
/// Returns the indices into `obs` that form a cycle, or `None` if the
/// graph is acyclic.
fn find_cycle(obs: &[NetObligation]) -> Option<Vec<usize>> {
    // Build adjacency: deliverer_id → [(receiver_id, obligation_index)]
    let mut adj: HashMap<u64, Vec<(u64, usize)>> = HashMap::new();
    for (i, ob) in obs.iter().enumerate() {
        if ob.net_quantity > 0 {
            adj.entry(ob.deliverer_id)
                .or_default()
                .push((ob.receiver_id, i));
        }
    }

    // Collect all nodes that have outgoing edges
    let starts: Vec<u64> = adj.keys().copied().collect();

    for start in starts {
        // DFS: try to find a path from `start` back to `start`
        let mut visited = HashSet::new();
        let mut path_edges = Vec::new();

        if dfs_find_cycle(&adj, start, start, &mut visited, &mut path_edges, true) {
            return Some(path_edges);
        }
    }

    None
}

/// Recursive DFS looking for a path from `current` back to `target`.
fn dfs_find_cycle(
    adj: &HashMap<u64, Vec<(u64, usize)>>,
    current: u64,
    target: u64,
    visited: &mut HashSet<u64>,
    path_edges: &mut Vec<usize>,
    is_start: bool,
) -> bool {
    if !is_start && current == target {
        return true;
    }
    if visited.contains(&current) {
        return false;
    }
    visited.insert(current);

    if let Some(edges) = adj.get(&current) {
        for &(next, ob_idx) in edges {
            path_edges.push(ob_idx);
            if dfs_find_cycle(adj, next, target, visited, path_edges, false) {
                return true;
            }
            path_edges.pop();
        }
    }

    visited.remove(&current);
    false
}

/// Cancel a cycle by subtracting the minimum edge weight.
///
/// Payment is reduced proportionally to preserve the average price per unit.
fn cancel_cycle(obs: &mut [NetObligation], cycle_indices: &[usize]) {
    // Find minimum quantity in the cycle
    let min_qty = cycle_indices
        .iter()
        .map(|&i| obs[i].net_quantity)
        .min()
        .unwrap_or(0);

    if min_qty == 0 {
        return;
    }

    // Reduce each edge in the cycle
    for &i in cycle_indices {
        let ob = &mut obs[i];
        let original_qty = ob.net_quantity;
        ob.net_quantity -= min_qty;
        // Proportional payment reduction (avoiding division: multiply first)
        if original_qty > 0 {
            let payment_reduction =
                (ob.net_payment as i128 * min_qty as i128 / original_qty as i128) as i64;
            ob.net_payment -= payment_reduction;
        }
    }
}

/// Return the canonical (lo, hi) ordering of a counterparty pair.
#[inline(always)]
fn canonical_pair(a: u64, b: u64) -> (u64, u64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
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
        let t2 = make_trade(2, 0xABCD, 200, 100, 120, 30); // B buys 30 @ 120

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

        let sym1 = obligations
            .iter()
            .find(|o| o.symbol_hash == 0x0001)
            .unwrap();
        assert_eq!(sym1.net_quantity, 5);
        assert_eq!(sym1.net_payment, 500);

        let sym2 = obligations
            .iter()
            .find(|o| o.symbol_hash == 0x0002)
            .unwrap();
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

    // ── Multilateral Netting Tests ────────────────────────────────────

    #[test]
    fn test_multilateral_no_cycle() {
        // A→B, B→C — no cycle, obligations unchanged
        let obs = vec![
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 100,
                receiver_id: 200,
                net_quantity: 10,
                net_payment: 1_000,
                trade_count: 1,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 200,
                receiver_id: 300,
                net_quantity: 5,
                net_payment: 500,
                trade_count: 1,
            },
        ];
        let result = multilateral_net(obs.clone());
        assert_eq!(result.len(), 2);
        // Quantities unchanged (no cycle to cancel)
        let total_qty: u64 = result.iter().map(|o| o.net_quantity).sum();
        assert_eq!(total_qty, 15);
    }

    #[test]
    fn test_multilateral_triangle_cycle() {
        // A→B: 10, B→C: 10, C→A: 10 — perfect triangle cancels entirely
        let obs = vec![
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 100,
                receiver_id: 200,
                net_quantity: 10,
                net_payment: 1_000,
                trade_count: 1,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 200,
                receiver_id: 300,
                net_quantity: 10,
                net_payment: 1_200,
                trade_count: 1,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 300,
                receiver_id: 100,
                net_quantity: 10,
                net_payment: 900,
                trade_count: 1,
            },
        ];
        let result = multilateral_net(obs);
        // All edges cancelled: empty result
        assert!(
            result.is_empty(),
            "perfect triangle should cancel: {:?}",
            result
        );
    }

    #[test]
    fn test_multilateral_partial_cancellation() {
        // A→B: 10, B→C: 8, C→A: 6 — cycle min=6, reduce by 6
        let obs = vec![
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 100,
                receiver_id: 200,
                net_quantity: 10,
                net_payment: 1_000,
                trade_count: 2,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 200,
                receiver_id: 300,
                net_quantity: 8,
                net_payment: 800,
                trade_count: 1,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 300,
                receiver_id: 100,
                net_quantity: 6,
                net_payment: 600,
                trade_count: 1,
            },
        ];

        let result = multilateral_net(obs);
        // C→A: 6 fully cancelled, remaining: A→B: 4, B→C: 2
        let total_qty: u64 = result.iter().map(|o| o.net_quantity).sum();
        assert_eq!(total_qty, 6, "should reduce from 24 to 6");
        assert_eq!(result.len(), 2, "C→A should be fully cancelled");
    }

    #[test]
    fn test_multilateral_preserves_bilateral() {
        // Same test as bilateral: no cycle → identical result
        let mut engine = NettingEngine::new();
        let t1 = make_trade(1, 0xABCD, 100, 200, 100, 50);
        let t2 = make_trade(2, 0xABCD, 200, 100, 120, 20);
        engine.add_trade(&t1);
        engine.add_trade(&t2);

        let bilateral = engine.compute_net();
        let multilateral = engine.compute_multilateral();

        // A↔B only, no cycle → same result
        assert_eq!(bilateral.len(), multilateral.len());
        assert_eq!(bilateral[0].net_quantity, multilateral[0].net_quantity);
    }

    #[test]
    fn test_multilateral_multi_symbol_independent() {
        // Cycle in symbol 0x1 (A→B→C→A), no cycle in symbol 0x2 (A→B)
        let obs = vec![
            // Symbol 0x1 cycle
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 100,
                receiver_id: 200,
                net_quantity: 5,
                net_payment: 500,
                trade_count: 1,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 200,
                receiver_id: 300,
                net_quantity: 5,
                net_payment: 500,
                trade_count: 1,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 300,
                receiver_id: 100,
                net_quantity: 5,
                net_payment: 500,
                trade_count: 1,
            },
            // Symbol 0x2 — no cycle
            NetObligation {
                symbol_hash: 0x2,
                deliverer_id: 100,
                receiver_id: 200,
                net_quantity: 20,
                net_payment: 2_000,
                trade_count: 1,
            },
        ];
        let result = multilateral_net(obs);

        // Symbol 0x1: fully cancelled (triangle)
        let sym1: Vec<&NetObligation> = result.iter().filter(|o| o.symbol_hash == 0x1).collect();
        assert!(sym1.is_empty());

        // Symbol 0x2: unchanged
        let sym2: Vec<&NetObligation> = result.iter().filter(|o| o.symbol_hash == 0x2).collect();
        assert_eq!(sym2.len(), 1);
        assert_eq!(sym2[0].net_quantity, 20);
    }

    #[test]
    fn test_multilateral_empty_input() {
        let result = multilateral_net(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_multilateral_gross_exposure_reduction() {
        // A→B: 100, B→C: 80, C→A: 60
        // Gross before: 100+80+60 = 240
        // After cycle cancel (min=60): A→B: 40, B→C: 20 → gross = 60
        let obs = vec![
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 1,
                receiver_id: 2,
                net_quantity: 100,
                net_payment: 10_000,
                trade_count: 3,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 2,
                receiver_id: 3,
                net_quantity: 80,
                net_payment: 8_000,
                trade_count: 2,
            },
            NetObligation {
                symbol_hash: 0x1,
                deliverer_id: 3,
                receiver_id: 1,
                net_quantity: 60,
                net_payment: 6_000,
                trade_count: 1,
            },
        ];
        let gross_before: u64 = obs.iter().map(|o| o.net_quantity).sum();
        assert_eq!(gross_before, 240);

        let result = multilateral_net(obs);
        let gross_after: u64 = result.iter().map(|o| o.net_quantity).sum();
        assert_eq!(gross_after, 60);
    }

    #[test]
    fn test_netting_engine_clear_resets_state() {
        let mut engine = NettingEngine::new();
        let t = make_trade(1, 0xABCD, 100, 200, 100, 10);
        engine.add_trade(&t);
        assert!(!engine.compute_net().is_empty());

        engine.clear();
        assert!(engine.compute_net().is_empty());
    }

    #[test]
    fn test_netting_engine_default_is_empty() {
        let engine = NettingEngine::default();
        assert!(engine.compute_net().is_empty());
    }

    #[test]
    fn test_perfectly_offsetting_trades_produce_no_obligation() {
        // Same counterparties, same symbol, same quantity in opposite directions.
        let mut engine = NettingEngine::new();
        let t1 = make_trade(1, 0xABCD, 100, 200, 100, 50); // A buys 50
        let t2 = make_trade(2, 0xABCD, 200, 100, 100, 50); // B buys 50 (= A sells 50)
        engine.add_trade(&t1);
        engine.add_trade(&t2);
        let obs = engine.compute_net();
        assert!(obs.is_empty(), "perfect offset should cancel: {:?}", obs);
    }

    #[test]
    fn test_canonical_pair_ordering() {
        // canonical_pair(a, b) == canonical_pair(b, a)
        let (lo1, hi1) = canonical_pair(300, 100);
        let (lo2, hi2) = canonical_pair(100, 300);
        assert_eq!(lo1, lo2);
        assert_eq!(hi1, hi2);
        assert!(lo1 <= hi1);
    }

    #[test]
    fn test_canonical_pair_equal_ids() {
        let (lo, hi) = canonical_pair(42, 42);
        assert_eq!(lo, hi);
        assert_eq!(lo, 42);
    }

    #[test]
    fn test_saturating_i128_to_i64_clamps() {
        let too_big: i128 = (i64::MAX as i128) + 1;
        let too_small: i128 = (i64::MIN as i128) - 1;
        assert_eq!(saturating_i128_to_i64(too_big), i64::MAX);
        assert_eq!(saturating_i128_to_i64(too_small), i64::MIN);
        assert_eq!(saturating_i128_to_i64(42), 42i64);
    }

    #[test]
    fn test_multilateral_single_obligation_no_cycle() {
        let obs = vec![NetObligation {
            symbol_hash: 0x1,
            deliverer_id: 1,
            receiver_id: 2,
            net_quantity: 10,
            net_payment: 1_000,
            trade_count: 1,
        }];
        let result = multilateral_net(obs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].net_quantity, 10);
    }

    #[test]
    fn test_net_obligation_equality() {
        let ob1 = NetObligation {
            symbol_hash: 0xAB,
            deliverer_id: 1,
            receiver_id: 2,
            net_quantity: 5,
            net_payment: 500,
            trade_count: 1,
        };
        let ob2 = ob1.clone();
        assert_eq!(ob1, ob2);
    }

    #[test]
    fn test_netting_accumulates_trade_count() {
        let mut engine = NettingEngine::new();
        let t1 = make_trade(1, 0xABCD, 100, 200, 100, 10);
        let t2 = make_trade(2, 0xABCD, 100, 200, 110, 5);
        let t3 = make_trade(3, 0xABCD, 100, 200, 90, 3);
        engine.add_trade(&t1);
        engine.add_trade(&t2);
        engine.add_trade(&t3);
        let obs = engine.compute_net();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].trade_count, 3);
        // All three trades: A (100) buys from B (200)
        assert_eq!(obs[0].net_quantity, 18); // 10 + 5 + 3
    }
}
