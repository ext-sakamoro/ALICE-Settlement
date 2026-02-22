# Changelog

All notable changes to ALICE-Settlement will be documented in this file.

## [0.1.0] - 2026-02-23

### Added
- `trade` — `Trade` and `SettlementStatus` (Pending → Netted → Cleared → Settled / Failed)
- `netting` — `NettingEngine` bilateral netting and `multilateral_net` reduction
- `clearing` — `ClearingHouse` with `ClearingAccount` fund management and transfer
- `margin` — SPAN-style `MarginEngine` (initial, variation, stress margin)
- `journal` — `SettlementJournal` append-only hash-chained event log
- `replay` — `ReplayVerifier` deterministic journal replay with discrepancy detection
- `waterfall` — `DefaultWaterfall` loss-absorption cascade with configurable layers
- FNV-1a shared hash utility
- Integration with ALICE-Ledger order types
- 115 tests (114 unit + 1 doc-test)
