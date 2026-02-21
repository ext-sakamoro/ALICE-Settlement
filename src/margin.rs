// ALICE-Settlement — SPAN-style margin computation
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Moroya Sakamoto

use crate::fnv1a;
use crate::netting::NetObligation;

// ── Configuration ──────────────────────────────────────────────────────

/// Configuration for the margin engine.
#[derive(Debug, Clone)]
pub struct MarginConfig {
    /// Initial margin rate as a fraction of notional (e.g. 0.05 = 5%).
    pub initial_margin_rate: f64,
    /// Variation margin rate (fraction of mark-to-market exposure).
    pub variation_margin_rate: f64,
    /// Stress scenario price-shock multipliers
    /// (e.g. 0.85 = -15% shock, 1.15 = +15% shock).
    pub stress_scenarios: Vec<f64>,
    /// Absolute minimum margin floor.
    pub margin_floor: i64,
}

impl Default for MarginConfig {
    fn default() -> Self {
        Self {
            initial_margin_rate: 0.05,
            variation_margin_rate: 1.0,
            stress_scenarios: vec![0.85, 0.90, 0.95, 1.05, 1.10, 1.15],
            margin_floor: 100,
        }
    }
}

// ── Margin Requirement ─────────────────────────────────────────────────

/// Computed margin requirement for an account.
#[derive(Debug, Clone)]
pub struct MarginRequirement {
    /// Account for which margin was computed.
    pub account_id: u64,
    /// Initial margin component (notional × rate).
    pub initial_margin: i64,
    /// Variation margin component (mark-to-market exposure × rate).
    pub variation_margin: i64,
    /// Worst-case stress margin across all configured scenarios.
    pub stress_margin: i64,
    /// Total required margin: max(initial + variation, stress, floor).
    pub total_margin: i64,
    /// Deterministic content hash.
    pub content_hash: u64,
}

// ── Margin Engine ──────────────────────────────────────────────────────

/// SPAN-style margin engine.
///
/// Computes initial, variation, and stress margin requirements based on
/// net obligations.  Stress margin evaluates worst-case exposure across
/// configurable price-shock scenarios.
pub struct MarginEngine {
    config: MarginConfig,
    /// Pre-computed reciprocal: 1.0 / 1.0 (placeholder for future per-symbol
    /// multipliers).  Avoids division in hot path.
    _rcp_one: f64,
}

impl MarginEngine {
    /// Create a new margin engine with the given configuration.
    pub fn new(config: MarginConfig) -> Self {
        Self {
            config,
            _rcp_one: 1.0,
        }
    }

    /// Compute margin for a single obligation from the deliverer's perspective.
    pub fn compute_obligation_margin(&self, obligation: &NetObligation) -> MarginRequirement {
        let notional = obligation.net_payment.unsigned_abs() as i64;

        let initial = (notional as f64 * self.config.initial_margin_rate) as i64;
        let variation = (notional as f64 * self.config.variation_margin_rate) as i64;
        let stress = self.worst_case_stress(notional);

        let base = initial.saturating_add(variation);
        let total = base.max(stress).max(self.config.margin_floor);

        MarginRequirement {
            account_id: obligation.deliverer_id,
            initial_margin: initial,
            variation_margin: variation,
            stress_margin: stress,
            total_margin: total,
            content_hash: Self::hash_requirement(obligation.deliverer_id, total),
        }
    }

    /// Compute portfolio margin for a single account across multiple obligations.
    ///
    /// Obligations where the account is deliverer contribute short exposure;
    /// obligations where the account is receiver contribute long exposure.
    pub fn compute_portfolio_margin(
        &self,
        account_id: u64,
        obligations: &[NetObligation],
    ) -> MarginRequirement {
        let mut total_notional: i64 = 0;
        let mut net_exposure: i64 = 0;

        for ob in obligations {
            if ob.deliverer_id == account_id {
                total_notional = total_notional.saturating_add(ob.net_payment.abs());
                net_exposure = net_exposure.saturating_sub(ob.net_payment);
            } else if ob.receiver_id == account_id {
                total_notional = total_notional.saturating_add(ob.net_payment.abs());
                net_exposure = net_exposure.saturating_add(ob.net_payment);
            }
        }

        let initial = (total_notional as f64 * self.config.initial_margin_rate) as i64;
        let variation =
            (net_exposure.unsigned_abs() as f64 * self.config.variation_margin_rate) as i64;
        let stress = self.worst_case_stress(total_notional);

        let base = initial.saturating_add(variation);
        let total = base.max(stress).max(self.config.margin_floor);

        MarginRequirement {
            account_id,
            initial_margin: initial,
            variation_margin: variation,
            stress_margin: stress,
            total_margin: total,
            content_hash: Self::hash_requirement(account_id, total),
        }
    }

