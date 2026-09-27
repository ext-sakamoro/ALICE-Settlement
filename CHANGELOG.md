# Changelog

All notable changes to ALICE-Settlement will be documented in this file.

## [Unreleased]

### Changed
- **License: `AGPL-3.0-only` → `AGPL-3.0-only OR LicenseRef-Commercial` (dual-licensed、2026-09-27)** AGPL 側の条件は変更なし (既存 AGPL 利用者への影響ゼロ)、商用という選択肢が追加されただけ SPDX が AGPL 単独だと cargo-deny / FOSSA / SBOM に「商用オプションなし」と見えるため宣言を dual に 変更点: SPDX / `LICENSE` → `LICENSE-AGPL` / `LICENSE-COMMERCIAL.md` (商用トリガー 6 条件 = クローズド製品・商用 SaaS・エッジ / ファームウェア配布・plugin 再配布・プラットフォーム NDA・保証、社内利用は AGPL 側で無償と明記) / README の選択肢表 商用窓口は法人 `contact@extoria.co.jp`

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
