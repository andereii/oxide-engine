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

## 2026-11-05 — M2 fixes (human review): `with_frame_attrs` + `frame_quad`

- FIX 1: gate A label — `trimmed (non-zero frame*)` renamed to `with_frame_attrs`.
  Measured ground truth: presence-of-any-frame-attr == any-non-zero count == 5684
  (every carrying subtexture has a non-zero frame attr); the human's alternative
  framings measured 4198 (frameX/frameY != 0) and 4838 (frameW/H differ from w/h) —
  neither equals 5684, so the number was kept and the name made truthful. Gate A
  re-run: `with_frame_attrs: 5684`, all other numbers unchanged, exit 0.
- FIX 2: `oxide_atlas::{FrameQuad, frame_quad}` — flixel `fromSparrow` semantics:
  dest_offset = (-frameX,-frameY); dest_size = frame* attrs or page dims (rotated &&
  no frame attrs → swapped, pinned though 0 occurrences in data); atlas_rect swaps
  w/h ONLY for rotated (sprite orientation). Tests: (a) untrimmed, (b) the
  frameX=-10/frameY=-5 spec shape (hand-computed 120x210/10,5), (c) real-data-shaped
  rotated frame (342,8,303,69 / 12,5 / 318,78); regression sweep over all 8486 real
  subtextures asserting the rotated invariant, no panic. gate A block unaffected.
- Data facts recorded for M5 renderer: all 198 rotated frames carry frame attrs
  (`rotated && !trimmed` branch: 0 occurrences); frameWidth/Height never present as
  "0" (3274 present-zero frameX/frameY only — offset case, semantics identical).
- Next: M3 media contract — PLAN-ES.md §7.1–§7.7, tool availability probe, corpus
  measurement.

## 2026-11-05 — M3 opened: contract, ADRs, mini fixture

- M3 scope per human brief + PLAN-ES.md §7.1–§7.7 (read this session): offline media
  packer (`oxide-media` + `oxide pack` + `assets.pak` + assets-dist), build-time only,
  ffmpeg as hard external binary (never linked; game/audio code never spawns it),
  no runtime decoding, oxide-atlas/font/vfs untouched.
- Corpus re-measured: 685 png (114 MB) / 432 json / 431 ogg (504 MB) / 413 mp3
  (477 MB) / 266 xml / 114 hxc / 25 fonts / 12 frag / 14 videos (8 mp4 122 MB +
  6 mkv 21 MB). ffmpeg 8.1.3 present with libsvtav1/libwebp/libopus (no libx264).
- Probe findings: WebP-lossless pixel compare = identical;
  bopeebo/Inst.ogg = 92 kbps → grows at 112k → copy rule validated;
  voices/inst relative first-sample delta stable (−8..0 samples @48k);
  SFX oggs as low as 13 kbps exist.
- `docs/M3-CONTRACT.md` frozen: EntryKind/Disposition classification, never-grow
  audio+image rules, EXCLUDED_VIDEOS exact-stem list, dist layout, totedinge
  accounting (media once + compressed text), gate report block, exit conditions
  (files_grew 0, align_ok n/n, duration_ok n/n, image n/n, ssim≥0.93, ≤350 MB),
  determinism (sorted entries, fixed mtime 2000-01-01, sha2 digest).
- ADR-0010 (ffmpeg external + `sha2` dep) and ADR-0011 (dist/pak layout +
  total_after semantics). ADR numbering: existing 0009 letter-ordered, no gap.
- Fixture `tests/fixtures/media/mini/**` committed (49 KB: ogg stems, SFX incl.
  32k quiet case, webp-source png, 3 videos incl. excluded-stem stub, fonts, text,
  hxc, fla skip stub). Tests using it are ffmpeg-guarded.
- Next: delegate oxide-media (incl. `oxide pack` CLI + xtask media-check/pack).

## 2026-11-05 — M3 media gate green (oxide-media + assets.pak)

- Gate: `cargo xtask media-check --in ~/Descargas/fnf/Funkin/assets --out assets-dist`
  → exit 0 (run once by the orchestrator, twice by the implementer — identical block
  and identical pak sha256 all three times):
  ```
  audio  before: 505.1 MB  after: 216.6 MB  (converted 370, copied 61)
  images before: 114.6 MB  after: 84.3 MB  (converted 633, kept png 53)
  video  before: 143.6 MB  after: 29.0 MB  (converted 11, excluded 3)
  text   before: 29.6 MB  after: 2.3 MB  (deflate in pak, 861 files)
  fonts  before: 2.0 MB  after: 2.0 MB  (25 files)
  skipped (non-runtime files): 6 (fla/git metadata)
  mp3 dropped: 413 files, 477.7 MB
  files_grew: 0            align_ok: 126/126     duration_ok: 370/370
  video_ssim_min: 0.96 (threshold 0.93)          total_after: 334.3 MB (limit 350)
  image_pixels_identical: 686/686  (pixels differing only where alpha==0: 130)
  pak sha256: dc89a2f766e2f20c88e726d265f00ceeb182855096cda5194736ab4b9e5d5a13
  excluded_videos: mobileRelease.mp4, riftCollabTrailer.mp4, boyfriendEverywhere.mp4
  ```
  total_after 334.3 MB ≤ 350 (no 350–380 ADR clause needed). assets-dist tree +
  assets.pak (350,571,996 bytes) materialized; determinism proven structurally on
  the mini fixture (byte-identical paks) AND across three independent full runs.
- **Midmilestone finding, fixed metric:** the first full run was align_ok 121/126:
  the fixed-threshold onset detector mis-measures gradual crescendos (measured on
  `milf/Inst.ogg`: crossing drifts ~53 ms on the opus curve). Replaced by the
  energy-normalized cross-correlation delay estimator (±512@8k coarse, ±64@48k
  refine; pair budget unchanged ≤128 samples relative). No thresholds were relaxed;
  contract §3 amended to describe the estimator (method change documented, numbers
  unchanged). After the fix: 126/126 with the SAME conversions.
