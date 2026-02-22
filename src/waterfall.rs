// ALICE-Settlement — Default waterfall cascade for loss absorption
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Moroya Sakamoto

use crate::fnv1a;

// ── Types ──────────────────────────────────────────────────────────────

/// The five layers of the default waterfall, applied in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WaterfallLayer {
    /// Layer 1: Defaulter's own margin deposit.
    DefaulterMargin = 0,
    /// Layer 2: Defaulter's contribution to the default fund.
    DefaulterFund = 1,
    /// Layer 3: CCP's first-loss capital (skin-in-the-game).
    CcpFirstLoss = 2,
    /// Layer 4: Non-defaulting members' default fund contributions.
    MembersFund = 3,
    /// Layer 5: CCP's remaining capital.
    CcpCapital = 4,
}

/// Per-layer absorption result.
#[derive(Debug, Clone)]
pub struct LayerAbsorption {
    /// Which waterfall layer.
    pub layer: WaterfallLayer,
    /// Total capacity of this layer.
    pub capacity: i64,
    /// Amount absorbed by this layer.
    pub absorbed: i64,
    /// Loss remaining after this layer.
    pub remaining_after: i64,
}

/// Configuration for the default waterfall.
///
/// Each field represents the capacity (in ticks) available at that layer.
#[derive(Debug, Clone)]
pub struct WaterfallConfig {
    pub defaulter_margin: i64,
    pub defaulter_fund: i64,
    pub ccp_first_loss: i64,
    pub members_fund: i64,
    pub ccp_capital: i64,
}

impl Default for WaterfallConfig {
    fn default() -> Self {
        Self {
            defaulter_margin: 10_000,
            defaulter_fund: 5_000,
            ccp_first_loss: 2_000,
            members_fund: 20_000,
            ccp_capital: 50_000,
        }
    }
}

/// Result of running a loss through the waterfall.
#[derive(Debug, Clone)]
pub struct WaterfallResult {
    /// Total loss presented to the waterfall.
    pub total_loss: i64,
    /// Total amount absorbed across all layers.
    pub total_absorbed: i64,
    /// Per-layer absorption details (always 5 entries).
    pub layers: Vec<LayerAbsorption>,
    /// True if the entire loss was covered.
    pub fully_covered: bool,
    /// Unabsorbed loss (zero if fully covered).
    pub shortfall: i64,
    /// Deterministic content hash.
    pub content_hash: u64,
}

// ── Default Waterfall ──────────────────────────────────────────────────

/// Five-layer default waterfall for loss absorption.
///
/// When a clearing member defaults, losses are absorbed sequentially
/// through five layers of capital, each exhausted before the next is
/// tapped.  This follows the standard CCP loss waterfall structure.
pub struct DefaultWaterfall {
    config: WaterfallConfig,
}

impl DefaultWaterfall {
    /// Create a new waterfall with the given layer capacities.
    pub fn new(config: WaterfallConfig) -> Self {
        Self { config }
    }

    /// Absorb a loss through the waterfall layers in order.
    ///
    /// Returns a detailed result showing how much each layer absorbed.
    pub fn absorb_loss(&self, loss: i64) -> WaterfallResult {
        if loss <= 0 {
            return self.zero_result(loss);
        }

        let capacities = [
            (
                WaterfallLayer::DefaulterMargin,
                self.config.defaulter_margin,
            ),
            (WaterfallLayer::DefaulterFund, self.config.defaulter_fund),
            (WaterfallLayer::CcpFirstLoss, self.config.ccp_first_loss),
            (WaterfallLayer::MembersFund, self.config.members_fund),
            (WaterfallLayer::CcpCapital, self.config.ccp_capital),
        ];

        let mut remaining = loss;
        let mut layers = Vec::with_capacity(5);

        for (layer, capacity) in capacities {
            // Branchless min: absorbed = min(remaining, capacity)
            let absorbed = if remaining <= capacity {
                remaining
            } else {
                capacity
            };
            remaining -= absorbed;
            layers.push(LayerAbsorption {
                layer,
                capacity,
                absorbed,
                remaining_after: remaining,
            });
        }

        let total_absorbed = loss - remaining;
        let fully_covered = remaining == 0;

        WaterfallResult {
            total_loss: loss,
            total_absorbed,
            layers,
            fully_covered,
            shortfall: remaining,
            content_hash: Self::compute_hash(loss, total_absorbed),
        }
    }

