# Decision records (ADRs)

One entry per non-trivial decision: context, options, decision, consequences,
and what would make us revisit it. Gate/test/threshold changes MUST have an ADR.

---

## ADR-0001 — Workspace structure and deferred heavy dependencies

- **Context / date:** 2026-11-04, M0 skeleton. Plan §3 lists the final stack (Bevy
  =0.20, kira, symphonia, libopus...).
- **Options:** (A) declare all final dependencies in M0 so the skeleton mirrors
  the final stack; (B) stub crates with zero dependencies and add each
  dependency when its crate gets real work.
- **Decision:** B. Each `oxide-*` crate is an empty stub in M0; `oxide-data`
  starts with `serde` + `serde_json` + `thiserror`. Bevy `=0.20` is added with
  an ADR when `oxide-game` (or the CI gate) first needs it.
- **Consequences:** fast M0 cycles; every dependency addition stays visible and
  costed. Risk: none — dependencies are additive.
- **Revisit when:** a milestone gate requires the dependency earlier than planned.

- `serde` 1.x (Apache/MIT): JSON data formats are the core of M1. Build cost:
  seconds.
- `serde_json` 1.x (Apache/MIT): parse + round-trip of the 432 asset JSONs.
  Build cost: seconds; pure Rust.
- `thiserror` 2.x (Apache/MIT): per-crate error enums, convention in AGENTS.md.
  Build cost: seconds.

## ADR-0002 — Strip a leading UTF-8 BOM before JSON parsing

- **Context / date:** 2026-11-04, M1. 37 shipped asset JSON files (Animate Atlas
  `spritemap1.json` under `images/...`) carry a UTF-8 BOM; `serde_json` fails on it.
- **Options:** (A) treat those files as invalid data; (B) strip one leading BOM in
  `oxide_data::parse_any` and in the gate's reparse.
- **Decision:** B. The BOM is a property of the shipped data. `parse_all` re-parses raw
  bytes with the same strip, so both sides see identical input.
- **Consequences:** parse-all counts those 37 files under `unknown` and they round-trip.
  No fixture or gate weakened.
- **Revisit when:** a parser misbehaves because a *secondary* BOM appears mid-stream.

## ADR-0003 — Fields that only exist as JSON shapes stay verbatim (`Value`)

- **Context / date:** 2026-11-04, M1 contract v2. Real asset files disagree with the
  Haxe types in three places: metadata `offsets` is freeform (`{"vocals":{"bf":0,"dad":-8}}`),
  level `capsule` and stage `characters` have unstructured stamps.
- **Decision:** keep those three as `Option<serde_json::Value>` verbatim in M1 instead of
  inventing field shapes. Everything else is typed. Key presence is preservation-enforced:
  an original `[]` must serialize (`stage props`), an absent key must stay absent
  (`songVariations`).
- **Consequences:** lossless round-trip over all 432 files is guaranteed by the gate;
  typed refinement of these three shapes is deferred to the milestone that consumes them
  (M5 gameplay for `offsets`/`characters`, M7 freeplay for `capsule`).
- **Revisit when:** gameplay code needs typed access to one of them, with the Haxe oracle
  as the field-name source.

## ADR-0004 — parse-all kind taxonomy is shape-based, not directory-based
- **Context / date:** 2026-11-04, M1. The plan (§1) claims "43 characters"; a shape-based
  classifier sorts 55 files into the character kind (includes `isPixel` text-anims and
  per-directory variants) and 3 dialogue boxes into unknown after the `text` guard.
- **Decision:** parse-all classifies by JSON shape (frozen rules in `crates/oxide-data/src/kind.rs`),
  counts are reported per kind, and the gate condition is "zero failed". The 432 file
  total and 61/61/21 subtotals match the plan; per-kind counts are documented in PROGRESS.md.
- **Consequences:** the gate measures parseability, not an editorial character census.
- **Revisit when:** M2/M5 needs an exact character registry count (registry, not classifier).

## ADR-0005 — Path-first classification with strict shape validation (supersedes 0004)

- **Context / date:** 2026-11-04, M1 evolution (human bug report). Shape-based
  `classify` could silently accept a malformed chart (a `"notes"` array) because
  nothing anchored a file to a kind. Measured guards: every real chart lives under
  `*/songs/`; all `*/data/characters/*.json` validate as characters.
- **Decision:** `oxide_data::parse_file(path, bytes)` classifies path-first:
  `songs/*-chart*` ⇢ Chart, `songs/*-metadata*` ⇢ Metadata, exact `data/<characters|
  stages|levels|notestyles|players>` parent pairs ⇢ Character/Stage/Level/NoteStyle/
  Player. Only files with no implied kind fall back to shape classification. An
  implied kind whose shape does not validate fails with `DataError::Schema` (gate:
  `tests/fixtures/broken` exits 1, wired as an xtask integration test). This
  *strengthens* the gate, no fixture or threshold weakened.