    /// Evaluate worst-case loss across all stress scenarios.
    fn worst_case_stress(&self, notional: i64) -> i64 {
        let mut worst: i64 = 0;
        for &scenario in &self.config.stress_scenarios {
            let shocked = (notional as f64 * scenario) as i64;
            let loss = (shocked - notional).abs();
            // Branchless max
            let gt = (loss > worst) as i64;
            worst = gt * loss + (1 - gt) * worst;
        }
        worst
    }

    fn hash_requirement(account_id: u64, total: i64) -> u64 {
        let mut data = [0u8; 16];
        data[0..8].copy_from_slice(&account_id.to_le_bytes());
        data[8..16].copy_from_slice(&total.to_le_bytes());
        fnv1a(&data)
    }

    /// Access the current configuration.
    #[inline]
    pub fn config(&self) -> &MarginConfig {
        &self.config
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_engine() -> MarginEngine {
        MarginEngine::new(MarginConfig::default())
    }

    fn make_obligation(
        deliverer_id: u64,
        receiver_id: u64,
        net_quantity: u64,
        net_payment: i64,
    ) -> NetObligation {
        NetObligation {
            symbol_hash: 0xABCD,
            deliverer_id,
            receiver_id,
            net_quantity,
            net_payment,
            trade_count: 1,
        }
    }

    #[test]
    fn default_config_values() {
        let config = MarginConfig::default();
        assert!((config.initial_margin_rate - 0.05).abs() < 1e-10);
        assert!((config.variation_margin_rate - 1.0).abs() < 1e-10);
        assert_eq!(config.margin_floor, 100);
        assert_eq!(config.stress_scenarios.len(), 6);
    }

    #[test]
    fn single_obligation_margin() {
        let engine = default_engine();
        let ob = make_obligation(100, 200, 10, 5_000);
        let req = engine.compute_obligation_margin(&ob);

        assert_eq!(req.account_id, 100);
        // initial: 5000 * 0.05 = 250
        assert_eq!(req.initial_margin, 250);
        // variation: 5000 * 1.0 = 5000
        assert_eq!(req.variation_margin, 5_000);
        // stress: max shock is 15% → 5000 * 0.15 = 750
        assert_eq!(req.stress_margin, 750);
        // total: max(250 + 5000, 750, 100) = 5250
        assert_eq!(req.total_margin, 5_250);
    }

    #[test]
    fn margin_floor_enforced() {
        let config = MarginConfig {
            initial_margin_rate: 0.0,
            variation_margin_rate: 0.0,
            stress_scenarios: vec![1.0], // no shock
            margin_floor: 500,
        };
        let engine = MarginEngine::new(config);
        let ob = make_obligation(100, 200, 1, 10); // tiny obligation
        let req = engine.compute_obligation_margin(&ob);

        // All components are 0, but floor is 500
        assert_eq!(req.total_margin, 500);
    }

    #[test]
    fn stress_margin_selects_worst_case() {
        let config = MarginConfig {
            initial_margin_rate: 0.0,
            variation_margin_rate: 0.0,
            stress_scenarios: vec![0.70, 0.95, 1.05, 1.30], // ±30% is worst
            margin_floor: 0,
        };
        let engine = MarginEngine::new(config);
        let ob = make_obligation(1, 2, 10, 10_000);
        let req = engine.compute_obligation_margin(&ob);

        // 30% shock: 10000 * 0.30 = 3000
        assert_eq!(req.stress_margin, 3_000);
        assert_eq!(req.total_margin, 3_000);
    }

    #[test]
    fn portfolio_margin_deliverer_only() {
        let engine = default_engine();
        let obs = vec![
            make_obligation(100, 200, 5, 2_000),
            make_obligation(100, 300, 3, 3_000),
        ];
        let req = engine.compute_portfolio_margin(100, &obs);

        assert_eq!(req.account_id, 100);
        // total_notional = 2000 + 3000 = 5000
        // initial: 5000 * 0.05 = 250
        assert_eq!(req.initial_margin, 250);
    }

    #[test]
    fn portfolio_margin_receiver_only() {
        let engine = default_engine();
        let obs = vec![
            make_obligation(200, 100, 5, 2_000),
            make_obligation(300, 100, 3, 3_000),
        ];
        let req = engine.compute_portfolio_margin(100, &obs);

        assert_eq!(req.account_id, 100);
        // Account 100 is receiver in both → positive exposure
        // total_notional = 2000 + 3000 = 5000
        assert_eq!(req.initial_margin, 250);
        // net_exposure = +2000 + 3000 = 5000
        assert_eq!(req.variation_margin, 5_000);
    }

    #[test]
    fn portfolio_margin_mixed_exposure() {
        let engine = default_engine();
        let obs = vec![
            make_obligation(100, 200, 5, 4_000), // 100 delivers
            make_obligation(300, 100, 3, 3_000),  // 100 receives
        ];
        let req = engine.compute_portfolio_margin(100, &obs);

        // total_notional = 4000 + 3000 = 7000
        // net_exposure = -4000 + 3000 = -1000
        assert_eq!(req.initial_margin, 350); // 7000 * 0.05
        assert_eq!(req.variation_margin, 1_000); // |−1000| * 1.0
    }

    #[test]
    fn portfolio_margin_unrelated_account() {
        let engine = default_engine();
        let obs = vec![make_obligation(200, 300, 10, 10_000)];
        let req = engine.compute_portfolio_margin(100, &obs);

        // Account 100 is not involved → zero exposure, but floor applies
        assert_eq!(req.initial_margin, 0);
        assert_eq!(req.variation_margin, 0);
        assert_eq!(req.stress_margin, 0);
        assert_eq!(req.total_margin, 100); // floor
    }

    #[test]
    fn zero_payment_obligation() {
        let engine = default_engine();
        let ob = make_obligation(1, 2, 10, 0);
        let req = engine.compute_obligation_margin(&ob);

        assert_eq!(req.initial_margin, 0);
        assert_eq!(req.variation_margin, 0);
        assert_eq!(req.stress_margin, 0);
        assert_eq!(req.total_margin, 100); // floor
    }

    #[test]
    fn content_hash_deterministic() {
        let engine = default_engine();
        let ob = make_obligation(100, 200, 10, 5_000);
        let r1 = engine.compute_obligation_margin(&ob);
        let r2 = engine.compute_obligation_margin(&ob);
        assert_eq!(r1.content_hash, r2.content_hash);
        assert_ne!(r1.content_hash, 0);
    }

    #[test]
    fn content_hash_varies_with_input() {
        let engine = default_engine();
        let ob1 = make_obligation(100, 200, 10, 5_000);
        let ob2 = make_obligation(101, 200, 10, 5_000);
        let r1 = engine.compute_obligation_margin(&ob1);
        let r2 = engine.compute_obligation_margin(&ob2);
        assert_ne!(r1.content_hash, r2.content_hash);
    }

    #[test]
    fn negative_payment_handled() {
        let engine = default_engine();
        let ob = make_obligation(1, 2, 5, -3_000);
        let req = engine.compute_obligation_margin(&ob);
        // notional = |-3000| = 3000
        assert_eq!(req.initial_margin, 150); // 3000 * 0.05
    }

    #[test]
    fn no_stress_scenarios_uses_floor() {
        let config = MarginConfig {
            initial_margin_rate: 0.0,
            variation_margin_rate: 0.0,
            stress_scenarios: vec![],
            margin_floor: 42,
        };
        let engine = MarginEngine::new(config);
        let ob = make_obligation(1, 2, 10, 10_000);
        let req = engine.compute_obligation_margin(&ob);
        assert_eq!(req.stress_margin, 0);
        assert_eq!(req.total_margin, 42);
    }
}
