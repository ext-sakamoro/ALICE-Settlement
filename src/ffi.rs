//! C FFI for ALICE-Settlement
//!
//! Provides 22 `extern "C"` functions for Unity / UE5 / native integration.
//!
//! License: AGPL-3.0-only
//! Author: Moroya Sakamoto

use std::ptr;
use std::slice;

use crate::clearing::ClearingHouse;
use crate::journal::{JournalEvent, SettlementJournal};
use crate::margin::{MarginConfig, MarginEngine};
use crate::netting::NettingEngine;
use crate::trade::{SettlementStatus, Trade};
use crate::waterfall::{DefaultWaterfall, WaterfallConfig};

// ── FFI-safe repr(C) structs ────────────────────────────────────────

/// FFI-safe trade representation.
#[repr(C)]
pub struct FfiTrade {
    pub trade_id: u64,
    pub symbol_hash: u64,
    pub buyer_id: u64,
    pub seller_id: u64,
    pub price: i64,
    pub quantity: u64,
    pub timestamp_ns: u64,
    pub status: u8,
}

/// FFI-safe net obligation.
#[repr(C)]
pub struct FfiNetObligation {
    pub symbol_hash: u64,
    pub deliverer_id: u64,
    pub receiver_id: u64,
    pub net_quantity: u64,
    pub net_payment: i64,
    pub trade_count: u32,
    pub _pad: u32,
}

/// FFI-safe margin requirement.
#[repr(C)]
pub struct FfiMarginRequirement {
    pub account_id: u64,
    pub initial_margin: i64,
    pub variation_margin: i64,
    pub stress_margin: i64,
    pub total_margin: i64,
    pub content_hash: u64,
}

/// FFI-safe waterfall result.
#[repr(C)]
pub struct FfiWaterfallResult {
    pub total_loss: i64,
    pub total_absorbed: i64,
    pub fully_covered: u8,
    pub shortfall: i64,
    pub content_hash: u64,
}

// ── Helper conversions ──────────────────────────────────────────────

fn status_from_u8(v: u8) -> SettlementStatus {
    match v {
        0 => SettlementStatus::Pending,
        1 => SettlementStatus::Netted,
        2 => SettlementStatus::Cleared,
        3 => SettlementStatus::Settled,
        _ => SettlementStatus::Failed,
    }
}

// ── NettingEngine (5) ───────────────────────────────────────────────

/// NettingEngineを新規作成する。
///
/// # Safety
///
/// 戻り値は`alice_netting_engine_destroy`で解放すること。
#[no_mangle]
pub unsafe extern "C" fn alice_netting_engine_new() -> *mut NettingEngine {
    Box::into_raw(Box::new(NettingEngine::new()))
}

/// NettingEngineにトレードを追加する。
///
/// # Safety
///
/// `engine`は`alice_netting_engine_new`で取得した有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_netting_engine_add_trade(
    engine: *mut NettingEngine,
    trade_id: u64,
    symbol_hash: u64,
    buyer_id: u64,
    seller_id: u64,
    price: i64,
    quantity: u64,
    timestamp_ns: u64,
    status: u8,
) {
    if engine.is_null() {
        return;
    }
    let trade = Trade {
        trade_id,
        symbol_hash,
        buyer_id,
        seller_id,
        price,
        quantity,
        timestamp_ns,
        status: status_from_u8(status),
    };
    (*engine).add_trade(&trade);
}

/// ネット債務を計算する。結果配列のポインタを返す。
///
/// # Safety
///
/// `engine`は有効なポインタであること。`out_len`は有効なポインタであること。
/// 戻り値は`alice_obligations_free`で解放すること。
#[no_mangle]
pub unsafe extern "C" fn alice_netting_engine_compute_net(
    engine: *mut NettingEngine,
    out_len: *mut u32,
) -> *mut FfiNetObligation {
    if engine.is_null() || out_len.is_null() {
        return ptr::null_mut();
    }
    let obligations = (*engine).compute_net();
    let len = obligations.len();
    *out_len = len as u32;

    if len == 0 {
        return ptr::null_mut();
    }

    let mut ffi_obs: Vec<FfiNetObligation> = obligations
        .iter()
        .map(|ob| FfiNetObligation {
            symbol_hash: ob.symbol_hash,
            deliverer_id: ob.deliverer_id,
            receiver_id: ob.receiver_id,
            net_quantity: ob.net_quantity,
            net_payment: ob.net_payment,
            trade_count: ob.trade_count,
            _pad: 0,
        })
        .collect();

    let ptr = ffi_obs.as_mut_ptr();
    std::mem::forget(ffi_obs);
    ptr
}

