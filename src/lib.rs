/*
    ALICE-Settlement
    Copyright (C) 2026 Moroya Sakamoto
*/

pub mod clearing;
pub mod journal;
/// SPAN-style margin computation (initial, variation, stress).
pub mod margin;
pub mod netting;
/// Deterministic journal replay and verification.
pub mod replay;
pub mod trade;
/// Default waterfall cascade for loss absorption.
pub mod waterfall;

pub use clearing::{ClearingAccount, ClearingError, ClearingHouse, ClearingResult};
pub use journal::{JournalEntry, JournalEvent, SettlementJournal};
pub use margin::{MarginConfig, MarginEngine, MarginRequirement};
pub use netting::{multilateral_net, NetObligation, NettingEngine};
pub use replay::{ReplayDiscrepancy, ReplayResult, ReplayStep, ReplayVerifier};
pub use trade::{SettlementStatus, Trade};
pub use waterfall::{
    DefaultWaterfall, LayerAbsorption, WaterfallConfig, WaterfallLayer, WaterfallResult,
};

/// FNV-1a hash (crate-internal shared utility).
#[inline(always)]
pub(crate) fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
