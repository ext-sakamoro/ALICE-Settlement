# ALICE-Settlement

Post-trade settlement, netting, and clearing engine for the ALICE financial infrastructure.

## Features

- Bilateral and multilateral netting with cycle cancellation via DFS
- Central clearing house with per-obligation balance transfer and error reporting
- SPAN-style margin computation: initial, variation, and stress scenarios
- Five-layer default waterfall (CCP loss absorption cascade)
- Append-only settlement journal with monotonic sequence numbers
- Deterministic journal replay and cross-log verification
- FNV-1a content hashing throughout for audit integrity
- Dependency on `alice-ledger` only; no external crates

## Module Overview

| Module | Key Types | Description |
|--------|-----------|-------------|
| `trade` | `Trade`, `SettlementStatus` | Confirmed trade record and lifecycle state (Pending, Netted, Cleared, Settled, Failed) |
| `netting` | `NettingEngine`, `NetObligation` | Bilateral netting engine; `compute_multilateral()` applies DFS cycle cancellation |
| `clearing` | `ClearingHouse`, `ClearingError`, `ClearingResult` | Account balance management and net obligation settlement |
| `margin` | `MarginEngine`, `MarginConfig`, `MarginRequirement` | Initial, variation, and worst-case stress margin per obligation or portfolio |
| `waterfall` | `DefaultWaterfall`, `WaterfallConfig`, `WaterfallLayer` | Five-layer sequential loss absorption with per-layer result detail |
| `journal` | `SettlementJournal`, `JournalEntry`, `JournalEvent` | Append-only audit journal; five event variants |
| `replay` | `ReplayVerifier`, `ReplayStep`, `ReplayResult` | Build content-hashed replay logs and verify two logs for equality |

## Quick Start

```rust
use alice_settlement::{
    Trade, SettlementStatus,
    NettingEngine,
    ClearingHouse,
    MarginEngine, MarginConfig,
    DefaultWaterfall, WaterfallConfig,
    SettlementJournal, JournalEvent,
    ReplayVerifier,
    multilateral_net,
};

// 1. Build trades
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

// 2. Net obligations
let mut netting = NettingEngine::new();
netting.add_trade(&trade);
let obligations = netting.compute_multilateral(); // bilateral + cycle cancellation

// 3. Clear obligations
let mut ch = ClearingHouse::new();
ch.register_account(100, 1_000_000);
ch.register_account(200, 1_000_000);
let results = ch.clear_all(&obligations);

// 4. Compute margin
let engine = MarginEngine::new(MarginConfig::default());
for ob in &obligations {
    let req = engine.compute_obligation_margin(ob);
    println!("account {} total_margin {}", req.account_id, req.total_margin);
}

// 5. Waterfall for a defaulted loss
let wf = DefaultWaterfall::new(WaterfallConfig::default());
let result = wf.absorb_loss(15_000);
println!("covered={} shortfall={}", result.fully_covered, result.shortfall);

// 6. Journal and replay
let mut journal = SettlementJournal::new();
journal.record(1_000, JournalEvent::TradeReceived { trade_id: 1 });
journal.record(2_000, JournalEvent::NettingCompleted { obligation_count: obligations.len() });

let log = ReplayVerifier::build_replay_log(&journal);
let verify = ReplayVerifier::verify(&log, &log);
assert!(verify.success);

let hash = ReplayVerifier::compute_journal_hash(&journal);
println!("journal fingerprint: {:#x}", hash);
```

## Performance

Hardware-native optimizations applied throughout:

- **FNV-1a hashing** — `#[inline(always)]` crate-internal utility used for all content hashes in `margin`, `waterfall`, and `replay`. Basis `0xcbf29ce484222325`, prime `0x100000001b3`, no heap allocation.
- **Branchless max in margin** — `worst_case_stress()` uses integer mask select (`(loss > worst) as i64`) instead of conditional branches to evaluate worst-case stress across all scenarios without pipeline stalls.
- **Reciprocal pre-computation** — `MarginEngine` stores `_rcp_one: f64` as a placeholder for per-symbol multipliers, avoiding division in the hot margin path.
- **Saturating arithmetic** — `saturating_add` and `saturating_i128_to_i64` clamp accumulator overflow throughout netting and margin without panics or undefined behavior.
- **`#[inline(always)]`** on all hot constructors and accessors: `NettingEngine::new`, `NettingEngine::clear`, `ClearingHouse::new`, `ClearingHouse::register_account`, `ClearingHouse::get_account`, `SettlementJournal::new`, `SettlementJournal::entries`, `SettlementJournal::len`, `SettlementJournal::is_empty`, `SettlementJournal::last_entry`, `canonical_pair`, `saturating_i128_to_i64`.
- **`repr(u8)` enum** — `WaterfallLayer` uses `#[repr(u8)]` for compact discriminant representation and direct numeric casting in tests.
- **`Vec::with_capacity`** — Pre-allocated output vectors in `NettingEngine::compute_net` and `DefaultWaterfall::absorb_loss` to avoid incremental reallocations.

## Test Coverage

114 unit tests across 7 modules:

| Module | Tests |
|--------|-------|
| `trade` | 7 |
| `netting` | 21 |
| `clearing` | 14 |
| `margin` | 19 |
| `journal` | 8 |
| `waterfall` | 24 |
| `replay` | 21 |

Run all tests:

```bash
cargo test
```

## Release Profile

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

Full link-time optimization across all crates, single codegen unit for maximum inlining, symbols stripped from the final binary.

## License

AGPL-3.0-only. Copyright (C) 2026 Moroya Sakamoto.