/// compute_netで返された配列を解放する。
///
/// # Safety
///
/// `ptr`は`alice_netting_engine_compute_net`で取得したポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_obligations_free(ptr: *mut FfiNetObligation, len: u32) {
    if !ptr.is_null() && len > 0 {
        drop(Vec::from_raw_parts(ptr, len as usize, len as usize));
    }
}

/// NettingEngineの内部状態をクリアする。
///
/// # Safety
///
/// `engine`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_netting_engine_clear(engine: *mut NettingEngine) {
    if !engine.is_null() {
        (*engine).clear();
    }
}

/// NettingEngineを解放する。
///
/// # Safety
///
/// `engine`は`alice_netting_engine_new`で取得したポインタであること。
/// 解放後にこのポインタを使用してはならない。
#[no_mangle]
pub unsafe extern "C" fn alice_netting_engine_destroy(engine: *mut NettingEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

// ── ClearingHouse (5) ───────────────────────────────────────────────

/// ClearingHouseを新規作成する。
///
/// # Safety
///
/// 戻り値は`alice_clearing_house_destroy`で解放すること。
#[no_mangle]
pub unsafe extern "C" fn alice_clearing_house_new() -> *mut ClearingHouse {
    Box::into_raw(Box::new(ClearingHouse::new()))
}

/// アカウントを登録する。
///
/// # Safety
///
/// `ch`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_clearing_house_register_account(
    ch: *mut ClearingHouse,
    id: u64,
    initial_balance: i64,
) {
    if !ch.is_null() {
        (*ch).register_account(id, initial_balance);
    }
}

/// アカウントの残高を取得する。存在しない場合はi64::MINを返す。
///
/// # Safety
///
/// `ch`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_clearing_house_get_balance(
    ch: *const ClearingHouse,
    id: u64,
) -> i64 {
    if ch.is_null() {
        return i64::MIN;
    }
    (*ch).get_account(id).map_or(i64::MIN, |acc| acc.balance)
}

/// ネット債務をクリアリングする。成功=0, 残高不足=-1, アカウント未登録=-2。
///
/// # Safety
///
/// `ch`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_clearing_house_clear_obligation(
    ch: *mut ClearingHouse,
    symbol_hash: u64,
    deliverer_id: u64,
    receiver_id: u64,
    net_quantity: u64,
    net_payment: i64,
    trade_count: u32,
) -> i32 {
    if ch.is_null() {
        return -2;
    }
    let ob = crate::netting::NetObligation {
        symbol_hash,
        deliverer_id,
        receiver_id,
        net_quantity,
        net_payment,
        trade_count,
    };
    match (*ch).clear_obligation(&ob) {
        Ok(()) => 0,
        Err(crate::clearing::ClearingError::InsufficientBalance { .. }) => -1,
        Err(crate::clearing::ClearingError::AccountNotFound(_)) => -2,
    }
}

/// ClearingHouseを解放する。
///
/// # Safety
///
/// `ch`は`alice_clearing_house_new`で取得したポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_clearing_house_destroy(ch: *mut ClearingHouse) {
    if !ch.is_null() {
        drop(Box::from_raw(ch));
    }
}

// ── MarginEngine (4) ────────────────────────────────────────────────

/// MarginEngineを新規作成する。
///
/// # Safety
///
/// 戻り値は`alice_margin_engine_destroy`で解放すること。
#[no_mangle]
pub unsafe extern "C" fn alice_margin_engine_new(
    initial_margin_rate: f64,
    variation_margin_rate: f64,
    margin_floor: i64,
) -> *mut MarginEngine {
    let config = MarginConfig {
        initial_margin_rate,
        variation_margin_rate,
        stress_scenarios: vec![0.85, 0.90, 0.95, 1.05, 1.10, 1.15],
        margin_floor,
    };
    Box::into_raw(Box::new(MarginEngine::new(config)))
}

/// 単一債務に対するマージンを計算する。
///
/// # Safety
///
/// `engine`は有効なポインタであること。`out`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_margin_engine_compute_obligation(
    engine: *const MarginEngine,
    deliverer_id: u64,
    receiver_id: u64,
    net_quantity: u64,
    net_payment: i64,
    out: *mut FfiMarginRequirement,
) -> i32 {
    if engine.is_null() || out.is_null() {
        return -1;
    }
    let ob = crate::netting::NetObligation {
        symbol_hash: 0,
        deliverer_id,
        receiver_id,
        net_quantity,
        net_payment,
        trade_count: 1,
    };
    let req = (*engine).compute_obligation_margin(&ob);
    *out = FfiMarginRequirement {
        account_id: req.account_id,
        initial_margin: req.initial_margin,
        variation_margin: req.variation_margin,
        stress_margin: req.stress_margin,
        total_margin: req.total_margin,
        content_hash: req.content_hash,
    };
    0
}

