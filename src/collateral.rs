// ALICE-Settlement — Collateral management with haircuts and concentration limits
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Moroya Sakamoto

use crate::fnv1a;

// ── Collateral Types ───────────────────────────────────────────────────

/// 担保の品質ティア（高い方が優良）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum CollateralType {
    /// 現金（最高品質、ヘアカット 0%）。
    Cash = 0,
    /// 国債（高品質）。
    GovernmentBond = 1,
    /// 社債（中品質）。
    CorporateBond = 2,
    /// 株式（低品質、高ヘアカット）。
    Equity = 3,
}

/// ヘアカット設定（担保タイプ別）。
///
/// ヘアカットは 0〜10000 の基点（bps）で表現する。
/// 10000 bps = 100% = 担保価値ゼロ。
#[derive(Debug, Clone)]
pub struct HaircutConfig {
    /// Cash のヘアカット（bps）。通常 0。
    pub cash_bps: u32,
    /// 国債のヘアカット（bps）。通常 200〜500。
    pub gov_bond_bps: u32,
    /// 社債のヘアカット（bps）。通常 500〜1500。
    pub corp_bond_bps: u32,
    /// 株式のヘアカット（bps）。通常 1500〜3000。
    pub equity_bps: u32,
}

impl Default for HaircutConfig {
    fn default() -> Self {
        Self {
            cash_bps: 0,
            gov_bond_bps: 300,
            corp_bond_bps: 1000,
            equity_bps: 2500,
        }
    }
}

impl HaircutConfig {
    /// 担保タイプに対応するヘアカット（bps）を返す。
    #[must_use]
    pub const fn haircut_bps(&self, collateral_type: CollateralType) -> u32 {
        match collateral_type {
            CollateralType::Cash => self.cash_bps,
            CollateralType::GovernmentBond => self.gov_bond_bps,
            CollateralType::CorporateBond => self.corp_bond_bps,
            CollateralType::Equity => self.equity_bps,
        }
    }

    /// ヘアカット後の価値を計算する。
    ///
    /// `value * (10000 - haircut_bps) / 10000`
    #[must_use]
    pub const fn apply_haircut(&self, collateral_type: CollateralType, value: i64) -> i64 {
        let bps = self.haircut_bps(collateral_type) as i64;
        value * (10_000 - bps) / 10_000
    }
}

// ── Collateral Holding ─────────────────────────────────────────────────

/// 単一タイプの担保保有。
#[derive(Debug, Clone)]
pub struct CollateralHolding {
    /// 担保タイプ。
    pub collateral_type: CollateralType,
    /// 額面価値（ティック単位）。
    pub face_value: i64,
}

// ── Collateral Account ─────────────────────────────────────────────────

/// 担保アカウント（複数タイプの担保を保有）。
pub struct CollateralAccount {
    /// アカウント ID。
    account_id: u64,
    /// 各タイプの保有額面（`Cash`, `GovBond`, `CorpBond`, `Equity`）。
    holdings: [i64; 4],
    /// ヘアカット設定。
    haircut: HaircutConfig,
}

impl CollateralAccount {
    /// 新規作成。
    #[must_use]
    pub const fn new(account_id: u64, haircut: HaircutConfig) -> Self {
        Self {
            account_id,
            holdings: [0; 4],
            haircut,
        }
    }

    /// アカウント ID。
    #[must_use]
    pub const fn account_id(&self) -> u64 {
        self.account_id
    }

    /// 担保を預け入れる。
    pub const fn deposit(&mut self, collateral_type: CollateralType, amount: i64) {
        if amount > 0 {
            self.holdings[collateral_type as usize] += amount;
        }
    }

    /// 担保を引き出す。残高不足の場合は `false` を返す。
    pub const fn withdraw(&mut self, collateral_type: CollateralType, amount: i64) -> bool {
        if amount <= 0 {
            return true;
        }
        let idx = collateral_type as usize;
        if self.holdings[idx] < amount {
            return false;
        }
        self.holdings[idx] -= amount;
        true
    }

    /// 指定タイプの額面残高。
    #[must_use]
    pub const fn face_value(&self, collateral_type: CollateralType) -> i64 {
        self.holdings[collateral_type as usize]
    }

    /// 指定タイプのヘアカット後価値。
    #[must_use]
    pub const fn adjusted_value(&self, collateral_type: CollateralType) -> i64 {
        self.haircut
            .apply_haircut(collateral_type, self.holdings[collateral_type as usize])
    }