- Reviewer cycle (needs-fix → fixed): copies now byte-compared (hash equality),
  destructive output prep replaced by marker-guarded wipe (`assets-dist.marker`),
  zip `system` byte pinned (ADR-0010 amendment), SSIM scoped to converted videos,
  symlink/git-dir walker hardening, derived-key collision preflight, `oxide pack`
  bare command implemented, coverage floor (converted artefacts must be checked).
  Verdict after fixes: accepted (gate re-run green, same sha).
- `cargo xtask verify` → exit 0 (100 tests / 0 failed; parse-all 432/432 unchanged;
  oxide-atlas/font/vfs untouched — gate A/B/C still green).
- Environment disclosure (outside the repo, for the human): Fedora's `ffmpeg-free`
  lacks HEVC/H264 decoders needed by the shipped cutscene sources; a full RPMFusion
  ffmpeg 8.1.3 was installed at `~/.local/lib/oxide-ffmpeg` with wrappers at
  `~/.local/bin/{ffmpeg,ffprobe}` (documented in ADR-0010). The media gates require
  a full ffmpeg build where HEVC cutscenes must convert.
- Deferred: pak reading via VFS release mode (M8), pak as sole release payload (M9),
  `--twice` full double-run on the real corpus (available as a flag, not a gate).
- M3 status: gate green and committed.
- Next: either M4 (clock+input) per the plan's order, or M7 (UI) if the human
  prefers the §8.4 parallel allowance. Default: M4.

## 2026-11-05 — M2/M3 cleanups (human rules): hxc excluded, visible-pixel metric, durable dist

- `assets-dist` is now a durable repo-local, gitignored artifact: regenerated at
  `<repo>/assets-dist/` with `cargo xtask pack --in ~/Descargas/fnf/Funkin/assets
  --out assets-dist` (equivalent `cargo run --bin oxide -- pack ...`). Later runs
  delete ONLY files the previous pack owned (recorded in `assets-dist.marker`);
  foreign files inside the dir survive; foreign non-marker dirs are refused.
  Two consecutive full runs byte-identical (`BLOCKS_IDENTICAL`, `PAK_BYTES_IDENTICAL`).
  The artifact currently sits at `/home/alex/Documentos/OxideEngine/oxengine/assets-dist/`
  with `assets.pak` (sha 11abb4ad…) — usable by M4/M5 gates.
- `*.hxc` (114 files, 0.4 MB) excluded from dist AND pak: the engine cannot execute
  HScript (out of scope per §10; shipped modding is Ruby/mruby). New report line
  `hxc excluded: 114 files, 0.4 MB`; pak sha256 rotated to 11abb4ad…; text group
  now 747 files / 2.2 MB. Decision: they do NOT ship "for reference" — the upstream
  Haxe source in `~/Descargas/fnf/Funkin/source` already holds every script, and the
  release payload should not carry unreachable code.
- Metric rename in gate + code + contract: `image_pixels_identical` →
  `visible_pixels_identical` (source alpha > 0; must be byte-identical). Real value:
  539,040,413/539,040,413 identical; separate informational counter
  (pixels differing only where alpha==0): 166,216,170. `align_ok` etc. unchanged
  (126/126, duration 370/370, ssim 0.96, total_after 334.2 MB ≤ 350, files_grew 0).
- `cargo xtask verify` exit 0.

## 2026-11-05 — M4 phase 1: oracle fixtures + oxide-audio core

- **Oracle tooling (committed, `tools/oracle/`)**: self-contained Haxe (Neko)
  instrumented runner — `OracleConductor.hx` transcribes `source/funkin/Conductor.hx`
  method-by-method with line citations from `Verbatim.hx` (30 entries); reads the real
  chart/metadata pairs (discovery covers the `preload/data/songs/` layout with
  `songs/` fallback), ffprobe-probes `Inst.ogg` for `music.length`, simulates
  PlayState's phase shapes (pre-roll `update(pos,false)`, playback `playing=true` with
  the clamp branch active) on a 1 ms grid, and emits one JSONL per chart file with the
  frozen field order.
- **Fixtures**: `tests/fixtures/conductor/` = 61 chart timelines + `_smoke.jsonl`
  (hand-crafted 7/8 micro-chart for unit tests). Byte-identical determinism proven by
  `snapshot_validate.sh` (builds twice, regenerates twice, compares with the committed
  set): whole-set sha256
  2a6045cb8cdcbe260f994b505dddff8353a8edff227764347f5293bdf6013da6 identical.
- **roundDecimal amendment** (parity with the installed runtime, recorded in
  M4-CONTRACT §0/§1): shipped HaxeFlixel computes roundDecimal via
  `Math.fround(v*mult)/mult` — a f32 escape; on Neko the stdlib fround is missing, so
  the oracle uses the IEEE binary32 Bytes round-trip; both the oracle and
  `oxide-audio::conductor` implement it identically.
- **`crates/oxide-audio` core**: `conductor.rs` (pure f64 port: map_time_changes with
  round-4 beat recomputation, update with playing-clamp + offsets + step-0 dispatch
  rule + step→beat→measure dispatch order, six time-conversion functions); 15 tests
  green incl. the Conductor.hx class-doc length checklist (4/4, 7/8, 15/16, 3/2 at 120
  BPM). `clock.rs`: kira 0.12 wrapper (default features OFF = simulation backend,
  1000 Hz; opt-in `real-audio` feature for M5) — ADR-0012.
- `cargo xtask verify` exit 0.
- Next: M4 chapter 2 — `crates/oxide-input` (evdev/uinput + jitter probe), then wire
  `cargo xtask fixtures-conductor` (61-chart comparison) and `cargo xtask replay`.

## 2026-11-05 — Workflow migration, rounding correction and scoped evidence

- Authorization: human approved the complete reviewed workflow plan and selective
  cleanroom-inspired behavior/specification separation; ADR-0013/0014 record scope.
  Initial HEAD `733c39c`, clean tree. No Git mutations/commits or original-tree writes.
- Active instructions are now repository-local: AGENTS, docs/GOAL, docs/WORKFLOW and
  role-specific agents. Outer plan section 8 is amended; old copies are retired pointers.
  Contracts permit a substantiated objection, private simplification and stronger tests;
  no automatic scope cuts. Resume commands no longer bootstrap the existing workspace.
