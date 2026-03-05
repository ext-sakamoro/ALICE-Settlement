// ALICE-Settlement — UE5 C++ Bindings
// License: AGPL-3.0-only
// Author: Moroya Sakamoto
//
// 22 extern "C" declarations + RAII wrapper classes

#pragma once

#include <cstdint>
#include <utility>

// ── FFI structs ─────────────────────────────────────────────────────

struct FfiNetObligation {
    uint64_t symbol_hash;
    uint64_t deliverer_id;
    uint64_t receiver_id;
    uint64_t net_quantity;
    int64_t  net_payment;
    uint32_t trade_count;
    uint32_t _pad;
};

struct FfiMarginRequirement {
    uint64_t account_id;
    int64_t  initial_margin;
    int64_t  variation_margin;
    int64_t  stress_margin;
    int64_t  total_margin;
    uint64_t content_hash;
};

struct FfiWaterfallResult {
    int64_t  total_loss;
    int64_t  total_absorbed;
    uint8_t  fully_covered;
    int64_t  shortfall;
    uint64_t content_hash;
};

// ── extern "C" declarations ─────────────────────────────────────────

extern "C" {

// NettingEngine
void*    alice_netting_engine_new();
void     alice_netting_engine_add_trade(void* engine, uint64_t trade_id,
             uint64_t symbol_hash, uint64_t buyer_id, uint64_t seller_id,
             int64_t price, uint64_t quantity, uint64_t timestamp_ns, uint8_t status);
FfiNetObligation* alice_netting_engine_compute_net(void* engine, uint32_t* out_len);
void     alice_obligations_free(FfiNetObligation* ptr, uint32_t len);
void     alice_netting_engine_clear(void* engine);
void     alice_netting_engine_destroy(void* engine);

// ClearingHouse
void*    alice_clearing_house_new();
void     alice_clearing_house_register_account(void* ch, uint64_t id, int64_t initial_balance);
int64_t  alice_clearing_house_get_balance(const void* ch, uint64_t id);
int32_t  alice_clearing_house_clear_obligation(void* ch, uint64_t symbol_hash,
             uint64_t deliverer_id, uint64_t receiver_id, uint64_t net_quantity,
             int64_t net_payment, uint32_t trade_count);
void     alice_clearing_house_destroy(void* ch);

// MarginEngine
void*    alice_margin_engine_new(double initial_margin_rate, double variation_margin_rate,
             int64_t margin_floor);
int32_t  alice_margin_engine_compute_obligation(const void* engine, uint64_t deliverer_id,
             uint64_t receiver_id, uint64_t net_quantity, int64_t net_payment,
             FfiMarginRequirement* out);
int32_t  alice_margin_engine_compute_portfolio(const void* engine, uint64_t account_id,
             const FfiNetObligation* obligations, uint32_t len, FfiMarginRequirement* out);
void     alice_margin_engine_destroy(void* engine);

// DefaultWaterfall
void*    alice_waterfall_new(int64_t defaulter_margin, int64_t defaulter_fund,
             int64_t ccp_first_loss, int64_t members_fund, int64_t ccp_capital);
int32_t  alice_waterfall_absorb_loss(const void* wf, int64_t loss, FfiWaterfallResult* out);
int64_t  alice_waterfall_total_capacity(const void* wf);
void     alice_waterfall_destroy(void* wf);

// SettlementJournal
void*    alice_journal_new();
void     alice_journal_record_trade(void* journal, uint64_t timestamp_ns, uint64_t trade_id);
uint32_t alice_journal_len(const void* journal);
void     alice_journal_destroy(void* journal);

} // extern "C"

// ── RAII wrappers ───────────────────────────────────────────────────

class AliceNettingEngine {
    void* ptr_;
public:
    AliceNettingEngine() : ptr_(alice_netting_engine_new()) {}
    ~AliceNettingEngine() { if (ptr_) alice_netting_engine_destroy(ptr_); }
    AliceNettingEngine(const AliceNettingEngine&) = delete;
    AliceNettingEngine& operator=(const AliceNettingEngine&) = delete;
    AliceNettingEngine(AliceNettingEngine&& o) noexcept : ptr_(std::exchange(o.ptr_, nullptr)) {}
    AliceNettingEngine& operator=(AliceNettingEngine&& o) noexcept {
        if (this != &o) { if (ptr_) alice_netting_engine_destroy(ptr_); ptr_ = std::exchange(o.ptr_, nullptr); }
        return *this;
    }
    void AddTrade(uint64_t tid, uint64_t sym, uint64_t buyer, uint64_t seller,
                  int64_t price, uint64_t qty, uint64_t ts, uint8_t status) {
        alice_netting_engine_add_trade(ptr_, tid, sym, buyer, seller, price, qty, ts, status);
    }
    void Clear() { alice_netting_engine_clear(ptr_); }
    void* Handle() const { return ptr_; }
};

