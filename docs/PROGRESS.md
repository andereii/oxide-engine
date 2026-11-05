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
## 2026-11-04 — M1 evolution: path-first classification, Player kind, broken-fixture regression

- Human-reported bug: shape-based classify could accept a malformed chart. Fixed by
  `parse_file(path, bytes)` — path-first, strict shape validation on implied kinds;
  new `Player` kind (`data/players/*`, Haxe `PlayerData.hx`); Metadata without
  `timeChanges` round-trips permissively (ADR-0005, ADR-0006).
- Regression fixture: `tests/fixtures/broken/notes-as-array/songs/regression/regression-chart.json`
  (orchestrator-owned, committed) with an xtask integration test asserting exit 1.
- Gates:
  ```
  $ cargo xtask verify                       → exit 0
    oxide-data: 30 passed, xtask broken-fixture test: 1 passed
  $ cargo xtask parse-all --dir ~/Descargas/fnf/Funkin/assets → exit 0
    character 55 ok, chart 61, level 10, metadata 78, notestyle 2,
    player 2 (new kind), stage 21, unknown 203, TOTAL 432 0 432
  $ cargo xtask parse-all --dir tests/fixtures/broken        → exit 1
    chart failed 1: Schema validation error for chart: invalid type: sequence, expected a map
  ```
- Next: unchanged — M2 assets.

## 2026-11-04 — M2 opened: contract + non-goals (no implementation yet)

- M1 independently verified before opening: `cargo xtask verify` exit 0, parse-all
  432/432, `tests/fixtures/broken` exit 1 (human).
- **Non-goals (binding):** no renderer, no window, no game state (gates are headless and
  numeric; screenshots prove nothing in M2). No Animate Atlas playback — parse the format
  and assert structure only; playback is M5/M6. No new native dependencies. No touching
  oxide-game / oxide-audio / oxide-input. No changes to oxide-data schemas (ADR only if
  one is believed wrong, then stop).
- Contract frozen in `docs/M2-CONTRACT.md`, derived from the real files and the Haxe
  oracle; measured expectations for gate A: 266 xml visited (205 atlas-root + 61 flixel-ui
  non-atlas xml), 8486 subtextures, 5684 trimmed (non-zero frame*), 4144 negative
  frameX/Y, 198 rotated, 20 missing PNG refs; animate audit: 88 dirs, 6071 sprites,
  0 failures. Packer files: none shipped (fixtures only).
- New fixtures (orchestrator-owned, committed): tests/fixtures/atlas/{ok,broken,packer}/**,
  tests/fixtures/fonts/{sparrow,angel,overflow}/**, tests/fixtures/vfs/**.
- ADRs: 0007 (two font formats; Sparrow-glyph metric semantics from AtlasText.hx),
  0008 (VFS precedence: last mod wins, per polymod source), 0009 (notify 8.2 + zip 8.2
  feature-trimmed, no C; hand-rolled lenient XML scanner in oxide-atlas).
- Next: delegate oxide-atlas to @oxide-impl-rust.

## 2026-11-05 — M2 gate A green (oxide-atlas)

- Gate: `cargo xtask parse-atlas --dir ~/Descargas/fnf/Funkin/assets` → exit 0
  (independently re-run by the orchestrator, not just the subagent):
  ```
  xml visited: 266
  atlas ok: 205
  atlas failed: 0
  non-atlas xml (skipped): 61
  subtextures: 8486
  trimmed (non-zero frame*): 5684
  negative frameX/Y: 4144
  rotated true: 198
  missing png refs: 20
  packer ok 0 failed 0 (none in assets; fixtures cover parser)
  animate ok 88 failed 0 sprites 6071
  ```
  Every number matches the pre-delegation audit (docs/M2-CONTRACT.md §0).
- Broken-XML regression: `cargo test` in xtask runs `parse-atlas` over
  `tests/fixtures/atlas/broken/numeric-garbage.xml` → exit 1 asserted
  (`xtask/tests/parse_atlas_fixture.rs`).
- `oxide-atlas`: lenient std-only XML scanner (raw `<`/`&` in attribute values,
  BOM/blank line before the declaration), Sparrow/Packer(hash+array)/Animate parsers,
  12 crate tests green. No new dependencies (ADR-0009). Deviation accepted: in-memory
  parser entry points report `uri = "<inline>"`; xtask prints the real path on stderr.
- `cargo xtask verify` → exit 0 (including parse-all 432/432 unchanged).
- Next: delegate oxide-font and oxide-vfs (parallel; disjoint crates).

## 2026-11-05 — M2 gates B and C green (oxide-font, oxide-vfs)

- Gate B (`cargo xtask test` covers it): oxide-font 14 green — fixture-derived
  (`micro-font.xml`: {A,B,-dash-}, A=45x59, B=38x59, maxH=59, case Upper,
  measure AB=(83,59), "A B"=(123,59), "A\nB"=(45,118), "AA\nA"=(90,118);
  `vcr-lite.fnt`: 5 chars faithful rows, AB=(20,18), rects fit 122x122 page,
  kerning absent+kerning-present+cross-newline cases) and real-font checks
  (default=99 prefixes/maxH 70/Both; bold=63/77/Upper; freeplay-clear=10/24/Both;
  default measure AB=(83,70)). Overflow fixture → Err(GlyphOverflowsPage) asserted
  (no-gate-weakening path: crate test).
- Gate C: oxide-vfs 4 unit + 8 gate-C integration tests green, AND green under a
  SYMLINKED TMPDIR (macOS-shape repro): (a) two-mod precedence last-wins; (b) zip mod,
  root manifest, `..`/absolute rejection; (c) case-exact `Bf.PNG` ⇏ `bf.png`, index
  enforced, event casing never enters the index; (d) duplicate ids; (e) decisive
  invalidate: post-cache-created files/dirs not resolvable until invalidate (test fails
  on no-op); (f) malformed manifest → `VfsError::Manifest`, no panic. Manifest reads
  bounded to 1 MiB. Roots canonicalized at add.
- Reviewer cycle (oxide-reviewer): verdict cycle = needs-fix → fixed (gate C(e) decisive,
  case-exact manifest discovery, bounded reads, canonical roots, invalidate grounding,
  widest-line angel measure, lowest-suffix glyph metrics, Chart song-id-only lowercase)
  → confirmed accept after final invariants-e test + symlinked-TMPDIR pass. Reject
  deviations: none outstanding; accepted decisions recorded (zip backend feature
  amendment in ADR-0009; non-existent mod root = Io NotFound, documented; mod ids
  caller-supplied per contract, ADR-0008 derivation belongs to the future loader).
- Full final orchestrator re-run: `cargo xtask verify` exit 0, 74 tests passed / 0
  failed; parse-atlas numbers identical to the frozen block.
- Deferred to later milestones (M2 non-goals respected): Animate playback AN/SD→M5/M6
  spike; Manifest Option-typing for absent fields (M8 modding hardening);
  `assets.pak` base layer (M3/M9 `oxide pack`). No renderer, no window, no oxide-data
  changes in M2.
- M2 status: all three gates green; crates committed.
- Next: M3 media (parallel allowance with M4-M6 per §8.4) — plan §7.9/§7.10 first.
