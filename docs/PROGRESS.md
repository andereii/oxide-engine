# Progress ledger

Append-only. Format: date, milestone, gate command, key output, notes, next step.

---

## 2026-11-04 — M0 skeleton green

- Gate: `cargo xtask verify` → exit 0 (baseline compile clean, 0 tests: fmt + clippy -D warnings + test on stub workspace).
- Unimplemented gates verified: `cargo xtask {parse-all, fixtures-conductor, replay, perf-budget, media-check, pack}` → each exits 2 with "not implemented yet".
- Workspace: 10 `oxide-*` crates (stubs), root binary `oxide` (subcommands exit 2), `rust-toolchain.toml` (stable + clippy/rustfmt), `.cargo/config.toml` alias `xtask = "run -p xtask --"`.
- Toolchain installed today: Rust 1.99.1 stable (rustup, minimal) — machine had none.
- Next: M1 `oxide-data` implementation.

## 2026-11-04 — M1 data green

- Gate: `cargo xtask verify` → exit 0; includes new `cargo xtask parse-all --dir ~/Descargas/fnf/Funkin/assets`:
  ```
  kind             ok failed  total
  character         55      0     55
  chart             61      0     61
  level             10      0     10
  metadata          78      0     78
  notestyle          2      0      2
  stage             21      0     21
  unknown          205      0    205
  TOTAL            432      0    432
  ```
- All 432 asset JSONs parse, validate and round-trip (semantic numeric equality; strict equality for untyped files).
- `oxide-data`: serde schemas (chart 2.0.0, metadata 2.2.4 + legacy 2.0.0, noteStyle 1.1.0, character 1.0.0/1.0.1, stage 1.0.0, level 1.0.0, save), `EventRegistry` (7 base event names), `classify`, `parse_any` (BOM-aware). 23 unit/integration tests green.
- Measured deltas vs plan claims (plan §1): metadata = 61 song + 17 music = 78; shape-classified characters = 55 (includes sserafim/variant/counting differences vs the plan's 43-by-directory); totals 61/61/21/432 match.
- parse-all features: `--dir`, `--diff` (per-failure first-mismatch path), `--all` (list every failure). Failure cap 10 without flags.
- Next: M2 assets (`oxide-atlas`, `oxide-font`, `oxide-vfs`).