class AliceClearingHouse {
    void* ptr_;
public:
    AliceClearingHouse() : ptr_(alice_clearing_house_new()) {}
    ~AliceClearingHouse() { if (ptr_) alice_clearing_house_destroy(ptr_); }
    AliceClearingHouse(const AliceClearingHouse&) = delete;
    AliceClearingHouse& operator=(const AliceClearingHouse&) = delete;
    AliceClearingHouse(AliceClearingHouse&& o) noexcept : ptr_(std::exchange(o.ptr_, nullptr)) {}
    AliceClearingHouse& operator=(AliceClearingHouse&& o) noexcept {
        if (this != &o) { if (ptr_) alice_clearing_house_destroy(ptr_); ptr_ = std::exchange(o.ptr_, nullptr); }
        return *this;
    }
    void RegisterAccount(uint64_t id, int64_t balance) { alice_clearing_house_register_account(ptr_, id, balance); }
    int64_t GetBalance(uint64_t id) const { return alice_clearing_house_get_balance(ptr_, id); }
    int32_t ClearObligation(uint64_t sym, uint64_t del, uint64_t rec,
                            uint64_t qty, int64_t pay, uint32_t cnt) {
        return alice_clearing_house_clear_obligation(ptr_, sym, del, rec, qty, pay, cnt);
    }
};

class AliceMarginEngine {
    void* ptr_;
public:
    AliceMarginEngine(double ir, double vr, int64_t floor)
        : ptr_(alice_margin_engine_new(ir, vr, floor)) {}
    ~AliceMarginEngine() { if (ptr_) alice_margin_engine_destroy(ptr_); }
    AliceMarginEngine(const AliceMarginEngine&) = delete;
    AliceMarginEngine& operator=(const AliceMarginEngine&) = delete;
    AliceMarginEngine(AliceMarginEngine&& o) noexcept : ptr_(std::exchange(o.ptr_, nullptr)) {}
    AliceMarginEngine& operator=(AliceMarginEngine&& o) noexcept {
        if (this != &o) { if (ptr_) alice_margin_engine_destroy(ptr_); ptr_ = std::exchange(o.ptr_, nullptr); }
        return *this;
    }
    FfiMarginRequirement ComputeObligation(uint64_t del, uint64_t rec,
                                           uint64_t qty, int64_t pay) {
        FfiMarginRequirement req{};
        alice_margin_engine_compute_obligation(ptr_, del, rec, qty, pay, &req);
        return req;
    }
    FfiMarginRequirement ComputePortfolio(uint64_t aid, const FfiNetObligation* obs, uint32_t len) {
        FfiMarginRequirement req{};
        alice_margin_engine_compute_portfolio(ptr_, aid, obs, len, &req);
        return req;
    }
};

class AliceDefaultWaterfall {
    void* ptr_;
public:
    AliceDefaultWaterfall(int64_t dm, int64_t df, int64_t cfl, int64_t mf, int64_t cc)
        : ptr_(alice_waterfall_new(dm, df, cfl, mf, cc)) {}
    ~AliceDefaultWaterfall() { if (ptr_) alice_waterfall_destroy(ptr_); }
    AliceDefaultWaterfall(const AliceDefaultWaterfall&) = delete;
    AliceDefaultWaterfall& operator=(const AliceDefaultWaterfall&) = delete;
    AliceDefaultWaterfall(AliceDefaultWaterfall&& o) noexcept : ptr_(std::exchange(o.ptr_, nullptr)) {}
    AliceDefaultWaterfall& operator=(AliceDefaultWaterfall&& o) noexcept {
        if (this != &o) { if (ptr_) alice_waterfall_destroy(ptr_); ptr_ = std::exchange(o.ptr_, nullptr); }
        return *this;
    }
    FfiWaterfallResult AbsorbLoss(int64_t loss) {
        FfiWaterfallResult r{};
        alice_waterfall_absorb_loss(ptr_, loss, &r);
        return r;
    }
    int64_t TotalCapacity() const { return alice_waterfall_total_capacity(ptr_); }
};

class AliceSettlementJournal {
    void* ptr_;
public:
    AliceSettlementJournal() : ptr_(alice_journal_new()) {}
    ~AliceSettlementJournal() { if (ptr_) alice_journal_destroy(ptr_); }
    AliceSettlementJournal(const AliceSettlementJournal&) = delete;
    AliceSettlementJournal& operator=(const AliceSettlementJournal&) = delete;
    AliceSettlementJournal(AliceSettlementJournal&& o) noexcept : ptr_(std::exchange(o.ptr_, nullptr)) {}
    AliceSettlementJournal& operator=(AliceSettlementJournal&& o) noexcept {
        if (this != &o) { if (ptr_) alice_journal_destroy(ptr_); ptr_ = std::exchange(o.ptr_, nullptr); }
        return *this;
    }
    void RecordTrade(uint64_t ts, uint64_t tid) { alice_journal_record_trade(ptr_, ts, tid); }
    uint32_t Len() const { return alice_journal_len(ptr_); }
};