/// ポートフォリオマージンを計算する。
///
/// # Safety
///
/// `engine`は有効なポインタであること。`obligations`は`len`個の有効な配列であること。
/// `out`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_margin_engine_compute_portfolio(
    engine: *const MarginEngine,
    account_id: u64,
    obligations: *const FfiNetObligation,
    len: u32,
    out: *mut FfiMarginRequirement,
) -> i32 {
    if engine.is_null() || out.is_null() {
        return -1;
    }
    let obs: Vec<crate::netting::NetObligation> = if obligations.is_null() || len == 0 {
        Vec::new()
    } else {
        let ffi_slice = slice::from_raw_parts(obligations, len as usize);
        ffi_slice
            .iter()
            .map(|f| crate::netting::NetObligation {
                symbol_hash: f.symbol_hash,
                deliverer_id: f.deliverer_id,
                receiver_id: f.receiver_id,
                net_quantity: f.net_quantity,
                net_payment: f.net_payment,
                trade_count: f.trade_count,
            })
            .collect()
    };
    let req = (*engine).compute_portfolio_margin(account_id, &obs);
    *out = FfiMarginRequirement {
        account_id: req.account_id,
        initial_margin: req.initial_margin,
        variation_margin: req.variation_margin,
        stress_margin: req.stress_margin,
        total_margin: req.total_margin,
        content_hash: req.content_hash,
    };
    0
}

/// MarginEngineを解放する。
///
/// # Safety
///
/// `engine`は`alice_margin_engine_new`で取得したポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_margin_engine_destroy(engine: *mut MarginEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

// ── DefaultWaterfall (4) ────────────────────────────────────────────

/// DefaultWaterfallを新規作成する。
///
/// # Safety
///
/// 戻り値は`alice_waterfall_destroy`で解放すること。
#[no_mangle]
pub unsafe extern "C" fn alice_waterfall_new(
    defaulter_margin: i64,
    defaulter_fund: i64,
    ccp_first_loss: i64,
    members_fund: i64,
    ccp_capital: i64,
) -> *mut DefaultWaterfall {
    let config = WaterfallConfig {
        defaulter_margin,
        defaulter_fund,
        ccp_first_loss,
        members_fund,
        ccp_capital,
    };
    Box::into_raw(Box::new(DefaultWaterfall::new(config)))
}

/// 損失をウォーターフォールで吸収する。
///
/// # Safety
///
/// `wf`は有効なポインタであること。`out`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_waterfall_absorb_loss(
    wf: *const DefaultWaterfall,
    loss: i64,
    out: *mut FfiWaterfallResult,
) -> i32 {
    if wf.is_null() || out.is_null() {
        return -1;
    }
    let result = (*wf).absorb_loss(loss);
    *out = FfiWaterfallResult {
        total_loss: result.total_loss,
        total_absorbed: result.total_absorbed,
        fully_covered: u8::from(result.fully_covered),
        shortfall: result.shortfall,
        content_hash: result.content_hash,
    };
    0
}

/// ウォーターフォールの総容量を返す。
///
/// # Safety
///
/// `wf`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_waterfall_total_capacity(wf: *const DefaultWaterfall) -> i64 {
    if wf.is_null() {
        return 0;
    }
    (*wf).total_capacity()
}

/// DefaultWaterfallを解放する。
///
/// # Safety
///
/// `wf`は`alice_waterfall_new`で取得したポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_waterfall_destroy(wf: *mut DefaultWaterfall) {
    if !wf.is_null() {
        drop(Box::from_raw(wf));
    }
}

// ── SettlementJournal (4) ───────────────────────────────────────────

/// SettlementJournalを新規作成する。
///
/// # Safety
///
/// 戻り値は`alice_journal_destroy`で解放すること。
#[no_mangle]
pub unsafe extern "C" fn alice_journal_new() -> *mut SettlementJournal {
    Box::into_raw(Box::new(SettlementJournal::new()))
}

/// トレード受信イベントを記録する。
///
/// # Safety
///
/// `journal`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_journal_record_trade(
    journal: *mut SettlementJournal,
    timestamp_ns: u64,
    trade_id: u64,
) {
    if !journal.is_null() {
        (*journal).record(timestamp_ns, JournalEvent::TradeReceived { trade_id });
    }
}

/// ジャーナルのエントリ数を返す。
///
/// # Safety
///
/// `journal`は有効なポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_journal_len(journal: *const SettlementJournal) -> u32 {
    if journal.is_null() {
        return 0;
    }
    (*journal).len() as u32
}

/// SettlementJournalを解放する。
///
/// # Safety
///
/// `journal`は`alice_journal_new`で取得したポインタであること。
#[no_mangle]
pub unsafe extern "C" fn alice_journal_destroy(journal: *mut SettlementJournal) {
    if !journal.is_null() {
        drop(Box::from_raw(journal));
    }
}