    /// Absorb multiple independent losses, returning individual results.
    pub fn absorb_losses(&self, losses: &[i64]) -> Vec<WaterfallResult> {
        losses.iter().map(|&l| self.absorb_loss(l)).collect()
    }

    /// Total capacity across all waterfall layers.
    #[inline]
    pub fn total_capacity(&self) -> i64 {
        self.config
            .defaulter_margin
            .saturating_add(self.config.defaulter_fund)
            .saturating_add(self.config.ccp_first_loss)
            .saturating_add(self.config.members_fund)
            .saturating_add(self.config.ccp_capital)
    }

    /// Access the current configuration.
    #[inline]
    pub fn config(&self) -> &WaterfallConfig {
        &self.config
    }

    fn zero_result(&self, loss: i64) -> WaterfallResult {
        let capacities = [
            (
                WaterfallLayer::DefaulterMargin,
                self.config.defaulter_margin,
            ),
            (WaterfallLayer::DefaulterFund, self.config.defaulter_fund),
            (WaterfallLayer::CcpFirstLoss, self.config.ccp_first_loss),
            (WaterfallLayer::MembersFund, self.config.members_fund),
            (WaterfallLayer::CcpCapital, self.config.ccp_capital),
        ];
        WaterfallResult {
            total_loss: loss,
            total_absorbed: 0,
            layers: capacities
                .iter()
                .map(|&(layer, capacity)| LayerAbsorption {
                    layer,
                    capacity,
                    absorbed: 0,
                    remaining_after: 0,
                })
                .collect(),
            fully_covered: true,
            shortfall: 0,
            content_hash: Self::compute_hash(loss, 0),
        }
    }

    fn compute_hash(loss: i64, absorbed: i64) -> u64 {
        let mut data = [0u8; 16];
        data[0..8].copy_from_slice(&loss.to_le_bytes());
        data[8..16].copy_from_slice(&absorbed.to_le_bytes());
        fnv1a(&data)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_waterfall() -> DefaultWaterfall {
        DefaultWaterfall::new(WaterfallConfig::default())
    }

    fn small_waterfall() -> DefaultWaterfall {
        DefaultWaterfall::new(WaterfallConfig {
            defaulter_margin: 100,
            defaulter_fund: 50,
            ccp_first_loss: 30,
            members_fund: 200,
            ccp_capital: 500,
        })
    }

    #[test]
    fn zero_loss() {
        let wf = default_waterfall();
        let result = wf.absorb_loss(0);
        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 0);
        assert_eq!(result.shortfall, 0);
        assert_eq!(result.layers.len(), 5);
    }