- **Consequences:** two-player files (`data/players/*`) become a new `Player` kind
  (schema grounded in Haxe `PlayerData.hx`; complex nested stamps stay verbatim
  `Value` same policy as ADR-0003). Real-assets table shifts: player 2, unknown 203,
  totals unchanged 432/432.
- **Revisit when:** a modder layout legitimately deviates from these dir names
  (mods define their own roots; resolution happens above this layer).

## ADR-0007 — Bitmap fonts come in two formats; Sparrow is the primary one

- **Context / date:** 2026-11-04, M2 contract. The plan assumed the three bitmap fonts
  (`preload/images/fonts/{bold,default,freeplay-clear}.xml`) carry BMFont-style
  `xoffset/yoffset/xadvance` and `<info face size>`; inspection shows they are Sparrow
  `TextureAtlas` files whose glyph frame names are `<char><4 digits>`
  (`A0000`, `#0000`, `&0000`, `-dash-0000`). The only angel-code font in the assets is
  `fonts/vcr-bmp.fnt` (`<info face size>`, `<common lineHeight>`, `<pages>/<page id file>`,
  `<chars>/<char id x y width height xoffset yoffset xadvance page>`), loaded via
  `FlxBitmapFont.fromAngelCode` in `PlayState.hx:812`.
- **Decision:** `oxide-font` supports both, as separate types: `SparrowFont` (metrics derived
  exactly like `source/funkin/ui/AtlasText.hx`: space=40, newline→`maxHeight` row advance,
  glyph advance = first frame width, `caseAllowed` via `^[A-Z]\d+$`/`^[a-z]\d+$`) and
  `AngelFont` (full angel-code attribute set incl. optional `<kernings>`; none exists in
  shipped data). Neither format unifies into one struct — no invented normalization.
- **Consequences:** gate assertions split per format; AtlasText's quirky glyph-prefix table
  (e.g. `&` → `-andpersand-`) is replicated verbatim since gameplay text depends on it.
- **Revisit when:** a third font format appears in modded data (mount through VFS by
  extension, not by guesswork).

## ADR-0008 — VFS mod-layer precedence follows Polymod: LAST mod wins

- **Context / date:** 2026-11-04, M2 contract. Layer order for `oxide-vfs` was ambiguous
  ("first mod wins" vs "last mod wins").
- **Decision:** the shipped polymod git dependency states in
  `backends/PolymodAssetLibrary.hx`: *"order matters -- mod files will load from first to
  last, with last taking precedence"*. `oxide-vfs` resolves last-added-mod-first, then the
  base assets directory. Mod id: the FUTURE game loader derives it from the mod root
  directory name (zips: file stem, ADR-0008; the `oxide-vfs` API takes ids as caller-
  supplied parameters, frozen contract §3). Duplicate ids are an error.
- **Consequences:** overrides in examples behave identically in Rust. A first-wins
  interpretation would silently break every multi-mod setup with conflicting files.
- **Revisit when:** polymod changes its documented precedence upstream.

## ADR-0010 — M3 media pipeline runs ffmpeg as an external binary; sha256 via `sha2`

- **Context / date:** 2026-11-05, M3 contract. Audio/image/video conversion
  (plan §7.4) needs real encoders; the promise "no DLLs on Windows" forbids linking.
- **Decision:** ffmpeg + ffprobe are invoked as EXTERNAL COMMANDS from `oxide pack` /
  `xtask media-check` only (build-time tooling). They are NOT crate dependencies
  (`oxide-media` never links against libav*), and game/audio code paths are statically
  forbidden from spawning them. `--check-tools` PRESENCE-guards both binaries and
  prints their version lines; no minimum version is enforced (advisory only). For the
  pak digest: `sha2` 0.10 (Apache/MIT, pure Rust, seconds of build cost; alternative:
  hand-rolled sha256 — rejected, pointless risk). Deterministic pak assembly uses the
  existing `zip` 8.x (ADR-0009 features) in write mode with the zip `system` byte
  pinned (amendment below).
- **Consequences:** media gates fail with a clear `Tools` error when ffmpeg is absent
  (test suites SKIP in that environment; the gate never passes silently). THIS
  machine note: Fedora's `ffmpeg-free` lacks the HEVC/H264 decoders needed by the
  shipped cutscene sources; a full RPMFusion ffmpeg 8.1.3 was installed OUTSIDE the
  repo at `~/.local/lib/oxide-ffmpeg` with invoker wrappers at `~/.local/bin/ffmpeg`
  and `~/.local/bin/ffprobe` (documented install; first in PATH on this machine).
