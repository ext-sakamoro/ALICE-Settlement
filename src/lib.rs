#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::module_name_repetitions,
    clippy::inline_always,
    clippy::too_many_lines
)]
/*
    ALICE-Settlement
    Copyright (C) 2026 Moroya Sakamoto
*/

//! # ALICE-Settlement
//!
//! Post-trade settlement, netting, and clearing engine for the ALICE
//! financial system.
//!
//! # Modules
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`trade`] | `Trade` and `SettlementStatus` lifecycle types |
//! | [`netting`] | Bilateral and multilateral netting of trade obligations |
//! | [`clearing`] | `ClearingHouse` account management and fund transfer |
//! | [`margin`] | SPAN-style margin computation (initial, variation, stress) |
//! | [`journal`] | Append-only settlement journal with hash-chained entries |
//! | [`replay`] | Deterministic journal replay and verification |
//! | [`waterfall`] | Default waterfall cascade for loss absorption |
//!
//! # Quick Start
//!
//! ```rust
//! use alice_settlement::trade::{Trade, SettlementStatus};
//! use alice_settlement::netting::NettingEngine;
//!
//! let trades = vec![
//!     Trade {
//!         trade_id: 1, symbol_hash: 0xABCD,
//!         buyer_id: 100, seller_id: 200,
//!         price: 50_000, quantity: 10,
//!         timestamp_ns: 0, status: SettlementStatus::Pending,
//!     },
//!     Trade {
//!         trade_id: 2, symbol_hash: 0xABCD,
//!         buyer_id: 200, seller_id: 100,
//!         price: 50_500, quantity: 3,
//!         timestamp_ns: 1, status: SettlementStatus::Pending,
//!     },
//! ];
//!
//! let mut engine = NettingEngine::new();
//! for t in &trades { engine.add_trade(t); }
//! let obligations = engine.compute_net();
//! assert_eq!(obligations.len(), 1);
//! assert_eq!(obligations[0].net_quantity, 7); // 10 - 3
//! ```

pub mod clearing;
pub mod collateral;
#[cfg(feature = "ffi")]
pub mod ffi;
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
pub use collateral::{
    check_concentration, CollateralAccount, CollateralHolding, CollateralType, ConcentrationBreach,
    ConcentrationLimits, HaircutConfig,
};
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
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
