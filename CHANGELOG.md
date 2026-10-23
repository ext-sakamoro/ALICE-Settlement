# Changelog

All notable changes to ALICE-Settlement will be documented in this file.

## [Unreleased]

### Security
- CI: `security-audit.yml` を追加 毎回新しい解決で `cargo audit` と `cargo deny --all-features check all` を走らせ、解決と deny の graph がこの crate を含むこと (0 件は失敗) を検査する ignore する勧告は無い 週次は月曜 02:11 UTC
- `deny.toml` を追加: alice-settlement の AGPL 側を crate 単位の exception で許可、alice-sign の git source を許可

### Added
- CI: `Test (public siblings)` job `cargo test --all-features` を走らせ、cargo の失敗、失敗した試験、通った試験が 167 件未満のどれでも失敗 `ci/**` の push でも走る `scripts/preflight.sh` は生成し直して同じ step を持つ

### Changed
- `merkle_trail` の Ed25519 / SHA-256 (`hash_data` / `Hash`) / `MerkleTree` を `alice-sign` (MIT OR Apache-2.0、公開、git rev 5420e71) から取るようにした 型と鍵・署名・hash・Merkle root の byte 列は同じ (これまでの依存先も同じ型を `alice-sign` から再公開していた) どこからも使っていなかった `alice-ledger` の path 依存を外した 公開の source だけで build できる
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