    #[test]
    fn negative_loss() {
        let wf = default_waterfall();
        let result = wf.absorb_loss(-100);
        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 0);
    }

    #[test]
    fn loss_fully_covered_by_first_layer() {
        let wf = small_waterfall(); // margin = 100
        let result = wf.absorb_loss(80);

        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 80);
        assert_eq!(result.shortfall, 0);

        assert_eq!(result.layers[0].layer, WaterfallLayer::DefaulterMargin);
        assert_eq!(result.layers[0].absorbed, 80);
        assert_eq!(result.layers[0].remaining_after, 0);

        // Other layers absorb nothing
        for i in 1..5 {
            assert_eq!(result.layers[i].absorbed, 0);
        }
    }

    #[test]
    fn loss_spans_two_layers() {
        let wf = small_waterfall(); // margin=100, fund=50
        let result = wf.absorb_loss(120);

        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 120);

        assert_eq!(result.layers[0].absorbed, 100); // full margin
        assert_eq!(result.layers[0].remaining_after, 20);
        assert_eq!(result.layers[1].absorbed, 20); // partial fund
        assert_eq!(result.layers[1].remaining_after, 0);
    }

    #[test]
    fn loss_spans_three_layers() {
        let wf = small_waterfall(); // margin=100, fund=50, ccp_first=30
        let result = wf.absorb_loss(170);

        assert!(result.fully_covered);
        assert_eq!(result.layers[0].absorbed, 100);
        assert_eq!(result.layers[1].absorbed, 50);
        assert_eq!(result.layers[2].absorbed, 20); // 170 - 100 - 50 = 20
        assert_eq!(result.layers[2].remaining_after, 0);
    }

    #[test]
    fn loss_spans_all_five_layers() {
        let wf = small_waterfall(); // total capacity = 880
        let result = wf.absorb_loss(800);

        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 800);

        assert_eq!(result.layers[0].absorbed, 100); // margin
        assert_eq!(result.layers[1].absorbed, 50); // fund
        assert_eq!(result.layers[2].absorbed, 30); // ccp first loss
        assert_eq!(result.layers[3].absorbed, 200); // members fund
        assert_eq!(result.layers[4].absorbed, 420); // ccp capital (partial)
    }

    #[test]
    fn loss_exceeds_all_layers() {
        let wf = small_waterfall(); // total = 880
        let result = wf.absorb_loss(1_000);

        assert!(!result.fully_covered);
        assert_eq!(result.total_absorbed, 880);
        assert_eq!(result.shortfall, 120);

        assert_eq!(result.layers[4].absorbed, 500); // full ccp capital
        assert_eq!(result.layers[4].remaining_after, 120); // shortfall
    }

    #[test]
    fn loss_exactly_equals_total_capacity() {
        let wf = small_waterfall(); // total = 880
        let result = wf.absorb_loss(880);

        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 880);
        assert_eq!(result.shortfall, 0);
        assert_eq!(result.layers[4].remaining_after, 0);
    }

    #[test]
    fn total_capacity_computed() {
        let wf = small_waterfall();
        assert_eq!(wf.total_capacity(), 880); // 100+50+30+200+500
    }

    #[test]
    fn layer_ordering_is_correct() {
        let wf = default_waterfall();
        let result = wf.absorb_loss(1);
        assert_eq!(result.layers[0].layer, WaterfallLayer::DefaulterMargin);
        assert_eq!(result.layers[1].layer, WaterfallLayer::DefaulterFund);
        assert_eq!(result.layers[2].layer, WaterfallLayer::CcpFirstLoss);
        assert_eq!(result.layers[3].layer, WaterfallLayer::MembersFund);
        assert_eq!(result.layers[4].layer, WaterfallLayer::CcpCapital);
    }

    #[test]
    fn absorb_losses_batch() {
        let wf = small_waterfall();
        let results = wf.absorb_losses(&[50, 200, 1000]);
        assert_eq!(results.len(), 3);
        assert!(results[0].fully_covered);
        assert!(results[1].fully_covered);
        assert!(!results[2].fully_covered);
    }

    #[test]
    fn content_hash_deterministic() {
        let wf = default_waterfall();
        let r1 = wf.absorb_loss(5_000);
        let r2 = wf.absorb_loss(5_000);
        assert_eq!(r1.content_hash, r2.content_hash);
        assert_ne!(r1.content_hash, 0);
    }

    #[test]
    fn content_hash_varies_with_loss() {
        let wf = default_waterfall();
        let r1 = wf.absorb_loss(1_000);
        let r2 = wf.absorb_loss(2_000);
        assert_ne!(r1.content_hash, r2.content_hash);
    }

    #[test]
    fn single_tick_loss() {
        let wf = default_waterfall();
        let result = wf.absorb_loss(1);
        assert!(result.fully_covered);
        assert_eq!(result.total_absorbed, 1);
        assert_eq!(result.layers[0].absorbed, 1);
    }

    #[test]
    fn waterfall_layer_repr_values() {
        assert_eq!(WaterfallLayer::DefaulterMargin as u8, 0);
        assert_eq!(WaterfallLayer::DefaulterFund as u8, 1);
        assert_eq!(WaterfallLayer::CcpFirstLoss as u8, 2);
        assert_eq!(WaterfallLayer::MembersFund as u8, 3);
        assert_eq!(WaterfallLayer::CcpCapital as u8, 4);
    }

    #[test]
    fn waterfall_layer_equality() {
        assert_eq!(
            WaterfallLayer::DefaulterMargin,
            WaterfallLayer::DefaulterMargin
        );
        assert_ne!(WaterfallLayer::DefaulterMargin, WaterfallLayer::CcpCapital);
    }

    #[test]
    fn absorb_losses_empty_slice() {
        let wf = default_waterfall();
        let results = wf.absorb_losses(&[]);
        assert!(results.is_empty());
    }

    #[test]
    fn absorb_losses_all_zero() {
        let wf = default_waterfall();
        let results = wf.absorb_losses(&[0, 0, 0]);
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.fully_covered);
            assert_eq!(r.total_absorbed, 0);
            assert_eq!(r.shortfall, 0);
        }
    }

    #[test]
    fn content_hash_differs_for_different_losses() {
        let wf = default_waterfall();
        let r1 = wf.absorb_loss(100);
        let r2 = wf.absorb_loss(101);
        assert_ne!(r1.content_hash, r2.content_hash);
    }

    #[test]
    fn total_capacity_default_config() {
        let wf = default_waterfall();
        // defaulter_margin=10000, fund=5000, ccp=2000, members=20000, ccp_cap=50000
        assert_eq!(wf.total_capacity(), 87_000);
    }

    #[test]
    fn config_accessor_returns_original_values() {
        let cfg = WaterfallConfig {
            defaulter_margin: 111,
            defaulter_fund: 222,
            ccp_first_loss: 333,
            members_fund: 444,
            ccp_capital: 555,
        };
        let wf = DefaultWaterfall::new(cfg.clone());
        let got = wf.config();
        assert_eq!(got.defaulter_margin, 111);
        assert_eq!(got.defaulter_fund, 222);
        assert_eq!(got.ccp_first_loss, 333);
        assert_eq!(got.members_fund, 444);
        assert_eq!(got.ccp_capital, 555);
    }

    #[test]
    fn layer_remaining_after_decreases_monotonically() {
        let wf = small_waterfall();
        let result = wf.absorb_loss(700); // spans several layers
        let mut prev = result.total_loss;
        for layer in &result.layers {
            assert!(layer.remaining_after <= prev);
            prev = layer.remaining_after;
        }
    }

    #[test]
    fn zero_capacity_waterfall_shortfall_equals_loss() {
        let wf = DefaultWaterfall::new(WaterfallConfig {
            defaulter_margin: 0,
            defaulter_fund: 0,
            ccp_first_loss: 0,
            members_fund: 0,
            ccp_capital: 0,
        });
        let result = wf.absorb_loss(9_999);
        assert!(!result.fully_covered);
        assert_eq!(result.shortfall, 9_999);
        assert_eq!(result.total_absorbed, 0);
    }

    #[test]
    fn absorb_loss_all_layers_have_correct_capacity_field() {
        let wf = small_waterfall();
        let result = wf.absorb_loss(1);
        assert_eq!(result.layers[0].capacity, 100);
        assert_eq!(result.layers[1].capacity, 50);
        assert_eq!(result.layers[2].capacity, 30);
        assert_eq!(result.layers[3].capacity, 200);
        assert_eq!(result.layers[4].capacity, 500);
    }
}