- Reviewer tool policy denies shell/edit. Implementation editor roots are narrower;
  arbitrary shell means per-task isolation is still procedural, not an OS sandbox.
  Cooperative queue/snapshot/scope helpers and static lint have 48 passing Python tests.
- Conductor correction (ADR-0014): real installed FlxMath was compiled/executed in native
  hxcpp; Math.fround is floor(x+0.5), not binary32. Replaced only 41 changed timelines;
  21 unchanged, all 86,131 rows/counts/index sequences retained. Candidate review and
  source/runtime/input provenance are in tools/oracle. No tolerance was relaxed.
- `cargo xtask fixtures-conductor` → exit 0: 61/61 model timelines, 86,020 chart + 111
  separate smoke boundaries, worst delta 0.000 ms. Every silent update is replayed;
  trusted embedded manifest rejects renamed-smoke corpus substitution. This proves
  model agreement, NOT independent full-game Conductor parity or complete M4.
- `bash tools/oracle/snapshot_validate.sh` → exit 0: two builds/runs and frozen manifest
  match; set hash ec4cc791930e20de96613da519dc85d4dd0c05b7c042d2ee2521c68104c4c41f.
- Parsers reject empty input, traversal/read errors and Packer failures. Official-corpus
  mode passes 432/432 JSON and 266 XML / 205 Sparrow / 61 non-atlas / 88 Animate.
  Media preflight checks category inventory and 126 stem pairs before output/conversion.
- `cargo test --locked --workspace -- --include-ignored` with explicit OXIDE_ASSETS_ROOT
  → exit 0, including real-data/atlas/font checks and actual synthetic audio conversion.
  Mini keeps four copies and adds two conversions/duration checks/alignment pairs.
  Portable runs explicitly ignore external checks, never silently count them as passed.
- `cargo xtask verify --profile quick` → exit 0: fmt, Clippy -D warnings and portable tests;
  milestone_acceptance=false. JSON scope/metadata/deadlines/coverage are explicit.
- M1 acceptance profile with the real corpus → expected exit 2: data checks passed,
  missing authentic CI receipts/dirty current tree block acceptance. Linux/Windows CI
  workflow is versioned but was NOT executed; offline receipt checks do not authenticate
  origin. Mandatory native Conductor/input/replay/jitter and later gates stay pending.
- Rust pinned to 1.99.1. Validation used RUSTUP_TOOLCHAIN=stable after verifying that
  alias is this exact compiler; exact-name Rustup install was not performed in sandbox.
  No new Rust package dependency, provider availability probes or service restart.
  Primary models unchanged; automatic global fallback narrowed to existing DeepSeek.
- Independent reviews identified/fixed renamed-smoke, orphan-stem and descendant-timeout
  regressions. Full-corpus media conversion and actual Windows/OpenCode runtime policy
  verification were not repeated. See docs/WORKFLOW-REPORT.md for before/after and limits.
- Next: keep M4 partial; obtain independent native-game Conductor traces, then continue
  timestamped input/replay/jitter with bounded behavior-first tasks. Do not start M5.

## 2026-11-06 — Post-migration verification and full-corpus media gate

- Exact toolchain `1.99.1-x86_64-unknown-linux-gnu` (with `rustfmt` and `clippy`) is now
  installed locally; `RUSTUP_TOOLCHAIN=stable` workaround is no longer required.
- Verified `opencode debug agents` (OpenCode 2.2.5) loads all 5 agents with the V2
  last-match-wins permission rules; documented CWD (`oxengine/`) requirement in `docs/WORKFLOW.md`.
- Marked `PLAN-ES.md` §11 as historical so it does not contradict `/oxide-continue`.
- `python3 tools/workflow.py run --timeout 900 -- cargo xtask verify --profile milestone --milestone M4 --assets /home/alex/Descargas/fnf/Funkin/assets --report target/verify-m4.json`
  → expected exit 2 (`milestone_acceptance=false`): all implemented M0–M4 phase 1 gates
  (`fmt`, `clippy`, `tests`, `parse_all_corpus`, `real_data`, `parse_atlas_corpus`,
  `real_atlas_geometry`, `real_fonts`, `media_mini_*`, full `media_corpus` conversion,
  and `conductor` 61-chart + smoke replay) passed; blocked as expected on `dual_platform_ci`
  and unimplemented `conductor_native_reference`, `replay`, and `input_jitter`.
- `bash ../preflight.sh` → exit 0 (0 lint errors).
- Next: M4 remaining obligations — native-game Conductor trace validation, `crates/oxide-input`
  (timestamped capture + jitter probe), and `cargo xtask replay`.

## 2026-11-06 — M4 input phase 1: crates/oxide-input + `cargo xtask input-jitter`