- **Revisit when:** a pure-Rust AV1/WebP encoder chain reaches release-tooling quality
  (plan §7.3's rav1d is decode-only; encode stays ffmpeg).
- **Amendment (2026-11-05, reviewer):** zip 8.6 `SimpleFileOptions::default()` leaves
  the archive `system` byte OS-dependent (Dos on Windows, Unix otherwise). The pak
  writer pins it explicitly so pak bytes are host-independent at the container level;
  byte-identity remains same-machine scoped (converted media may legitimately differ
  across ffmpeg builds).

## ADR-0011 — assets-dist layout, total_after accounting, image never-grow fallback

- **Context / date:** 2026-11-05, M3 contract. Plan §7.5 sizes the release at ~340 MB
  with per-media-step numbers; the human gate demands `files_grew: 0` and
  `total_after <= 350 MB`.
- **Decision:**
  1. `assets-dist/` carries ONLY media (+fonts); text lives exclusively in
     `assets.pak` (deflate). The pak additionally STOREs the media
     (seekable), so in release the pak can replace the whole dist tree (M9).
  2. `total_after` counts media once (dist bytes minus the pak itself) plus the
     compressed text (pak bytes minus stored media bytes) — i.e. every content byte
     counted exactly once, matching §7.5's arithmetic.
  3. Images follow the same never-grow rule as audio: if webp-lossless output
     >= source bytes, the original PNG is kept (converted numbers still reported
     separately). The gate counts a kept png as `copied`, and a resumed conversion
     can never violate `files_grew: 0` by construction.
  4. The three attract trailers (mobileRelease, riftCollabTrailer,
     boyfriendEverywhere) are excluded from dist AND pak via an explicit stem list
     (ADR line cross-referenced from the contract §1).
- **Consequences:** this yields two artifact forms (loose tree + pak) with intentional
  content duplication on disk until M9; the SIZE GATE counts bytes once.
- **Revisit when:** the release layout is finalized (M9) or the 350–380 MB ADR clause
  actually triggers.

## ADR-0009 — New dependencies for M2: notify 8.2, zip 8.2 (feature-trimmed)

- **Context / date:** 2026-11-04, M2. `oxide-vfs` needs hot reload + zip mod support; the
  plan mandates `notify`. No new native dependencies allowed this milestone.
- **Decision:**
  - `notify` version=8.2 (licence CC0-1.0, pure Rust using inotify on Linux); used only for
    "path changed → invalidate" events. Build cost: seconds (small dep tree:
    `event-listener`, `crossbeam-channel`, `*, inotify`).
  - `zip` = 8.2 (MIT) with `default-features = false, features =
    ["deflate-flate2", "deflate-flate2-zlib-rs"]` so the read path is pure Rust — the
    default feature set would pull zstd-sys/lzma/bzip2 (C build) which contradicts the
    no-native-deps promise. **Amendment (2026-11-05, reviewer-corrected):** as first
    written the feature list (`deflate-flate2` alone) was unsatisfiable — zip 8.2 declares
    `flate2` with `default-features = false`, so an explicit zlib backend is required for
    the crate to compile. The chosen backend `zlib-rs` (Zlib licence, pure Rust) matches
    the flate2 `zlib-rs` feature; no `cc`/`cmake`/`*-sys` C crates enter the lockfile
    (verified by the M2 reviewer against `Cargo.lock`).
  - `oxide-atlas` adds **no** dependencies: the lenient XML scanner is hand-rolled because
    shipped Sparrow data is not std-well-formed (unescaped `<` / `&` inside attribute
    values); `xml` crates don't offer a "strict-ish but tolerant attrs" mode without
    editing semantics.
  - `serialize_json` deps in atlas: none — reuse `serde_json`/`thiserror` (already ADR-0001).
- **Consequences:** Cargo.lock gains these crates; no `build.rs` codegen; also headless.
- **Revisit when:** mod zips in the wild ship lzma/bzip2 compression (then an extra pure-
  Rust decode feature is reconsidered with a fresh ADR).

## ADR-0006 — Metadata without `timeChanges` (permissive skip)

- **Context / date:** 2026-11-04, M1 evolution (human bug report). Some metadata
  files may lack `timeChanges` entirely.
- **Options:** strict (require the key) vs permissive (default empty, omit on serialize).
- **Decision:** permissive for this one field, documented: absent key ⇢ empty
  `Vec` decode ⇢ key absent on serialize; a present-but-empty `[]` still serializes
  as `[]`. Rationale: charts always validate strongly through `timeChanges`-bearing
  metadata at gameplay time (M4 conductor gate), so early parse leniency here does
  not hide errors that matter.
- **Revisit when:** M4 finds real charts whose metadata lacks `timeChanges`
  (then strictness moves into the conductor preflight, not the parser).

## ADR-0012 — kira 0.12 as the audio clock (default features OFF; simulation backend)

- **Context / date:** 2026-11-05, M4 contract §2. The plan (§3) pins Kira plus
  symphonia/libopus as the audio stack; M4 only needs the CLOCK primitive ("Kira
  Clock as single source of truth"), not sound playback or decoding.
- **Decision:** `kira 0.12.6` (licence MIT OR Apache-2.0, pure Rust) in
  `crates/oxide-audio` with `default-features = false`: the default build uses kira's
  mock/SIMULATION backend (1000 Hz tick, pure Rust deps, ~6 s cold build, no C/C++)
  so CI and the M4 gates need no OS audio. An opt-in `real-audio` feature enables
  `kira/cpal` (cpal → ALSA native on Linux) for M5 playback; it stays off until then.
  API notes in code: kira's `ClockHandle` is not `Clone` (owns command writers), so
  the crate exposes a cloneable `Arc`-shared wrapper; `set_start_offset_ms` is
  interior-mutable (`&self` per contract). Alternative considered: hand-rolled tick
  clock (rejected: kira ships the real-audio path for M5 and its clock simulation
  models the exact per-buffer tick publication the game will run on).
- **Consequences:** no native dependency this milestone even with the audio crate;
  Cargo.lock gains ~7 pure-Rust crates.
- **Revisit when:** M5 attaches real sounds (cpal/ALSA backend gets its own ADR line).

## ADR-0013 — Behavior-first workflow and explicitly scoped acceptance

- **Context / date:** 2026-11-05, workflow review while M4 phase 1 is in progress.
  Rigid implementation contracts, duplicated bootstrap instructions and an incomplete
  `verify` command obscured simpler designs and the extent of verification.
- **Authorization:** the human approved the updated workflow plan, including selective
  cleanroom-inspired behavior specifications, and requested its implementation.
- **Decision:** repository-local goal, policy and workflow are authoritative; contracts
  specify observables and integration boundaries, with an explicit objection path.
  `verify` defaults to a clearly labelled quick check; milestone and release acceptance
  are explicit and fail closed on missing mandatory gates or prerequisites. Keep existing
  numeric tolerances; strengthen coverage and negative tests. Ordinary regression tests
  may be added without a fixture-change approval ritual.
- **Consequences:** quick success does not certify a milestone. Read-only review has no
  arbitrary shell. Implementation write sets are enforced procedurally unless a real
  filesystem boundary is separately established. No claim of strict legal clean-room
  independence or formal Cleanroom certification is made for existing or future code.
- **Revisit when:** actual OpenCode capabilities or project acceptance requirements change.

## ADR-0014 — Correct Haxe fround semantics and review conductor fixture replacement

- **Context / date:** 2026-11-05. The Rust port and standalone Haxe transcription
  incorrectly treated Haxe `Math.fround` as a binary32 conversion. The installed hxcpp
  `include/hxMath.h` defines it as `floor(x + 0.5)`, returning a floating-point integer;
  the native Haxe behavior differs from JavaScript's similarly named function.
- **Authorization:** the human explicitly approved implementation of the reviewed plan,
  including this identified correction and replacement of the affected expectations.
- **Decision:** verify discriminating cases against real Haxe/runtime utilities before
  correcting the port and model. Generate replacement fixture candidates separately,
  inspect their differences, then replace only the affected conductor expectations.
  Record source/runtime provenance. Do not increase the 1 ms tolerance or relax event
  counts, boundary indices, signatures or event ordering. Fixture verification must
  never regenerate missing expectations automatically.
- **Consequences:** fixture bytes and their hashes can change because the old expectations
  encoded a demonstrated error. Determinism is not proof of parity. The standalone model
  remains a transcription, not a fully independent execution of the game Conductor;
  unvalidated portions must remain explicit in acceptance reports.
- **Revisit when:** a real-game trace disagrees with the model, or the reference target changes.

## ADR-0015 — evdev 0.13 as the M4 input layer; observed default gameplay key set

- **Context / date:** 2026-11-06, M4 contract §3 (`crates/oxide-input`). Requires Linux
  evdev capture with kernel event timestamps, a uinput-backed virtual device for tests
  and the jitter probe, and a gameplay key-code constant.
- **Decision:** `evdev 0.13.2` (Apache-2.0 OR MIT, pure Rust with no C/C++ build step;
  Cargo.lock closure: evdev, nix 0.29, bitvec/funty/radium/tap/wyz, libc, cfg-if,
  bitflags (via nix), cfg_aliases), default features off (no tokio/serde). One
  dependency serves both `open(path)` (real device) and the uinput `VirtualDevice`
  used by `open_virtual()`; the sync, non-blocking polling model matches the
  contract's `poll(now_audio_ms)` shape. Alternative considered: raw `libc` ioctl
  against `linux/input.h` (rejected: hand-rolled ABI maintenance with an existing
  licensed, maintained crate providing the identical observable).
- **CODES:** the M4 contract placeholder `SECONDARY_CODES: [u16; 7]` had no observed
  content. Observed original source (read-only reference checkout): precise gameplay
  inputs come from `funkin.input.PreciseInputManager`, whose keyboard `_keyList` is
  populated only from keys bound to `NOTE_*` actions via `Controls.getKeysForDirection`;
  the default Solo scheme binds NOTE_LEFT=[A, LEFT], NOTE_DOWN=[S, DOWN], NOTE_UP=[W, UP],
  NOTE_RIGHT=[D, RIGHT] (`source/funkin/input/Controls.hx` `getDefaultKeybinds`, Solo
  branch) — 8 distinct keys, not 7. The Rust constant therefore exposes the Linux u16
  evdev codes for those 8 default keys (this is a **derived mapping** of codes; FlxKey
  Int codes remain the *observed* save format and are NOT converted silently here).
  Equivalence check: each exposed code maps 1:1 onto a default `NOTE_*` binding, and
  every `UI_*`-only key is absent.
- **Consequences:** `crates/oxide-input` stays a timestamped capture primitive; binding
  customization (save-driven key sets, M7) is a caller concern. One **engine decision**
  is embedded in `open_virtual`: the virtual device advertises two never-emitted
  joystick button codes (`BTN_SOUTH`/`BTN_EAST`, outside `SECONDARY_CODES`) because a
  keys-only uinput device on this host does not get a udev `uaccess` session ACL for
  its `/dev/input/eventN` node within usable time; the read side retries
  permission-denied for up to 1 second, and a lasting denial still surfaces as the
  distinct `PermissionDenied` variant. Host-specific evidence (uinput console ACL,
  real-device group `input`) remains necessary and is NOT complete yet. Known residual:
  `evdev::InputEvent::timestamp()` may panic on a negative/out-of-range kernel timeval
  before the crate's own checked conversion runs; post-epoch realistic timestamps do
  not trigger it, and the relevant upgrade would be upstream.
- **Revisit when:** the original save-format semantics for keybind Int codes are
  ported (M7), or a real-game precise-input trace disagrees with this mapping.

## ADR-0016 — M4 replay gate: observed note-time semantics + placeholder scoring

- **Context / date:** 2026-11-06, M4 contract §4 `cargo xtask replay --fixture <chart>`.
  Requires a deterministic hits/misses/score report with a two-run sha256 ordering
  proof, synthesized from the chart, with no human replay and no parity claim.
- **Decision:** `xtask/src/replay.rs` implements the gate as a small deterministic
  pipeline: provenance-verified chart load (`oxide-tools/x task`, `sha2` added to
  xtask — same 0.10 already locked via oxide-media; MIT OR Apache-2.0, pure Rust) →
  stream synthesis (engine-decision) → lane-merge evaluation (engine-decision) →
  single canonical report, evaluated **twice**, printed with the sha256 equality
  label. Misses are never zero-stated as a pass condition; hit/miss semantics are
  explicit and unit-tested with a negative case (a 110 ms off-press misses both the
  note and the press), so the pipeline provably discriminates.
- **Observed:** V-Slice chart note `t` is song-position ms: `PlayState.hx` L3118
  subtracts `note.noteData.time` (in ms) against `Conductor.songPosition`. That
  expression is the scoring delta computed inside `goodNoteHit` AFTER the note has
  already been accepted by the window logic (`GRhythmUtil.processWindow`/
  `getNotesMayHit`, accept window constant 160 ms at `Constants.hx` L361) — so
  `Std.int(...)==0` is NOT the observed accept window; it is the observed rating
  delta shape. Wire `l` is the sustain length in ms. `NoteData.getStepTime`
  (`SongData.hx`) is an independent ms→steps conversion, not a step that follows
  L3118. Using note `t` instead of the oracle fixture timeline for note strum
  times is therefore an engine decision (list below), not an observation.
- **Engine decisions (labeled, not observed):**
  1. synthesized perfect player presses on the first 1 ms grid integer ≥ `t`
     (`press_grid_ms`), reproducing the observed `Std.int(k − t)` comparison shape;
  2. `inputLatencyMs` assumed 0 (no hardware input the replay claims);
  3. score placeholder = 350/hit to satisfy the print shape; `epicas` and real
     windows are measured at M5 and not invented;
  4. stream order (time, direction, difficulty, press-before-release) and the
     per-lane earliest-pending-note matching are engine choices exercised only by
     the synthesized stream in this milestone;
  5. chart-file inputs are verified against the frozen
     `tools/oracle/provenance.json` `input_asset_sha256` and the fixture id must
     resolve to exactly one entry (fail-closed on drift or ambiguity).
- **Consequences:** `cargo xtask replay` is deterministic and fail-closed without
  fabricating parity claims; `verify`'s `replay` milestone gate stays pending until
  the catalogue owner wires it with explicit coverage wording (review first). The
  gate output states: input source, score placeholder, model boundary (1 ms grid,
  Std.int comparison, two passes) and "not native-game note parity".
- **Revisit when:** M5 measures the real `PlayState` hit windows/rating tiers
  (then `evaluate_pass` semantics are replaced by measured behavior and the
  placeholder score is retired), or when a real-game note trace contradicts the
  observed `t`-in-ms reading.

## ADR-0017 — native hxcpp Conductor probe + wired M4 executive gates

- **Context / date:** 2026-11-06. Human approval: (1) build a read-only native
  hxcpp probe harness (ProbeSlice-style non-invasive slicing, no writes into the
  original Funkin' checkout) executing the real `funkin.Conductor` on the frozen
  recipe; (2) wire `replay`, `input_jitter`, `conductor_native_reference` into
  the acceptance catalogue with explicit honesty labels.
- **Decision:** as documented in `tools/oracle/BEHAVIOR.md` §probe — the harness
  lives in `tools/oracle/` (`NativeConductorProbe.hx`, `ConductorSlice.hx`
  macro, probe-types stub instrumentation, `compare_native_rows.py`,
  `native_conductor_probe.sh`); outputs are frozen at
  `tests/fixtures/conductor-native/` (same scale/shape as the model fixtures,
  with `_manifest_native.json` digests/provenance). The xtask gate
  `conductor-native` replays native-vs-model field equality AND Rust-vs-native
  (1 ms budget) fail-closed with digest verification; verify.rs moves the three
  M4 obligations from `pending` to executable gates:
  `conductor_native_reference` (no assets required), `replay --corpus`
  (explicit `--assets` fail-closed), `input-jitter` (runs the uinput probe).
- **Provenance:** observed native-vs-model equality across all 62 timelines
  (86131 rows, worst 0.000 ms) on this machine for the FIRST run; row digests
  committed; re-runs report deterministic digests (harness infra recorded);
  the manifest re-runs require the original assets/inputs, and the copied
  source bytes are hashed into the frozen manifest.
- **Recorded divergence (`clamp`):** installed `FloatTools.clamp(min, max)` is
  `Math.max(min, Math.min(max, value))`, so the native playback clamp
  evaluates `max(songPos, min(length, min(combined, 0)))` — the contract §0
  reading (`min(max(songPos, c), length)`) differs when `songPos > length`.
  For this corpus both produce identical rows (combined == 0, songPos <=
  length); no clamp-identity parity claim is made. See BEHAVIOR.md §clamp
  divergence for the discriminating future case.
- **Consequences:** M4 aggregate acceptance now exercises ALL these gates
  explicitly, but the model/native boundaries stay visible: the native probe
  executes the real Conductor over the frozen SIMULATED recipe (no gameplay
  integration, audio engine or hardware), so broader native-game parity keeps
  its own pending obligations. `replay` fails closed without explicit assets;
  `input_jitter` only runs where the uinput probe exists (this Linux host);
  Windows/hardware input evidence remains pending (M5+). Catalogue tests
  updated; `fuzz_parsers` stays pending.
- **Revisit when:** the acceptance obligations for gameplay/notes/audio
  integration or Windows inputs change, or the reference checkout updates
  (regenerating native rows requires the full ADR-0014-style review) and the review round (2026-11-06: 5 findings all addressed — the
  step-to-ms conversion fix in both comparators with per-field max-delta
  reporting (all 0.000 ms over 86131 rows after the fix, and the fix also
  closes the discovered 'acceptance pending' label staleness, the distinct
  copied-source provenance keys, the native input-truth avOff fail-closed path,
  and the corpus 61-chart count check). The visible clamp divergence between
  the installed FloatTools and the contract §0 reading is recorded here and in
  BEHAVIOR.md with the note that the corpus cannot discriminate it.

## ADR-0019 — native note reference probe for the M5 vertical slice

- **Context / date:** approved 2026-11-06 (human approval #2 of the M5
  phase-2a follow-up); evidence rounds 2026-11-06/07 with three review/fix
  rounds. The human approved building and wiring the read-only native hxcpp
  probe for `native_note_reference` under the ADR-0014/0017 non-invasive
  approach, to validate `oxide_game::slice` scoring/judgement/window/hold
  semantics against the real Haxe classes. Probe behavior specification:
  `tools/oracle/NOTE-BEHAVIOR.md` (§6, §6.1 REVISED = the operative hold
  regime, §9 implementation/reconciliation notes items 1-17).
- **Decision (validated, round 2):** retained-REAL-code, stub-labelled probe:
  byte-exact copies of `Scoring.hx`, `GRhythmUtil.hx`, `PlayState.hx`,
  `Conductor.hx` (macros drop FIELDS, no body is modified; original/copied
  sha256 pairs recorded in the fixture `provenance.txt`); last-classpath
  shadow modules for graphics/audio/character surfaces; `dispatchEvent` as
  the sole event-row instrument with the fail-closed
  <=1-stateful-event-per-tick rule. Driver orders each tick as the real
  frame does — mirror of the real `Strumline` hold chain FIRST with the
  PREVIOUS frame's conductor position (`PlayState.update` calls
  `super.update` at L1035), then the conductor update to T, then
  `processInputQueue`, then `processNotes` — with the penalizing drop row
  fields read FROM the real `HoldNoteScriptEvent` (fail-closed against the
  `-125.0 * stale / 1000` formula) and driver-emitted hold rows mirroring
  6.1. Rows came from observed real state, never from a model projection
  (the first freeze's projected values were re-derived after review).
- **Frozen set:** `tests/fixtures/note-native/` — three synthesized streams
  (`note-stream-1`, digests `9737e8a0.../b3c02ffa.../a1cd8b4c...`, 78276
  ticks each, 90/104/126 inputs incl. the two-case scripted boundary), three
  native row files (18880/17224/14216 rows; digests `923ce1cb.../ae138208.../
  3c1acd54...`), `_manifest_native.json` digest/provenance manifest (real
  chart `2d1fd06f...`, metadata `f891cd53...`), probe `provenance.txt`. Gate
  `cargo xtask note-native` re-exports the stream from the real corpus
  (sha256 fail-closed), replays the Rust slice twice, compares field-wise
  (integers/bools/strings exact; f64 <=1e-9 labelled; order/sims/key order
  exact) and fail-closes on provenance drift.
- **Evidence results (frozen-gate run):** PASS; worst strict f64 delta
  4.663e-15 over ~351k compared f64 fields; labeled clamp exclusions easy
  58/29 rows (first sim 31200), normal 66/33 (31200), hard 78/39 (35700),
  last sim 75600 in all; every non-health field strict. Native evidence
  corrected FOUR sim errors/over-generalizations (all now fixed and tested):
  (1) sub-threshold hold drops dispatch no event — zero-penalty rows keep
  `is_combo_break=false`, combo unchanged; (2) the real hold mirror runs at
  the frame start with the previous songPosition — hit-tick first accrual,
  fixed per-entry 0.00012/0.25, stale snapshot at the drop branch =
  `end-(R-1)`, drop row emitted on the flag tick = release+1 (NOT one tick
  later, NOT at a projected `end-R` value); the recipe's `ceil(t+l-160)`
  boundary release is therefore PENALIZED (stale 161, `-20.125`) and the
  zero-penalty boundary is `ceil(t+l-159)` (stale exactly 160.0) —
  ADR-0018's "exact-160 does not penalize" expectation is revised by this
  evidence; (3) the windowed-miss path does NOT bump `total_notes_hit`
  (goodNoteHit-only; the 161 hit-path judgement-miss keeps its bump);
  (4) `hold_drop.input_ms` = 0.0. The recipe releases every sustain at
  exactly `ceil(end)`, so a full-length sustain ends as a zero-penalty DROP
  row, and `hold_end` rows appear only when the key stays held past the
  sustain end (toy/negative coverage at ceil(end)+2). Probe negative battery
  (schema drift, `input_ms` below sim, short/missing/extra ticks,
  multi-event tick) all exit 2.
- **Engine-decision divergence (labeled, not modeled):** the retained probe
  path excludes `PlayState.update()`, so it has no health clamp/death
  ordering; native health accumulates unclamped past HEALTH_MAX and the
  traces diverge on `health_before/health_after` from the first crossing.
  The probe refuses to emulate the clamp. The comparator implements the 6.1
  item-6 labeled policy: strict until the first crossing (shape-checked:
  native > HEALTH_MAX, Rust in [HEALTH_MAX, native+1e-9]); from there only
  those two fields are excluded, counted per difficulty (58/66/78 fields),
  all other fields stay strict. This is an evidence-interpretation rule over
  an engine-decision field, not a data/acceptance tolerance change; no
  tolerance value changed.
- **Not covered (unchanged honesty list):** game-over/clamp ordering
  (`update()` not retained), render geometry (phase-2b), opponent side,
  botplay/practice, audio outputs, mod/script bodies, input device
  timestamps, everything outside bopeebo easy/normal/hard default charts.
  Field-wise equality on the probe boundary is NOT full native-game parity;
  real-game integration traces remain a separate obligation. Additional
  accepted limitation: the gate validates manifest digests/provenance fields
  but does not authenticate the fixture files themselves (the conductor
  gate's embedded-manifest trust pattern remains unused here); a coordinated
  fixture+manifest replacement is out of the threat model and is documented,
  not independently verified.


## ADR-0018 — M5 slice sim terminal game-over + gate trace conventions

> Addendum 2026-11-07 (native-probe evidence, ADR-0019): the "exact-160
> boundary release is MANDATORY (bitwise remaining_ms == 160.0)" convention is
> REVISED: the recipe still releases that gate check at `ceil(t + l - 160)`,
> but under the real frame order its stale hold snapshot is 161 ms and the
> release PENALIZES (-20.125, combo break); the zero-penalty boundary is the
> additional mandatory case released at `ceil(t + l - 159)` (stale snapshot
> bitwise exactly 160.0). Both cases are embedded fail-closed in the
> `gameplay_sim_bopeebo` gate and validated against the frozen native rows in
> `tests/fixtures/note-native/`. Similarly, "hold_end at a full-length
> sustain" applies only when the key stays held past the end; the recipe's
> `ceil(t+l)` releases end as zero-penalty drop rows. Details: `docs/
> M5-CONTRACT.md` §7 corrigendum item 3 and ADR-0019.

- **Context / date:** 2026-11-06, M5 phase 2a implementing the frozen
  `docs/M5-CONTRACT.md`. The contract freezes the game-over transition up to
  the GameOverSubState boundary (§1.4) but does not define the sim's behavior
  AFTER a death; the first implementation silently continued processing
  inputs/notes after `game_over` (review finding).
- **Decision (engine decision, not observed):** death is terminal for the
  phase-1 sim. The step that detects it (clamp → death check at step start)
  emits exactly one `game_over` (death_count++, songScore = 0.0, health =
  1.0 per moveToGameOver), clears active holds WITH no hold bookkeeping
  events, returns without processing inputs/notes; every later `step` returns
  an empty Vec. No retry/reset InputEvent exists in the phase-1 schema; a
  restart flow would be a new phase-2 input/trace boundary, not a silent
  state behavior.
- **Gate trace conventions (engine decisions; contract §2 silent):** scripted
  negative-phase targets are the first lane-collision-free qualifying notes in
  chart order, fail-closed if none; the sustain release uses `ceil(t + l)` on
  the 1 ms grid (M4 presses already `ceil(t)`); the exact-160 boundary drop is
  MANDATORY (release `ceil(t + l − 160)`, bitwise `remaining_ms == 160.0`,
  else the gate fails — the `(0,160]` fallback a reviewer caught is removed);
  the six threshold edges 45/46/90/91/135/136, the 110-late, the 160/161 full
  path with tallies/`combo_max` assertions, the ghost press and the full-hold
  tick rates (0.00012 health / 0.25 score per 1 ms tick) are embedded
  fail-closed gate checks, not recommendations.
- **Evidence provenance:** all gameplay semantics are the frozen contract's
  observed/derived readings of the native source; the decisions above are
  model assumptions. The gate proves deterministic two-run sha256 stability
  over the REAL bopeebo chart data by re-executing the whole pipeline twice,
  but proves NO native-game parity: `native_note_reference` (human
  ADR-0014-style approval), `render_diff` and `perf_budget` remain pending
  and M5 acceptance stays blocked with them.
- **Consequences:** the `oxide_game::slice` API (§3) and the `gameplay_sim_
  bopeebo` gate are compiled; the sim trace jsonl is written only under
  `target/` and no trace golden is committed (digests are printed at gate
  time; a committed trace golden would be its own expectation-freeze decision).
- **Revisit when:** native note reference evidence arrives, a real human/
  device input stream becomes available, or the phase-2 terminal-state
  (Game Over flow / retry) input schema is designed.