    /// 全タイプ合計の額面残高。
    #[must_use]
    pub const fn total_face_value(&self) -> i64 {
        self.holdings[0] + self.holdings[1] + self.holdings[2] + self.holdings[3]
    }

    /// 全タイプ合計のヘアカット後価値。
    #[must_use]
    pub const fn total_adjusted_value(&self) -> i64 {
        self.adjusted_value(CollateralType::Cash)
            + self.adjusted_value(CollateralType::GovernmentBond)
            + self.adjusted_value(CollateralType::CorporateBond)
            + self.adjusted_value(CollateralType::Equity)
    }

    /// 全保有の詳細を返す。
    #[must_use]
    pub fn holdings(&self) -> Vec<CollateralHolding> {
        let types = [
            CollateralType::Cash,
            CollateralType::GovernmentBond,
            CollateralType::CorporateBond,
            CollateralType::Equity,
        ];
        types
            .iter()
            .filter(|&&ct| self.holdings[ct as usize] > 0)
            .map(|&ct| CollateralHolding {
                collateral_type: ct,
                face_value: self.holdings[ct as usize],
            })
            .collect()
    }

    /// コンテンツハッシュ。
    #[must_use]
    pub fn content_hash(&self) -> u64 {
        let mut data = [0u8; 40];
        data[0..8].copy_from_slice(&self.account_id.to_le_bytes());
        for (i, &h) in self.holdings.iter().enumerate() {
            data[8 + i * 8..16 + i * 8].copy_from_slice(&h.to_le_bytes());
        }
        fnv1a(&data)
    }
}

// ── Concentration Limits ───────────────────────────────────────────────

/// 集中リスク制限（タイプ別上限比率）。
///
/// 各タイプの担保が全担保（ヘアカット後）に占める割合の上限を設定する。
/// 上限は bps で表現（10000 = 100%）。
#[derive(Debug, Clone)]
pub struct ConcentrationLimits {
    /// Cash の上限 bps（通常 10000 = 制限なし）。
    pub cash_max_bps: u32,
    /// 国債の上限 bps。
    pub gov_bond_max_bps: u32,
    /// 社債の上限 bps。
    pub corp_bond_max_bps: u32,
    /// 株式の上限 bps。
    pub equity_max_bps: u32,
}

impl Default for ConcentrationLimits {
    fn default() -> Self {
        Self {
            cash_max_bps: 10_000,     // 制限なし
            gov_bond_max_bps: 10_000, // 制限なし
            corp_bond_max_bps: 5_000, // 50%
            equity_max_bps: 3_000,    // 30%
        }
    }
}

/// 集中リスク違反。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcentrationBreach {
    /// 違反した担保タイプ。
    pub collateral_type: CollateralType,
    /// 現在の比率（bps）。
    pub current_bps: u32,
    /// 上限（bps）。
    pub limit_bps: u32,
}