- Objective: implement the M4 contract §3 timestamped capture crate (Linux evdev real
  device, uinput virtual device, caller-supplied audio time, kernel µs timestamps) and
  the contract §4 `input-jitter` standalone report command. Delegated crate work to one
  `@oxide-impl-rust` worker (write set: crates/oxide-input/**) after an analyst pass
  over the original input sources; manifests/lockfile/ADR/xtask gate were orchestrator-
  owned and disjoint.
- Contract-placeholder resolution (ADR-0015, reviewer-checked): `SECONDARY_CODES` is 8
  Linux u16 codes [17,30,31,32,103,105,106,108] (W A S D + UP/LEFT/RIGHT/DOWN), derived
  from the observed default Solo NOTE_* bindings (`Controls.getDefaultKeybinds`) that
  `PreciseInputManager` binds exclusively; the contract's unobserved "[u16; 7]"
  placeholder was superseded. Save-format FlxKey Int codes are NOT converted silently.
- Worker deviations resolved by the orchestrator: (1) `thiserror` had not been wired
  (my manifest omission) — wired `thiserror = { workspace = true }` and switched the
  manual `Display`/`Error` impls to the derive; (2) accepted the disclosed uinput ACL
  shim (virtual device advertises two never-emitted BTN_* codes so udev applies
  uaccess; ≤1 s retry on permission-denied; lasting denial still surfaces as
  `PermissionDenied`) and the non-Linux `UnsupportedPlatform` stub for Windows CI.
- `python3 tools/workflow.py run --timeout 900 -- cargo test -p oxide-input` → exit 0
  (12 unit + 5 portable + 2 external tests); `-- --include-ignored` → exit 0 (real
  device permission variant and uinput probe round-trip pass on this host).
- `cargo fmt --all -- --check` → exit 0; `cargo clippy -p oxide-input --all-targets
  -- -D warnings` and `-p xtask` → exit 0. `cargo xtask verify --profile quick` →
  exit 0 (milestone acceptance: false).
- `python3 tools/workflow.py run --timeout 300 -- cargo xtask input-jitter` → exit 0:
  samples 2000 p50 0.020 ms p99 0.051 ms (≤2 ms target; mitigation label if above),
  and `--samples 500` → p99 0.068 ms; `--bogus` usage error exits 2. Output labels the
  measurement as uinput delivery jitter only, NOT audio-clock correlation and NOT
  input_jitter milestone acceptance.
- Scope audit (workflow.py scope vs 00:26 baseline) → one violation: `opencode.json`
  changed at 00:51 by the running OpenCode application itself (provider-fallback/
  cooldown rewrite: `prefill_cap_refused`/`cell is at capacity` patterns, fallback list
  reordered). Not written by me or the subagent; left unmodified pending human
  awareness. All other 11 changed paths within the allowed sets.
- Review: `@oxide-reviewer` verdict approve with comments; the single ADR-0015 wording
  finding (dependency closure, ACL-shim disclosure, host-specific term) was fixed.
  Defects found in code: none. `verify profiles` unchanged.
- Limitations: `input_jitter`/`replay` remain pending acceptance gates (platform-
  specific evidence pending; this probe never measures hardware audio latency);
  Windows/non-Linux stub not compiled locally; real-device success path still requires
  group `input`; ADR-0010/0014-era fixtures untouched; no Git writes.
- Next: M4 remaining — deterministic `cargo xtask replay` gate (synthesised chart
  inputs, sha256-identical reruns), then the native-game Conductor trace objective.
  Also: decide whether the `input_jitter` verify gate moves from pending with
  hardware-evidence wording or stays pending (owner: verify catalogue, review first).

## 2026-11-06 — M4 replay gate: `cargo xtask replay` (deterministic, fail-closed)

- Objective: contract §4 replay gate — deterministic hits/misses/score report from a
  chart-synthesized stream with a two-pass sha256 ordering proof. Implemented
  directly by the orchestrator as a coherent xtask tooling task (one objective,
  explicit write set: xtask/src/replay.rs, xtask/src/main.rs arm, xtask/Cargo.toml
  sha2 dep, Cargo.lock, docs). Two failed incrementalauthoring attempts were
  stopped per the two-fix rule; the module was then written atomically and
  compiled first-try (recorded as process learning, not as a code defect).
- Observed (recorded, ADR-0016): chart note `t` is song-position ms compared
  directly against `Conductor.songPosition` via `Std.int(...)` in the original
  (PlayState.hx L3118); sustains are wire `l` ms. The contract's implied
  fixture-timestamp round-trip was superseded with the equivalence argument in
  the ADR (using the model timeline would only re-add rounding drift).
- Engine decisions: 1 ms grid snap presses (`ceil(t)`), `inputLatencyMs = 0`,
  difficulty union from the chart file (BTreeMap order), 350/hit score
  placeholder (real windows measured at M5), lane-merge matching (earliest
  pending note with Std.int == 0, exhausted notes count as missed).
- Integrity: chart bytes must match the committed provenance sha256 exactly and
  the fixture id must resolve uniquely; `--assets`/`OXIDE_ASSETS_ROOT` follows the
  existing assets-directory convention (`preload/...` path composition, explicit
  `assets/`-prefix consumption, no silent reparenting).
- Evidence: `cargo test -p xtask --bin xtask` → exit 0 (9 replay unit tests incl.
  negative miss + double-pass determinism; whole binary suite 30 tests pass);
  `cargo xtask replay --fixture 2hot-default --assets …/Funkin/assets` (twice) →
  hits 2762 misses 0 score 966700, sha256
  328fdee4cc780cedbbe0fd851a84a7c055039390efadf055aeb0ed618a87ad78 equal across
  runs; bopeebo-erect → 1090 hits; philly-nice-erect → 1574; blammed-pico → 1811
  (all misses 0, synthesized perfect stream). Wrong `--assets` → explicit
  fail (exit 1); unknown fixture id → exit 1 fail-closed; missing `--fixture` →
  usage exit 2. fmt/clippy -D warnings → exit 0; `cargo xtask verify --profile
  quick` → exit 0 (milestone acceptance: false). Hit counts cross-checked against
  the raw chart JSON note sums (2762, 1090 exact).
- Limitations: synthesized perfect stream only (no human replay, no jittered
  stream, no sustain scoring); `verify`'s `replay` and `input_jitter` milestone
  gates remain pending (catalogue wiring is a separate reviewed step); no
  native-game note-parity claim; sha256 equality proves self-consistency, not
  external determinism of hardware paths.
- Next: reviewer round on this gate batch; then keep the M4 native-game conductor
  trace objective, and later arbitration of whether `replay`/`input_jitter` gates
  leave the pending catalogue with hardware-evidence wording (owner + review).
- Review of the replay gate batch: `@oxide-reviewer` verdict "approve with comments";
  two low findings fixed — (1) ADR-0016/module doc rewritten so L3118 is described
  as the observed scoring-delta computation after the window logic (window 160 ms
  per Constants.hx; `Std.int(...)==0` is explicitly NOT the accept window), and the
  note-t-vs-fixture-timeline choice moved into the engine-decision list; the
  printed model-boundary line states "stand-in … not the accept window"; (2) the
  in-process double pass re-framed as a purity tripwire in the docs — the digest +
  two CLI runs remain the ordering evidence. Added the reviewer-requested ambiguity
  regression test (a corpus- and mod-hosted duplicate chart id now tested).
  Post-fix: tests 31/31 pass, clippy -D warnings and fmt clean, quick verify exit 0,
  and the 2hot-default sha256 digest is byte-identical (wording changes only).

## 2026-11-06 — native Conductor probe (green) + verify catalogue wiring (M4)

- Human-approved objectives implemented and validated:
  1. Read-only native hxcpp harness (`tools/oracle/native_conductor_probe.sh`
     + `NativeConductorProbe.hx`/`ConductorSlice.hx`/stub instrumentation/
     `compare_native_rows.py`) executing the REAL `funkin.Conductor` from the
     installed game source (unrelated fields sliced off pre-typing; no game-tree
     writes; source/copy hashes recorded). First approved run: native == model
     on ALL 62 timelines — 86,131/86,131 rows field-wise at gate tolerances
     (counts/indices/sims exact, worst ms delta 0.000; lengths/BPM 1e-6 rel).
     Row digests frozen in `tests/fixtures/conductor-native/` (62 jsonl +
     `_manifest_native.json` provenance; ~29 MB / 29 MB same scale as model).
  2. `cargo xtask conductor-native` new gate (xtask/conductor_native_check.rs
     + pub(crate) reuse of conductor_check replay machinery): three strands —
     frozen-row digest verification vs manifest; native-vs-model field
     equality; Rust-vs-native 1 ms replay. Fail-closed on any row drift or
     manifest mismatch (`cargo test -p xtask --test conductor_native_gate`:
     4/4 pass incl. drifted-rows and partial-set negatives).
  3. `verify.rs` catalogue wiring (objective 2): the three M4 obligations
     moved from pending to executable gates with explicit honesty labels:
     `conductor_native_reference` (no assets), `replay --corpus` (explicit
     `--assets` required; new `replay --corpus` CLI mode in replay.rs, all
     61 charts deterministically in one call: ok 61 failed 0), `input_jitter`
     (Linux uinput p50/p99; each label states delivery-jitter-only and that
     Windows/hardware evidence remains pending). `fuzz_parsers` and M5+
     gates stay pending. `xtask --profile milestone M4` behaves: blocked
     without CI receipts/assets (fail-closed), and the three gates PASS
     locally (conductor_native_reference reproduced exactly).
- Evidence: `python3 tools/workflow.py run --timeout 900 -- cargo xtask verify
  --profile quick` → exit 0 (milestone_acceptance=false); milestone M4
  no-assets probe: conductor/conductor_native_reference/input_jitter PASS,
  replay BLOCKED on explicit --assets, overall blocked_with_pending; fmt/
  clippy -D warnings clean; xtask binary unit tests 31/31 + gate tests
  green. Full steps logged in real logs referenced by ledger.
- Honesty labels (all printed/gated): native rows = REAL Conductor execution
  over the frozen SIMULATED recipe (no gameplay/audio/HW); model/native
  equality is the probe boundary, not a full-game integration trace; replay
  reports are synthesized stream determinism; probe evidence boundary summary
  lives in tools/oracle/BEHAVIOR.md §native; ADR-0017 records both approved
  decisions.
- Limitations: the probe's input fixed by the frozen recipe (no save/hardware/
  alternate instrumentals); the fixture freeze introduces ~29 MB of
  committed evidence files pending human review/commit acceptance; full
  milestone M4 acceptance still blocked on `dual_platform_ci` and other
  required evidence; no human replay/timed input; no GPU/AUDIO hardware runs.
- Next: single remainder objective per the ledger: reviewer round over the
  full native-probe + gate-memory batch (one review pass, both approved
  objectives), then release the remaining M4 work (native-game conductor
  traces if assets stand-in evaluation permitted, or the broader native reference extension (new ADR if the scope
  changes).

- Gate-state summary (2026-11-06 evening quick verify exit 0; milestone M4
  still blocked as designed without CI receipts/assets; three wired gates —
  conductor_native_reference PASS with no assets, input_jitter PASS
  Linux-only, replay BLOCKED on explicit --assets). Review round from
  @oxide-reviewer on the native batch: verdict BLOCK with 5 findings — all
  fixed and re-evidenced: (1) HIGH: step/beat/measure time comparisons are
  now converted to ms through the row's own lengths before the 1 ms budget in
  BOTH comparators (python + rust), matching fixtures-conductor semantics;
  per-field max-delta reporting added to both comparators and gate output;
  fresh full run: max deltas 0.000 ms across all ms fields; frozen row digests
  unchanged by the evaluation-only fixes; (2) honesty label refresh post
  wiring (no more "acceptance stays pending" claims in gate output; verify.rs
  coverage string now points at the native row manifest); (3) the three
  copied-real-source modules get distinct provenance keys and the excluded
  FloatTools/AnsiUtil source hashes are recorded; instrument hashes labelled
  documentation-only (row digests + model manifest sha equality are the
  operative fail-closed verification); (4) the real clamp divergence between
  installed FloatTools and the contract's §0 reading recorded in BEHAVIOR.md
  + ADR-0017 with a note that the corpus cannot discriminate it (no claim of
  clamp parity made; no tolerance changed); (5) replay --corpus now
  fail-closes on a drifted (non-61) provenance inventory; usage line
  includes conductor-native; native probe now refuses nonzero recorded avOff
  (input-truth fail-closed) instead of silently writing 0; conductor_native
  usage error test asserts exit code 2. All reviewer requested checks met.
- Instrumentation trim: `RowDeltas` in the native gate now tracks only the five
  ms fields it reports (step/beat/measure are exact integer comparisons).
  Post-trim state: clippy -D warnings clean, fmt clean, `conductor-native`
  gate → native ok 62/62, worst/max deltas 0.000 ms, quick verify exit 0.

## 2026-11-06 — authorized local commits (no push) + hashes

Working tree committed in three logical batches per human approval (recorded by
the orchestrator; nothing pushed; no history rewrite):

- C1 3167efa — `m4: behavior-first workflow migration + ADR-0014 corrected
  fround fixtures` (policy/roles/queue/CI + gate catalogue + parser/media
  hardening + the 41 corrected conductor timelines + native rounding probe
  evidence).
- C2 33039cd — `m4: oxide-input — timestamped evdev capture + uinput probe +
  ADR-0015 codeset` (input crate, jitter CLI, ADR-0015 in the ledger commit).
- C3 bce444b — `m4: replay gate + native Conductor reference probe + wired
  executive gates` (ADR-0016/0017 deliverables: replay/corpus CLI, native
  hxcpp probe harness + frozen conductor-native fixtures, xtask native gate,
  verify.rs wiring + label refresh + negative regressions).

Every intermediate state was build-verified (`cargo check --locked -p xtask`
plus the xtask gate tests per state); the final tree passes
`cargo xtask verify --profile quick` (exit 0) and the probe pipeline re-run;
the fixture freeze introduced `tests/fixtures/conductor-native/` (62 jsonl +
manifest) which remains human-reviewed evidence (no fixture regeneration ever
ran as part of the commit). Remaining docs-only changes at commit time are the
ledger updates themselves (ADR text + progress entries + hashes), committed in
the ledger commit that follows this block.

## 2026-11-06 — M5 phase 1 kickoff (analysis + contract freeze)

- Objective per human approval: analyze observed behavior and freeze
  `docs/M5-CONTRACT.md` for the bopeebo + mainStage + bf/dad/gf vertical
  slice (no Rust gameplay implementation in this phase; acceptance gates for
  the slice's observables stay explicit and pending).
- Ledger commit C4 80364be records the three build commits' hashes; the M5
  contract freeze landed as C5 d0526be (docs/M5-CONTRACT.md + the M5 gate
  pendings in xtask/src/verify.rs + this ledger entry).

## 2026-11-06 — M5 phase 1: bopeebo/mainStage vertical-slice contract freeze

- Human-approved M5 phase-1 objective delivered: `docs/M5-CONTRACT.md` frozen
  (observed/derived/decided separation with PlayState/Strumline/Scoring/
  GRhythmUtil/Constants citations) covering the bopeebo + mainStage + bf/dad/gf
  slice: PBOT1 scoring (500/−100/sigmoid; judge thresholds 45/90/135/160 and
  the commented-out killer branch), the health/combo/tally accounting, the
  per-frame ordering (health clamp L1213-1215 → death check L1255 →
  processInputQueue L1327 → processNotes L1332), the strumline note geometry
  (104/112/−28.6/pad 16; render distance = height/0.45 per speed >=1),
  the game-over transition trigger, the character/stage/audio data facts, and
  the phase-1 acceptance artifacts (a NEW pending `gameplay_sim_bopeebo` gate
  with a normative event-driven trace schema plus the two-run sha256 rule, the
  scripted negative stream incl. full-path 160/161 window-edge cases, hold-drop
  score penalties with the shipped zero health constants, unconditional ghost
  penalties, and a NEW pending `native_note_reference` gate pending human
  ADR-0014-style approval). `xtask/src/verify.rs` now has the M5 gate pendings
  catalogue-listed (render_diff/perf_budget/gameplay_sim_bopeebo/
  native_note_reference).
- Review rounds: three `@oxide-reviewer` passes over the freeze — round 1: 10
  findings (metadata/stage facts wrong, negative-case judgement wrong, hold
  semantics wrong, geometry constants wrong, API boundary underspecified);
  round 2: 8 findings (ghost-gating, backgroundWidth 480/edge 464, §2-vs-§3
  incoherence, startDeath citation, citation ranges, frame ordering, small
  typos, device-timestamp digest conflict); round 3: 12 findings spanning
  those still-unsaved edits plus trace-edge specifics. All findings from all
  rounds are now incorporated: asset facts re-measured from the real files,
  the 110/160/161 full-path traces pinned to the actual per-frame semantics
  (the 161 press still triggers a HIT-path miss judge with no health change
  and a combo INCREMENT — recorded odd-but-true), hold_tick/drop fields
  (score_delta/health_delta accrual, f64 penalties, is_combo_break with the
  hard-coded true), unconditional ghost rules, background width 480/edge 464,
  citation-range verification, M5 pending catalogue wiring, the input-event
  ownership + asset-read path via the M1/VFS crates, and the phase-2
  extension rule (add-only).
- Honesty: the frozen contract covers the M5 slice ONLY; nothing claims
  native-game parity beyond the simulated semantics; `native_note_reference`
  stays pending on human approval; render_diff/perf_budget remain the M5
  visual deliverables.
- Limitations: the M5 gate implementations stay pending (their sim/trace
  wiring is phase 2+); no Rust gameplay code was written in this phase per
  the approved scope; asset/behavior facts derived from the reference
  checkout remain source-informed observations (independent full-game
  verification stays unfinished); the LLM review was read-only (its findings
  not independently re-executed as code paths).

## 2026-11-06 — M5 phase 2a: oxide-game slice sim + gameplay_sim_bopeebo gate wired

- Objective (in-scope continuation of the frozen M5 contract; no human approval
  required): implement `crates/oxide-game` `slice` (§3 boundary, currently
  compiled) and wire the `gameplay_sim_bopeebo` xtask gate (§2 normative
  schema). Two `@oxide-impl-rust` workers ran sequentially (non-overlapping:
  `crates/oxide-game/`, then `xtask/`); orchestrator owned Cargo.lock.
- Deliverable 1 — `oxide_game::slice` (923-line module + 19 unit tests + 1
  golden integration test): PBOT1 scoring/judgement with Std.int truncation,
  health/combo/tallies in absolute units, 1 ms grid `step` honoring the frozen
  per-frame order (clamp+death at start → processInputQueue → processNotes),
  160/161 full-path odd-but-true behavior, ghost/hold/drop semantics, VFS +
  oxide-data read path, fail-closed build errors, serde §2 field order,
  two-run byte-stability golden (synthetic fixture, CRLF-safe compare).
- Deliverable 2 — `cargo xtask gameplay-sim --assets DIR`:
  provenance-verified bopeebo chart+metadata (sha256 fail-closed), M4 stream
  recipe (press ceil(t), release ceil(t+l)), 1:1 InputEvent conversion,
  scripted negative phase on the REAL corpus (threshold edges 45/46/90/91/135/
  136 + 110-late + 160/161 full path incl. tallies/combo_max, ghost press,
  exact-160 boundary drop (remaining_ms bitwise == 160.0, mandatory),
  remaining>160 big drop, full-hold tick rates 0.00012/0.25), jsonl traces in
  target/gameplay-sim/ (never committed), two-run sha256 equality per
  difficulty, honesty labels printed; catalogue moved pending→executable
  (milestone 5, BASELINE deps, 600 s, assets required/fail-closed).
- Evidence: `cargo xtask verify --profile quick` exit 0 (workspace; report
  target/verify.json; quick = NOT milestone acceptance). Per-worker queue runs
  `cargo test -p oxide-game` (19+1 pass) / `cargo test -p xtask` (42+integration
  suites pass), clippy −D warnings and fmt --check exit 0 each crate. Real
  corpus gate run (assets /home/alex/Descargas/fnf/Funkin/assets) exit 0: easy
  19004 events `63f3e1c9…f3ca`, normal `c8b9466e…0968`, hard `e082738e…45a4c`
  (two-run digests matched; boundary remaining exactly 160.0 on all three).
- Reviews: `@oxide-reviewer` round 1 on the crate applied 6 findings (fmt one
  refuted by re-run; terminal-dead-state state fix is the substantive one);
  round 2 approved crate with 2 low test-strength comments (orchestrator
  applied: remainder-tick-before-hold_end order pin; dead-state fixture with
  sustain + detecting-tick press). Gate review round 1: 2 majors (full §2
  threshold set incl. tallies/combo_max/tick rates; exact-160 boundary
  mandatory, soft fallback removed) fixed and re-approved (zero findings).
- Scope audit: `tools/workflow.py scope` vs pre-batch baseline: 14 paths, 0
  violations (crates/oxide-game/ + xtask/ + mechanical Cargo.lock edges only).
- Recorded engine decisions in docs/DECISIONS.md (terminal game-over state,
  exact-160 mandatory boundary, trace-artifact conventions). Honesty: all of
  this is deterministic sim of the frozen rule set over a synthesized stream —
  NOT native-game parity; `native_note_reference` (human approval required),
  `render_diff`, `perf_budget` remain pending and M5 acceptance stays blocked.

## 2026-11-07 — native hxcpp note-reference probe (native_note_reference) green + land/hold semantics corrections

- Human-approved objective #2 of the M5 phase-2a follow-up implemented and
  validated: a read-only native hxcpp probe (`tools/oracle/note_probe.sh` +
  retained-body-only Haxe slices/stubs in `tools/oracle/note-probe/`) executing
  the REAL installed `Scoring`/`GRhythmUtil`/`PlayState` note-score path
  (macros drop fields, never modify bodies; copied==original sha256 pairs
  recorded) over the bopeebo synthesized streams; comparator field-wise
  (ints/bools/strings exact, f64 <=1e-9 labelled, order/sims/key-order exact);
  `cargo xtask note-native` gate wired (`xtask/src/note_native_check.rs` +
  verify.rs catalogue: milestone 5, explicit `--assets` fail-closed) with the
  fixtures frozen by the orchestrator into `tests/fixtures/note-native/`
  (streams `9737e8a0…/b3c02ffa…/a1cd8b4c…`, rows 18880/17224/14216 digests
  `923ce1cb…/ae138208…/3c1acd54…`, manifest with real chart `2d1fd06f…` /
  metadata `f891cd53…`).
- Evidence: frozen-fixture gate run exit 0 PASS (worst strict f64 delta
  4.663e-15; labeled health-clamp exclusions easy 58/29 rows first sim 31200,
  normal 66/33 first 31200, hard 78/39 first 35700, last 75600); `cargo xtask
  verify --profile quick` exit 0 (57 xtask bin tests, 12 note_native_gate CLI
  tests incl. reordered-row negative, 21+1 oxide-game, fmt/clippy clean,
  milestone_acceptance=false); probe negative battery exit 2 on all five
  conditions; scope audit 98 paths, 0 violations.
- The native round falsified FOUR sim errors and the review round (HIGH) a
  frame-order misread, all fixed and revalidated: (1) sub-threshold hold drops
  dispatch no event (is_combo_break=false, combo unchanged); (2) the real
  hold mirror runs at frame start with the previous songPosition
  (PlayState.update super.update L1035 first; Strumline.hx L632→L745 chain:
  drop detect → completion → keep-clipped → recompute with `end - P`), the
  stale snapshot at the drop branch is `end-(R-1)`, row at release+1; the
  `ceil(t+l-160)` boundary release is PENALIZED (stale 161, -20.125) and the
  zero-penalty boundary is `ceil(t+l-159)` (stale bitwise 160.0) — revising
  ADR-0018's convention (addendum recorded); (3) windowed misses do not bump
  total_notes_hit; (4) hold_drop.input_ms = 0.0. Recipe releases at
  `ceil(t+l)=end` end as zero-penalty drops; hold_end exists only for
  key-still-held completions (ceil(end)+2). Labeled unclamped-health crossing
  (update() not retained) is the single engine-decision divergence.
- Reviews: round 1 verdict BLOCK on the frame-order HIGH (driver overrode the
  retained sustainLength; real ordering verified in source); round 2 found no
  behavioral defect and listed documentation-consistency items, all applied:
  ADR-0019 rewritten to the round-3 regime/counts; M5-CONTRACT §7 corrigendum
  item 3 now corrects the §2 exact-160 and full-hold wording explicitly with
  frame-start mirror semantics; NOTE-BEHAVIOR §6.1 item 9 handles the
  fractional-end release (drop at ceil(end)+1, completion only at R >= end+1
  with the key path); ADR-0018 addendum pointer added; NOTE-BEHAVIOR §9 item
  17 marks superseded items and the stale NativeNoteProbe header comment
  (kept unmodified to preserve frozen source hashes).
- ADRs: ADR-0019 (probe decisions, frozen set, clamp policy, trust
  limitation), ADR-0018 addendum. Limitations: no full-game parity claim
  (probe boundary only; real-game integration traces separate); the gate does
  not authenticate fixture files (vs the conductor gate's embedded manifest —
  documented); no Windows/CI evidence; `worktree`-level attribution is not
  claimed (shared worktree).
- Next step: approval #3 — M5 phase-2b: stage/character/strumline rendering and
  animation groundwork toward `render_diff` and `perf_budget` vertical slice.

## 2026-11-07 — M5 phase-2a follow-up batch committed (human approval #1 executed)

- Human approved committing the phase-2a follow-up batch (tools/oracle/note*,
  tests/fixtures/note-native/, crates/oxide-game, xtask, docs/) to the local
  branch (no push). Committed as `a59ea65` ("m5(phase-2a): native
  note-reference probe + hold-mirror/miss corrections (ADR-0019)"; 99 files,
  +292992/−451 lines). No push, no history rewrite; orchestrator-coordinated
  Git write.
- Pre-commit evidence on the exact tree content: `python3 tools/workflow.py
  run --timeout 1800 -- cargo xtask verify --profile quick` exit 0
  (fmt/clippy/tests pass; quick = NOT milestone acceptance), workflow lint
  PASS, workflow unit suite 48 OK.
- Immediate next objective (human approval #3): M5 phase 2b/3 — mainStage
  stage sprites, bf/dad/gf character rendering/animation, strumline
  note/sustain rendering, then wire `render_diff` and `perf_budget` gates.

## 2026-11-07 — M5 phase 2b: headless Bevy visual slice + render_diff/perf_budget gates wired and passing (local)

- Objective (human approval #3): stage mainStage, characters bf/dad/gf,
  strumline rendering/animation implemented in `crates/oxide-game` (Bevy
  =0.20.1, license MIT OR Apache-2.0 verified; manifests/Cargo.lock owned by
  the orchestrator) behind the frozen boundary
  `SliceRenderer{build,step,frame,build_for_adapter}` /
  `AdapterPolicy{Software,AnyHardware}`, plus the M5 `render_diff` and
  `perf_budget` gates in `xtask/` with fail-closed fixtures
  (`tests/fixtures/render-slice/`, established once by the orchestrator
  after review — never regenerated during verification).
- Spec: `tools/oracle/RENDER-BEHAVIOR.md` (OBSERVED/DERIVED labels; two
  analyst rounds), contract additions in `docs/M5-CONTRACT.md` §8 and
  `docs/DECISIONS.md` ADR-0020 + three addenda (adapter policies, debug-build
  guard, capture swap, review resolutions).
- Feasibility probe (throwaway project outside the repo): headless
  render-to-image works (`WGPU_ADAPTER_NAME=llvmpipe`, byte-identical PNG
  twice; NVIDIA default-backends segfaulted, vulkan-pinned works later).
- Implementation: `oxide_game::render` (geometry/anim/scene/load/harness):
  stage/slot/feet placement, additive camera offsets, follow/bop/decay
  math, strumline receptors/notes/trails per the frozen formulas, character
  sing/dance scheduling (bf dance restart, gf danceLeft-first alternation,
  `-hold`, singTimeSteps), per-tick order per §6. Direct GPU readback
  (persistent COPY_SRC target + RenderGraph copy node + bounded buffer map)
  replacing the flaky per-frame `Screenshot` capture (observed: 3/3 warmup
  timeouts, one instrumented PASS, one timeout at frame 116 on NVIDIA —
  evidence recorded in ADR-0020 addendum 2).
- Distributions: W1 oxide-atlas `animate_stage` (BTA parse + symbol/label
  resolution + rotated-tile matrix `[0,1,-1,0,w,0]` confirmed by the observed
  dad bounds; 29 portable + 3 real-asset tests), W2 renderer, W3 gates; two
  review rounds (round 1 APPROVED with comments on atlas; aggregate round 1
  BLOCK: bf singTime default must be the observed 8.0, props LINEAR sampler,
  chart-event exclusion had to be an explicit scope decision, opponent-hold
  pruning, dropped holds hidden, covers/splashes exclusions, perf gate must
  step real frames, trace-digest comparison, bounds constants; re-review 2
  APPROVE with comments: hold width must be `(416/8)*0.7 = 36.4` (the 52 was
  my transcription error), opponent prune `> end_ms`, player confirm
  post-finish plays press while held; all applied).
- Gates (local evidence; NOT milestone acceptance):
  `cargo xtask render-diff --assets ...` exit 0 TWICE on the frozen fixture
  (schema render-slice-1; samples 27×3 difficulties; per-frame/run/trace
  digests + geometry fail-closed; adapter llvmpipe pinned; digests
  adapter-specific). `target/release/xtask perf-budget` exit 0 (AnyHardware
  → NVIDIA GTX 1050 Ti, driver 580.196.07; 16-step 60 Hz discretized frames,
  mean 3.525 ms / max 5.798 ms vs budgets 16.667/33.333 ms; DebugBuild fail
  closed observed in a debug run). Quick `cargo xtask verify` exit 0.
- Honesty: the render fixtures are OUR renderer's frames (engine fixture);
  they are NOT native pixel parity — native-game rendered-output trace
  remains a separate pending M5 obligation; chart song events
  (FocusCamera/PlayAnimation) and hold covers/note splashes are excluded by
  explicit recorded scope decisions with parity consequences; note/
  receptor/hold NEAREST filtering is our renderer choice (native funkin path
  is linear); decisions recorded in M5-CONTRACT §8 / ADR-0020 addenda. The
  perf numbers are platform-specific hardware measurements on this machine,
  not CI/Windows/hardware-latency claims. xtask edits were within its
  assigned tooling task; one W2 manifest-adjacent edit (oxide-atlas dep)
  was orchestrated and approved; atlas fixture creation by W1 used shell
  because editor policy denies fixtures (recorded deviation; additive-only).
- Scope audit: 30 changed paths inside the union write set
  (crates/oxide-atlas, crates/oxide-game, xtask, tests/fixtures/atlas,
  tests/fixtures/render-slice, docs, RENDER-BEHAVIOR.md, Cargo.lock, two
  crate Cargo.toml files); single violation = deletion of my throwaway
  `out.png` probe artifact at the repo root (no code). M5 acceptance stays
  blocked on the pending native rendered-output/window-platform obligations.

## 2026-11-07 — M5 phase-2b batch committed (human approval executed)

- Human-approved local commit of the phase-2b batch (no push): `1ad8727`
  ("m5(phase-2b): headless Bevy visual slice + render_diff/perf_budget gates
  (ADR-0020)"). Working tree clean except this ledger entry.
- Next objective (human approval): interactive playable window mode in the
  oxide CLI (`oxide play --assets <dir> [--difficulty easy|normal|hard]`):
  real 1280x720 Bevy window, live audio (Inst + Voices-bf + Voices-dad),
  WASD/arrow input, HUD/camera/character/strumline rendering for the bopeebo
  vertical slice.