// ── テスト ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_netting_engine_lifecycle() {
        unsafe {
            let engine = alice_netting_engine_new();
            assert!(!engine.is_null());

            alice_netting_engine_add_trade(engine, 1, 0xABCD, 100, 200, 50_000, 10, 0, 0);
            alice_netting_engine_add_trade(engine, 2, 0xABCD, 200, 100, 50_500, 3, 1, 0);

            let mut len: u32 = 0;
            let obs = alice_netting_engine_compute_net(engine, &mut len);
            assert_eq!(len, 1);
            assert!(!obs.is_null());

            let ob = &*obs;
            assert_eq!(ob.net_quantity, 7);
            alice_obligations_free(obs, len);

            alice_netting_engine_clear(engine);
            let obs2 = alice_netting_engine_compute_net(engine, &mut len);
            assert_eq!(len, 0);
            assert!(obs2.is_null());

            alice_netting_engine_destroy(engine);
        }
    }

    #[test]
    fn test_clearing_house_lifecycle() {
        unsafe {
            let ch = alice_clearing_house_new();
            assert!(!ch.is_null());

            alice_clearing_house_register_account(ch, 100, 50_000);
            alice_clearing_house_register_account(ch, 200, 10_000);

            let bal = alice_clearing_house_get_balance(ch, 100);
            assert_eq!(bal, 50_000);

            assert_eq!(alice_clearing_house_get_balance(ch, 999), i64::MIN);

            let rc = alice_clearing_house_clear_obligation(ch, 0xABCD, 100, 200, 10, 5_000, 1);
            assert_eq!(rc, 0);
            assert_eq!(alice_clearing_house_get_balance(ch, 100), 45_000);
            assert_eq!(alice_clearing_house_get_balance(ch, 200), 15_000);

            alice_clearing_house_destroy(ch);
        }
    }

    #[test]
    fn test_margin_engine_lifecycle() {
        unsafe {
            let engine = alice_margin_engine_new(0.05, 1.0, 100);
            assert!(!engine.is_null());

            let mut req = std::mem::zeroed::<FfiMarginRequirement>();
            let rc = alice_margin_engine_compute_obligation(engine, 100, 200, 10, 5_000, &mut req);
            assert_eq!(rc, 0);
            assert_eq!(req.account_id, 100);
            assert_eq!(req.initial_margin, 250);
            assert_eq!(req.total_margin, 5_250);

            alice_margin_engine_destroy(engine);
        }
    }

    #[test]
    fn test_waterfall_lifecycle() {
        unsafe {
            let wf = alice_waterfall_new(100, 50, 30, 200, 500);
            assert!(!wf.is_null());

            assert_eq!(alice_waterfall_total_capacity(wf), 880);

            let mut result = std::mem::zeroed::<FfiWaterfallResult>();
            let rc = alice_waterfall_absorb_loss(wf, 120, &mut result);
            assert_eq!(rc, 0);
            assert_eq!(result.total_absorbed, 120);
            assert_eq!(result.fully_covered, 1);
            assert_eq!(result.shortfall, 0);

            alice_waterfall_destroy(wf);
        }
    }

    #[test]
    fn test_journal_lifecycle() {
        unsafe {
            let journal = alice_journal_new();
            assert!(!journal.is_null());

            assert_eq!(alice_journal_len(journal), 0);

            alice_journal_record_trade(journal, 1000, 42);
            assert_eq!(alice_journal_len(journal), 1);

            alice_journal_record_trade(journal, 2000, 43);
            assert_eq!(alice_journal_len(journal), 2);

            alice_journal_destroy(journal);
        }
    }

    #[test]
    fn test_null_safety() {
        unsafe {
            alice_netting_engine_add_trade(ptr::null_mut(), 0, 0, 0, 0, 0, 0, 0, 0);
            alice_netting_engine_clear(ptr::null_mut());
            alice_netting_engine_destroy(ptr::null_mut());

            let mut len: u32 = 0;
            assert!(alice_netting_engine_compute_net(ptr::null_mut(), &mut len).is_null());

            alice_clearing_house_register_account(ptr::null_mut(), 0, 0);
            assert_eq!(alice_clearing_house_get_balance(ptr::null(), 0), i64::MIN);
            alice_clearing_house_destroy(ptr::null_mut());

            alice_margin_engine_destroy(ptr::null_mut());

            assert_eq!(alice_waterfall_total_capacity(ptr::null()), 0);
            alice_waterfall_destroy(ptr::null_mut());

            alice_journal_record_trade(ptr::null_mut(), 0, 0);
            assert_eq!(alice_journal_len(ptr::null()), 0);
            alice_journal_destroy(ptr::null_mut());
        }
    }
}