/// 担保アカウントの集中リスクをチェックする。
///
/// 違反があった場合、全違反の一覧を返す。
#[must_use]
pub fn check_concentration(
    account: &CollateralAccount,
    limits: &ConcentrationLimits,
) -> Vec<ConcentrationBreach> {
    let total = account.total_adjusted_value();
    if total <= 0 {
        return Vec::new();
    }

    let types_and_limits = [
        (CollateralType::Cash, limits.cash_max_bps),
        (CollateralType::GovernmentBond, limits.gov_bond_max_bps),
        (CollateralType::CorporateBond, limits.corp_bond_max_bps),
        (CollateralType::Equity, limits.equity_max_bps),
    ];

    let mut breaches = Vec::new();
    for (ct, limit_bps) in types_and_limits {
        let adjusted = account.adjusted_value(ct);
        if adjusted <= 0 {
            continue;
        }
        let current_bps = (adjusted * 10_000 / total) as u32;
        if current_bps > limit_bps {
            breaches.push(ConcentrationBreach {
                collateral_type: ct,
                current_bps,
                limit_bps,
            });
        }
    }
    breaches
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haircut_default_values() {
        let cfg = HaircutConfig::default();
        assert_eq!(cfg.cash_bps, 0);
        assert_eq!(cfg.gov_bond_bps, 300);
        assert_eq!(cfg.corp_bond_bps, 1000);
        assert_eq!(cfg.equity_bps, 2500);
    }

    #[test]
    fn haircut_apply_cash() {
        let cfg = HaircutConfig::default();
        // Cash: 0% haircut → 10000 → 10000
        assert_eq!(cfg.apply_haircut(CollateralType::Cash, 10_000), 10_000);
    }

    #[test]
    fn haircut_apply_gov_bond() {
        let cfg = HaircutConfig::default();
        // GovBond: 3% haircut → 10000 * 9700/10000 = 9700
        assert_eq!(
            cfg.apply_haircut(CollateralType::GovernmentBond, 10_000),
            9_700
        );
    }

    #[test]
    fn haircut_apply_corp_bond() {
        let cfg = HaircutConfig::default();
        // CorpBond: 10% haircut → 10000 * 9000/10000 = 9000
        assert_eq!(
            cfg.apply_haircut(CollateralType::CorporateBond, 10_000),
            9_000
        );
    }

    #[test]
    fn haircut_apply_equity() {
        let cfg = HaircutConfig::default();
        // Equity: 25% haircut → 10000 * 7500/10000 = 7500
        assert_eq!(cfg.apply_haircut(CollateralType::Equity, 10_000), 7_500);
    }

    #[test]
    fn haircut_apply_zero_value() {
        let cfg = HaircutConfig::default();
        assert_eq!(cfg.apply_haircut(CollateralType::Equity, 0), 0);
    }

    #[test]
    fn account_deposit_and_face_value() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 5_000);
        acc.deposit(CollateralType::GovernmentBond, 3_000);
        assert_eq!(acc.face_value(CollateralType::Cash), 5_000);
        assert_eq!(acc.face_value(CollateralType::GovernmentBond), 3_000);
        assert_eq!(acc.face_value(CollateralType::CorporateBond), 0);
    }

    #[test]
    fn account_total_face_value() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 5_000);
        acc.deposit(CollateralType::Equity, 2_000);
        assert_eq!(acc.total_face_value(), 7_000);
    }

    #[test]
    fn account_adjusted_value() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 10_000);
        acc.deposit(CollateralType::Equity, 10_000);
        // Cash: 10000, Equity: 10000 * 0.75 = 7500
        assert_eq!(acc.adjusted_value(CollateralType::Cash), 10_000);
        assert_eq!(acc.adjusted_value(CollateralType::Equity), 7_500);
        assert_eq!(acc.total_adjusted_value(), 17_500);
    }

    #[test]
    fn account_withdraw_success() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 5_000);
        assert!(acc.withdraw(CollateralType::Cash, 3_000));
        assert_eq!(acc.face_value(CollateralType::Cash), 2_000);
    }

    #[test]
    fn account_withdraw_insufficient() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 1_000);
        assert!(!acc.withdraw(CollateralType::Cash, 2_000));
        assert_eq!(acc.face_value(CollateralType::Cash), 1_000); // 変化なし
    }

    #[test]
    fn account_withdraw_zero() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        assert!(acc.withdraw(CollateralType::Cash, 0));
    }

    #[test]
    fn account_deposit_negative_ignored() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, -100);
        assert_eq!(acc.face_value(CollateralType::Cash), 0);
    }

    #[test]
    fn account_holdings_non_empty() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 1_000);
        acc.deposit(CollateralType::Equity, 2_000);
        let holdings = acc.holdings();
        assert_eq!(holdings.len(), 2);
    }

    #[test]
    fn account_holdings_empty() {
        let acc = CollateralAccount::new(1, HaircutConfig::default());
        assert!(acc.holdings().is_empty());
    }

    #[test]
    fn account_id_accessor() {
        let acc = CollateralAccount::new(42, HaircutConfig::default());
        assert_eq!(acc.account_id(), 42);
    }

    #[test]
    fn content_hash_deterministic() {
        let mut a = CollateralAccount::new(1, HaircutConfig::default());
        let mut b = CollateralAccount::new(1, HaircutConfig::default());
        a.deposit(CollateralType::Cash, 100);
        b.deposit(CollateralType::Cash, 100);
        assert_eq!(a.content_hash(), b.content_hash());
        assert_ne!(a.content_hash(), 0);
    }

    #[test]
    fn content_hash_varies() {
        let mut a = CollateralAccount::new(1, HaircutConfig::default());
        let mut b = CollateralAccount::new(1, HaircutConfig::default());
        a.deposit(CollateralType::Cash, 100);
        b.deposit(CollateralType::Cash, 200);
        assert_ne!(a.content_hash(), b.content_hash());
    }

    // ── Concentration Limits ───────────────────────────────────────

    #[test]
    fn concentration_no_breach() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 10_000);
        let breaches = check_concentration(&acc, &ConcentrationLimits::default());
        assert!(breaches.is_empty());
    }

    #[test]
    fn concentration_equity_breach() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        // 株式のみ → 集中率 100% > 上限 30%
        acc.deposit(CollateralType::Equity, 10_000);
        let breaches = check_concentration(&acc, &ConcentrationLimits::default());
        assert_eq!(breaches.len(), 1);
        assert_eq!(breaches[0].collateral_type, CollateralType::Equity);
        assert!(breaches[0].current_bps > 3_000);
        assert_eq!(breaches[0].limit_bps, 3_000);
    }

    #[test]
    fn concentration_corp_bond_breach() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        // 社債のみ → 集中率 100% > 上限 50%
        acc.deposit(CollateralType::CorporateBond, 10_000);
        let breaches = check_concentration(&acc, &ConcentrationLimits::default());
        assert_eq!(breaches.len(), 1);
        assert_eq!(breaches[0].collateral_type, CollateralType::CorporateBond);
    }

    #[test]
    fn concentration_mixed_no_breach() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        // Cash 70%, Equity 30% (ヘアカット後)
        acc.deposit(CollateralType::Cash, 70_000);
        acc.deposit(CollateralType::Equity, 13_334); // 13334 * 0.75 = 10000 → 10000/80000 = 12.5%
        let breaches = check_concentration(&acc, &ConcentrationLimits::default());
        assert!(breaches.is_empty());
    }

    #[test]
    fn concentration_empty_account_no_breach() {
        let acc = CollateralAccount::new(1, HaircutConfig::default());
        let breaches = check_concentration(&acc, &ConcentrationLimits::default());
        assert!(breaches.is_empty());
    }

    #[test]
    fn concentration_limits_default() {
        let limits = ConcentrationLimits::default();
        assert_eq!(limits.cash_max_bps, 10_000);
        assert_eq!(limits.gov_bond_max_bps, 10_000);
        assert_eq!(limits.corp_bond_max_bps, 5_000);
        assert_eq!(limits.equity_max_bps, 3_000);
    }

    #[test]
    fn concentration_breach_equality() {
        let b1 = ConcentrationBreach {
            collateral_type: CollateralType::Equity,
            current_bps: 5000,
            limit_bps: 3000,
        };
        let b2 = b1.clone();
        assert_eq!(b1, b2);
    }

    #[test]
    fn collateral_type_ordering() {
        assert!(CollateralType::Cash < CollateralType::GovernmentBond);
        assert!(CollateralType::GovernmentBond < CollateralType::CorporateBond);
        assert!(CollateralType::CorporateBond < CollateralType::Equity);
    }

    #[test]
    fn collateral_type_repr() {
        assert_eq!(CollateralType::Cash as u8, 0);
        assert_eq!(CollateralType::GovernmentBond as u8, 1);
        assert_eq!(CollateralType::CorporateBond as u8, 2);
        assert_eq!(CollateralType::Equity as u8, 3);
    }

    #[test]
    fn multiple_deposits_accumulate() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::Cash, 1_000);
        acc.deposit(CollateralType::Cash, 2_000);
        acc.deposit(CollateralType::Cash, 3_000);
        assert_eq!(acc.face_value(CollateralType::Cash), 6_000);
    }

    #[test]
    fn withdraw_exact_balance() {
        let mut acc = CollateralAccount::new(1, HaircutConfig::default());
        acc.deposit(CollateralType::GovernmentBond, 5_000);
        assert!(acc.withdraw(CollateralType::GovernmentBond, 5_000));
        assert_eq!(acc.face_value(CollateralType::GovernmentBond), 0);
    }

    #[test]
    fn custom_haircut_config() {
        let cfg = HaircutConfig {
            cash_bps: 100,
            gov_bond_bps: 500,
            corp_bond_bps: 2000,
            equity_bps: 5000,
        };
        // Cash 1%: 10000 * 9900/10000 = 9900
        assert_eq!(cfg.apply_haircut(CollateralType::Cash, 10_000), 9_900);
        // Equity 50%: 10000 * 5000/10000 = 5000
        assert_eq!(cfg.apply_haircut(CollateralType::Equity, 10_000), 5_000);
    }
}
