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

## ADR-0020 — M5 phase-2b visual slice: renderer architecture and gate definitions

- **Context / date:** 2026-11-07. Human approval #3: implement stage
  (mainStage), character (bf/dad/gf), strumline rendering/animation and wire
  `render_diff` + `perf_budget`. Behavior frozen in `tools/oracle/
  RENDER-BEHAVIOR.md` (OBSERVED/DERIVED labels; an ADR-0014-style problem).
- **Adopted stack:** Bevy `=0.20.1` (first Bevy integration; license
  MIT OR Apache-2.0, verified from crates.io metadata; release build cost on
  this machine ~minutes on an already-warm registry) enters `crates/oxide-game`
  only (GOAL: "Bevy =0.20 when required"; foundation crates stay Bevy-free per
  repository policy). Feasibility probe (throwaway project outside the
  repository, 2026-11-07): headless Bevy render-to-image — `DefaultPlugins.set(
  WindowPlugin{primary_window: None, exit_condition: DontExit}).disable::<
  WinitPlugin>() + ScheduleRunnerPlugin`, camera with `RenderTarget::image`,
  `Screenshot::image` + `save_to_disk` — renders a valid 256×256 PNG; two-run
  sha256 identical (`46968d05…4f2ffd27` twice) with
  `WGPU_ADAPTER_NAME=llvmpipe`; the NVIDIA driver backend segfaulted in the
  same probe.
- **Engine decisions:**
  1. render_diff/perf_budget PIN the wgpu software adapter
     (`WGPU_ADAPTER_NAME=llvmpipe`) for determinism and machine independence;
     adapter identity is printed by the gates. GPU-driver/platform render
     behavior stays a separate pending obligation (Windows/hardware evidence).
  2. render_diff golden frame digests are OUR renderer's output frozen once
     (fixture set `tests/fixtures/render-slice/`, to be created AFTER review —
     never regenerated during verification). They are an engine fixture, NOT
     native pixel parity; a native-game rendered-output trace remains a
     separate pending M5 obligation. Exact-digest first establishment; any
     fallback comparison method needs a BEFORE-USE ADR.
  3. perf_budget: WARMUP 120 + MEASURED 300 frames on difficulty easy;
     budget = mean ≤ 16.667 ms AND max ≤ 33.333 ms frame wall time (60 Hz
     target) on the pinned adapter; failure of the budget fails the gate
     (loosening requires the ADR/review/human path).
  4. Atlas semantics: f64 matrices in the resolver (Haxe Float is double);
     Bevy sprite composition f32 — fixtures pin our composition; no
     bit-exact-openfl claim is made (printed per gate run).
  5. Phase-2b frames exclude bf death/fakeout anims, judge popups, HUD, story
     UI; camera cutout machinery always desktop-equal 1280×720.
- **Consequences:** three disjoint write sets: `crates/oxide-atlas/`,
  `crates/oxide-game/` (+ root manifest/lockfile owned by the orchestrator),
  `xtask/`. M5 acceptance stays blocked for render_diff/perf_budget until the
  newly wired gates run against established fixtures AND the pending native
  rendered-output obligation is resolved by later evidence (explicit honesty
  line in verify coverage text).
- **Revisit when:** the reference checkout updates (re-freezing native rows /render fixtures separately on the same review path), the acceptance budget for perf_budget is revised, or Windows/hardware render evidence arrives.

## ADR-0017/0014 note (fixture flow applied to render-slice fixtures)

`tests/fixtures/render-slice/` planned content: per-difficulty frozen frame
digest manifest + sample song-position list + structural geometry constants.
Creation path: implementer writes gate; orchestrator runs the first
establishment passage after the reviewer approves the gate code; fixtures are
then frozen by the orchestrator (like ADR-0019) and verified by
`cargo xtask render-diff`; during verify, digest/regeneration paths stay
fail-closed (no auto-repair).

### ADR-0020 addendum (2026-11-07, discriminating experiment)

- **Observed evidence:** the release build of the phase-2b renderer runs at
  ~35–57 ms/frame on the pinned llvmpipe software adapter (60 Hz budget fail)
  and at 3.9–14.4 ms/frame steady state (first churn frames ~28–100 ms) on the
  NVIDIA "GeForce GTX 1050 Ti" hardware adapter of this machine (same scene,
  same code; per-tick sha256 digests differ between adapters, e.g. easy tick
  600 = `eb5f1e0e…` GPU vs `47b582d4…` llvmpipe). The harness's llvmpipe pin
  was verified to still render byte-identical frames across two runs (probe).
- **Engine decisions (superseding the single-adapter wording of ADR-0020):**
  1. `SliceRenderer::build(...)` remains pinned to the software adapter
     (llvmpipe): render_diff fixture digests are adapter-specific and the
     manifest pins the adapter name. No env-driven adapter override inside
     deterministic paths.
  2. A separate explicit-adapter entry point (engine-decision API:
     `SliceRenderer::build_for_adapter(assets_root, difficulty, policy)` with
     `policy ∈ {Software, AnyHardware}`) exists ONLY for `perf_budget`, which
     uses `AnyHardware` and FAILS CLOSED when only a software adapter exists
     in the environment (a software-raster 60 Hz measurement would not be a
     hardware performance claim). Both gates print the actual adapter identity
     chosen, and `AnyHardware` rejects software adapters by the printed
     adapter name check.
  3. perf_budget numbers are PLATFORM-SPECIFIC hardware measurements
     (this machine: NVIDIA GTX 1050 Ti, driver state printed); they are NOT
     llvmpipe times, NOT CI-agnostic, and NOT a real-game/release-binary
     latency claim (separate M5/M6 obligation).
  4. Lint exception policy: `#[allow(reason)]` for harness-specific clippy
     lints must carry an explanatory comment per repository policy.
- **Consequences:** gate coverage text for render_diff keeps "software adapter
  pinned"; perf_budget coverage text says hardware adapter, platform-specific,
  llvmpipe never substitutes. Budget constants stay 16.667/33.333 ms.
- **Additional decision (2026-11-07, observed):** the perf-budget gate measures
  whatever build profile invokes it (verify uses `current_exe()`). A first end-to-end
  run in a debug executable measured mean 34.191 ms / max 39.129 ms on the SAME
  hardware adapter where the release build measures 3.9–14.4 ms/frame steady state —
  debug overhead, not scene cost. Therefore `perf_budget` FAILS CLOSED in a
  debug (debug_assertions) build with a typed `DebugBuild` error and an instruction
  to run a release xtask; render_diff is profile-independent (digests already
  validated identical across profiles on llvmpipe).

### ADR-0020 addendum 2 (2026-11-07, NVIDIA capture flakiness)

- **Observed:** with the per-frame `Screenshot::image` capture, the release
  gate run on the NVIDIA adapter timed out on the readback poll (16 updates)
  in three consecutive runs; one instrumented run (identical bytes, only an
  extra save path) PASSED and wrote the PNG (mean 6.873 ms / max 10.945 ms);
  the next clean run timed out at measured frame 116 (after 115 passing
  frames). llvmpipe readback has been reliable in every run. Conclusion:
  the bevy `Screenshot` capture path is flaky for REPEATED captures on this
  NVIDIA driver; per-frame capture spawns are not a sound readback design for
  the perf-budget gate. (Temp diagnostic code was added and will be removed;
  instrument changes did not alter rendered bytes.)
- **Decision:** the harness readback is redesigned to the official headless
  pattern — persistent render-target texture with `COPY_SRC` usage, a custom
  render-graph node copying the last-base-pass texture to a persistent buffer
  after the camera node, and buffer-map readback of the SAME bytes each frame.
  This is internal to the private harness; the public API
  (`SliceRenderer::{build,step,frame,build_for_adapter}`) is unchanged. The
  perf-budget and render-diff evidence must be re-established after the swap
  (render-diff digests may change byte-wise; the fixtures freeze happens
  AFTER the swap to avoid freezing twice).
- **Revisit when:** Bevy updates change the screenshot/graph APIs or the
  driver state changes; Windows needs its own platform observation.

### ADR-0020 addendum 3 (2026-11-07, aggregate review round 1 resolutions)

- **Behavior corrections adopted (review BLOCK, re-established after):**
  1. Dropped PLAYER holds are hidden, not rendered (observed
     `Strumline.hx:634-640`: alpha 0.0/"completely hide"); the earlier visible
     DroppedTrail was an over-generalization of the §3 yOffset wording.
  2. Stage props sample with LINEAR filtering (`antialiasing = !isPixel`,
     Stage.hx:250-251; mainStage's three props are non-pixel); notes/
     receptors/hold sheet nearest remains OUR renderer choice for non-stage
     sprites (engine decision, inert to geometry).
  3. `singTimeSteps` defaults to 8.0 when character data omits `singTime`
     (observed `CharacterData.hx:451/593-596 DEFAULT_SINGTIME = 8.0`); the
     earlier 4.0 default was an invented value. bf and gf have no `singTime`
     key and use the default.
  4. Opponent holds are pruned when `sim_ms >= end_ms` (opponent receptors
     leave confirm-hold and return to static, Strumline.hx:653-669).
  5. Player receptor confirm returns to static immediately on key release
     (native `confirmHoldTimer = -1` semantics for the player); the 0.15 s
     confirmHoldTimer reset remains the opponent's auto-hit path.
- **Explicit scope decisions (also in gate coverage/honesty text):**
  - Chart song events (`FocusCamera`, `PlayAnimation`, chart-linked camera
    focus) are EXCLUDED from the phase-2b slice. Parity consequence stated:
    native bopeebo alternates the camera focus every 2.4 s (FocusCamera v:0/v:1)
    and plays chart `hey` animations; phase-2b frames do not reproduce these.
    The exclusion is deliberate scope, not a claim that the native scene
    matches; a later slice may implement chart events with its own evidence.
  - Hold covers and note splashes are EXCLUDED from the phase-2b scene
    (later-slice visual deliverable; zIndex 40/50 layers missing by decision).
- **Evidence process note:** the perf-budget gate now measures a 16-step
  scene-advancing discretization per frame (60 Hz), with the step cost inside
  the measured interval; the first measurement wording ("rendered sequentially")
  now also includes stepping. The fixture freeze happens only after the
  re-establishment run that includes these corrections (the earlier freeze
  candidates are superseded and were never held as acceptance evidence).
- **Re-review resolutions (2026-11-07, before the final fixture freeze):**
  1. Hold-trail width corrected to the honest formula `(graphic.width/8)*zoom =
     (416/8)*0.7 = 36.4 px` for funkin (volume notes scale 0.7); the earlier 52
     was the pre-zoom column width (transcription error, both in RENDER-BEHAVIOR
     §3 now corrected and in the renderer). Placement flows from the width
     function (no hardcoded 36.4).
  2. Opponent holds prune at the first grid tick strictly AFTER the end
     (`sim_ms > end_ms`) — the completion branch sees the previous-frame
     snapshot (Strumline.hx:653-669/721-727, ADR-0019 semantics).
  3. While the key stays held AND the confirm animation span has finished, the
     player receptor plays `press` (observed Strumline.hx:774-779
     onAnimationFinished fallback); release still returns to static
     immediately; the opponent 150 ms confirmHoldTimer path is unchanged.
- The final establishment froze the fixture with THESE corrections included
  (`tests/fixtures/render-slice/_manifest.json`, established 2026-11-07 after
  re-review; establishment log /tmp/opencode/render-diff-establishment.log4);
  the gate passes twice (two-run equality) and perf-budget passes on the
  hardware adapter (mean 3.525 ms / max 5.798 ms, 16-step discretized
  frames). The `render_diff` top-doc now also states which geometry values
  are independent-recomputed and which are pinned engine-decision
  constants.

## ADR-0021 — M5 phase-2c interactive bopeebo play mode (human-approved)

- **Context / date:** 2026-11-07. Human approval: wire an interactive window
  mode in the oxide CLI binary (`oxide play --assets DIR [--difficulty
  easy|normal|hard]`) opening a real 1280x720 Bevy window for the bopeebo
  vertical slice with live audio (Inst + Voices-bf + Voices-dad), live
  keyboard input (WASD/arrows), and HUD/camera/character/strumline
  rendering, for direct playtesting (not a milestone-acceptance claim).
- **Decisions (engine decisions unless observed):**
  1. `crates/oxide-audio` gains the REAL playback API on the approved kira
     0.12 stack: `kira` feature `cpal` (device output; the realtime-
     priority/dbus extras were REMOVED by review — see decision 6b) and
     `ogg`/`vorbis` (symphonia decode, no runtime transcoding — rule kept).
     New typed `AudioError` variants; bytes read through the M1/VFS crates
     (no path duplication). Feature enablement is a lockfile/manifest
     change owned by the orchestrator; Linux ALSA/PulseAudio is the observed
     cpal backend; Windows audio remains a pending platform obligation.
  2. Playback semantics: `SongPlayback::start` schedules Inst + the
     metadata's playerVocals/opponentVocals voice tracks on the SAME kira
     clock that `AudioClock` measures (songPosition via clock handle, 1 ms
     grid semantics preserved). Voice character VOLUME muting/restoring is
     surfaced (the phase-2 mute-ticks obligation); every mute/restore tick is
     recorded by the play session for the deferred waveform verification.
  3. Play session (in `crates/oxide-game`, e.g. `play.rs` + `play/` dir):
     consumes conductor-driven song position per frame, drives the EXISTING
     `GameplaySimState::step` on the 1 ms grid (the same frozen §M5-CONTRACT
     semantics; out-of-order events are typed errors), routes Bevy keyboard
     events mapped WASD/ARROWS {A/←:0, S/↓:1, W/↑:2, D/→:3} into
     `InputEvent` on the tick containing them, and drives the same scene/
     renderer stack as the headless gates — with a WINDOWED harness variant
     (winit window, primary 1280x720) instead the readback path.
  4. HUD: health bar background/fill + bf/dad icons on camHUD (statics per
     observed defaults); noteSplash/judgePopups stay EXCLUDED (recorded
     decision); the score text display is NOT part of this slice (vcr-bmp
     text is a later deliverable).
  5. Session controls: ESC toggles pause/resume (audio pause via kira handle
     + conductor pause; simulation stops with the same clock); no pause menu
     (observed PlayState pauses music later; esc/quit are the only controls);
     the game-over boundary (M5 sim terminal) stops audio and freezes the
     scene; restarting is NOT provided in this slice.
  6. No automatic writes: the play session records nothing to disk; only
     the CLI prints real-time diagnostics on exit; toolbar teardown is at
     least documented.
  6b. Feature plumbing: the root `oxide` binary does NOT enable
      `oxide-audio/real-audio` by default (feature unification would break the
      locked workspace quick verify, which builds all members: cpal needs
      system ALSA headers that are missing on this box without an approved
      package install). The root bin exposes a `audio` feature
      (`oxide-audio/real-audio`); `oxide play` builds/runs with
      `cargo run --release -p oxide --features audio -- play …` and fails
      closed with a typed AudioUnavailable error otherwise. cpal/ALSA
      (`alsa-lib-devel` on Fedora) is a native system dependency pending
      explicit human approval per policy; without it the runtime backend
      cannot initialize; the optional `cpal-realtime-dbus` variant was
      REMOVED by review (it would pull the dbus native library, a second
      system dependency beyond the approved cpal/ALSA scope).
  7. Honesty: the play mode is a playtest, NOT timing/latency evidence and
     NOT native parity; input source is the live keyboard (jitter labels
     remain M4); ET audio latency depends on the cpal driver envelope.
- **Spawn plan:** one worker per file set: oxide-audio playback; then
  oxide-game play module once the playback API compiles; CLI wiring by the
  orchestrator. At most 3 workers, disjoint sets.

### ADR-0020 addendum 4 (2026-11-07, human playtest findings — renderer corrections)

- **Provenance:** OBSERVED human playtest comparison against the original
  game (user report 2026-11-07) + orchestrator verification of each citation
  in the reference checkout. The earlier phase-2b engine decisions that are
  now corrected are labeled; the fixture freeze is (re-)established AFTER
  these fixes via the reviewed path (manifest guarded by ADR-0020's rules;
  human explicitly re-approved the re-freeze).
- **Corrections (observed):**
  1. Strumline background underlay default opacity is 0 —
     `Preferences.strumlineBackgroundOpacity ?? 0` (Preferences.hx:464) and
     `background.alpha = opacity/100` (Strumline.hx:249/300): alpha 0.0 at
     first launch (no save file). The phase-2b renderer's 1.0 was an engine
     invention; geometry now emits alpha 0.0 (background quads omitted).
  2. BF flip follows `flipX = !character.getDataFlipX()` (Stage.hx:425;
     BaseCharacter.hx:221-226): bf.json flipX=true ⇒ final flipX=false (no
     mirror); dad/gf get `flipX = getDataFlipX()` (absent ⇒ false). The
     phase-2b `role == Player ⇒ mirrored` hardcode was wrong (BF faced the
     wrong way).
  3. Rotated spritemap tiles use flixel's ANGLE_NEG_90 chain:
     `rotateByNegative90()` = `setTo(b,-a,d,-c,ty,-tx)` from identity →
     `(0,-1,1,0)` then `translate(0, frame.width)` (FlxFrame.hx:196-199,
     FlxMatrix.hx:60-65) — the atlas module's `[0,1,-1,0,w,0]` was the
     ANGLE_90 chain and rendered dad's head upside down (human-observed).
     NOTE: the dad real-asset bounds tests that "confirmed" the signs pass
     under both quarter-turns (the union footprint is similar); the visual
     playtest is the discriminating evidence.
  4. Sprite frame quads double-scaled: the scene passed pre-scaled
     width/height into a matrix that applies scale again (0.49 vs 0.70).
     Fix: quad locals use the unscaled frame rect; the matrix carries
     scale/translate once (receptors + note sprites).
  5. Receptor press/confirm frames are centered on the static arrow
     (`fixOffsets()`/`centerOffsets()`, StrumlineNote.hx:183-198; the
     confirm glow gets `offset -= 13`, DEFAULT_OFFSET:178) so the glow does
     not shift the receptor origin.
  6. HUD positioning follows PlayState.hx:1993-2030 +
     HealthIcon.hx (`POSITION_OFFSET 26`, `HEALTH_ICON_SIZE 150`, bar y =
     height*0.9 = 648, bg screen-centered x = 339.5, icon split along
     `healthBar.value` remap (health 0..2 → 100..0%), icon y = bar.y −
     height/2). The bf icon sheet is 2×1 150×150: draw ONE frame (engine
     decision: the steady frame only; the native loss/win frame switching
     is an unsliced observable for now, recorded as scope line).
  7. F6 debug overlay (playtest tool, engine decision): FPS averaged over
     the last 60 frames, frame delta ms, sim tick duration, song position
     vs total, health/score; Bevy default-font text on the top layer;
     toggling does not touch the sim.
- **Re-establishment chain for addendum 4 (2026-11-07):** the first
  post-correction freeze (log5) was superseded the same day when the
  review round BLOCKED the receptor centering (the implementation used a
  trimmed-rect approximation with an inverted −13 glow direction). The
  operative native content-placement formula (RENDER-BEHAVIOR §3, frozen):
  `content = spritePos + s·(frameX,frameY) − sourceSize·(1−s)²/2 + (confirm? 13 : 0)`
  (raw sparrow frameX/Y; flixel matrix-chain derivation: origin
  centerOrigin on the scaled hitbox, offset = fixOffsets = source·(1−s)/2,
  confirm offset −13 moves the content RIGHT/DOWN because the draw
  subtracts the offset; FlxSprite.drawFrameComplex + FlxFrame.copyFrom for
  tile semantics). Notes use the same formula without the +13
  (NoteStyle.buildNoteSprite offset (0,0) then updateHitbox). The renderer's
  `sprite_frame_quad` no longer folds `dest_offset` (callers pass the native
  content top-left; scale applies exactly once). The dependency-visible
  effects: camera_initial_scroll recomputed (dad content height shifted
  under ANGLE_NEG_90), fixture re-frozen from
  /tmp/opencode/render-diff-establishment.log6; render-diff PASS twice;
  perf-budget PASS (mean 2.494 / max 4.499 ms). The camera scroll / HUD
  pixel visuals still need the human re-playtest (headless-only coverage).

- **Reachability falsification of the +13 confirm glow (2026-11-08 local):** the
  requested "minus 13 px for confirm" lives ONLY inside
  `StrumlineNote.fixOffsets()` (StrumlineNote.hx:178-198), which has NO call
  site in this reference revision (grep: definition only; the
  `playAnimation`/`setup`/`update` paths run `centerOffsets(); centerOrigin()`).
  The frozen content formula is therefore `spritePos + s·dest_offset` for every
  receptor state and notes (RENDER-BEHAVIOR §3 updated). Superpositions: log5
  (wrong centering admin), log6 (the (1−s)²/2 + +13 formula), log7 (the +13
  term) — all superseded; the operative establishment is log8 (fixture
  re-frozen; render-diff PASS twice; perf-budget PASS mean 2.501/max 4.622 ms
  on NVIDIA AnyHardware). The provider of the finding was the reviewer;
  verified by the orchestrator's own grep + reading StrumlineNote.hx:60-130
  (update()/playAnimation/setup) and 183-198 (dead definition).

## 2026-11-08 — 6C scene consumption contract corrections (M6-CONTRACT §12.7)

- The 6C sim's `camera_focus.world_focus` row was a transcription error against
  FocusCameraSongEvent.hx L113-134: the native handler ADDS the literal
  `x`/`y` fields on top of the character focus point
  (`targetX += bfPoint.x`); the frozen row dropped them whenever a character
  focus point resolved. Corpus evidence: 317 FocusCamera events carry nonzero
  literal x/y (blammed-chart-pico, 2hot, …); the shipped Fixtures only assert
  per-kind row COUNTS, so no digest path depends on the row values. The row
  definition in `crates/oxide-game/src/slice/events.rs` is corrected to the
  handler algebra before any scene-side digest freezes; scene-side focus
  application consults the scene's own character rigs (§12.6 finding 1).
- With the two human-approved ADR-0020 fixture establishments (event-driven
  bopeebo re-freeze + new school/senpai pixel family), the §12.2
  DEFAULT-ZOOM-OFFSET contradiction resolution becomes active:
  `Constants.DEFAULT_ZOOM_OFFSET = 0` (Constants.hx L251, "Zero beats
  (on-beat)") replaces the frozen `1` in the render scene's bop-gate default;
  `geometry.rs CAMERA_ZOOM_RATE_OFFSET` moves 1 → 0, and both fixture
  families freeze digests that embed the observed on-disk default.
- PlayAnimation scene handoff implements the observed
  `Bopper.playAnimation` failure surface: `ignoreOther = force` re-locks
  `canPlayOtherAnims` until the fired animation finishes
  (Bopper.hx:250/291/110-117), so forced chart animations suppress
  sing/dance until they end; restart = force.

## 2026-11-08 — M8 scripting engine decision: Rhai exclusively on base branch; mruby reserved for a separate flavor/fork (human product decision)

- **Decision (human-approved 2026-11-08):** Milestone M8 (`oxide-script`) on the
  base branch will exclusively use **Rhai** (pure Rust softcoding/scripting engine,
  zero C toolchain/FFI overhead, compact binary footprint, and native Rust/Bevy
  integration).
- **Status of mruby:** Ruby/`mruby` is NOT discarded conceptually; it is reserved
  for a separate fork/flavor of Oxide aimed at modders who prefer Ruby's simple,
  comfortable syntax. Because of `mruby`'s larger C build footprint and potential
  runtime/FFI overhead, `mruby` must NOT be implemented in the base branch.
- **Timing:** Per `AGENTS.md`, the `rhai` crate dependency will be added when M8
  implementation begins (no unused planned dependencies in `Cargo.toml` during M6/M7).

## 2026-11-09 — playtest-verified animation parity fixes (M6-CONTRACT §12.8)

- Two human-playtest findings fixed directly (no subagents): (1) the native
  `processNotes` opponent-hold `holdTimer = 0` reset was missing from the
  scene step, so the dad stepped off its sing animation in the middle of
  long opponent sustains; (2) `ignoreExclusionPref = ['sing']` lets
  `sing{DIR}`/`sing{DIR}miss` bypass the `canPlayOtherAnims` lock of a
  forced `PlayAnimation`, which the Rust lock blocked.
- Fixture consequence: render-diff digests re-frozen a second time under the
  same human-approved ADR-0020 establishment; sample lists unchanged (the sample plan does not derive from animation state); school normal/hard
  still share one run digest with distinct trace digests. No tolerance or
  coverage semantics changed.

## 2026-11-09 — 6E behavior freeze (M6-CONTRACT §14): shipped .hxc scripts are content

- The week-6 dialogue + weekend-1 choreography surface is driven by shipped
  content scripts (`preload/scripts/{songs,characters,stages,dialogue}/*.hxc`)
  that the native loader attaches at runtime. Per the Rhai-only base-branch
  policy, 6E re-implements their OBSERVED per-song behavior as Rust content
  modules (the M8 Rhai surface will expose the same engine hooks; nothing in
  the engine interprets Haxe scripts at runtime).
- Dialogue engine surface: conversations/boxes/speakers JSON observed
  inventory (9 conversations, 3 boxes, 9 speakers) with the Conversation
  state machine, FlxTypeText cadence (0.05 s/char default, `Std.int(_timer
  / delay)` batch advance), CUTSCENE_ADVANCE [Z, ENTER], restart-dialogue
  pause path, and the pixel.otf (`Pixel Arial 11 Bold`) text raster as an
  explicit dependency approval item before implementation.
- Weekend-1: the first-entry video branches (darnellCutscene/2hotCutscene)
  are DEFERRED to the media/video phase; 6E implements the story retry
  in-game paths (darnell can-cutscene timer chain with the 6C camera
  algebra, the 2hot note-kind can/gun choreography with the -0.5 health
  can miss and the picoFade/can/bf-dad-gf exclusion lists).
- New engine capability named: per-prop static color multiply (the
  darkenStageProps/blackenStageProps passes) — extends the 6B scene
  without touching the frozen camera algebra.

## 2026-11-09 — Phase 6G added to M6: native gameplay shaders & visual effects in Bevy/WGSL (human product decision)

- **Decision (human-approved 2026-11-09):** Milestone M6 gains **Phase 6G**
  (`docs/M6-CONTRACT.md` §3, `docs/GOAL.md`) to reimplement the native V-Slice
  gameplay shaders and post-processing visual effects in Bevy/WGSL.
- **Scope:**
  - Per-sprite / per-character shaders: `AdjustColorShader` (hue, saturation,
    brightness, contrast for erect stages/characters), `DropShadowShader` (rim
    lighting / drop shadow with mask threshold on erect stages), `HSVShader`,
    `MultiplyShader`, `StrokeShader`, and `LeftMaskShader` / `SserafimShader`
    (`EnableMask` event consumption).
  - Stage / post-processing shaders: `RuntimeRainShader` and `PuddleShader`
    (Philly Streets / Weekend-1 dynamic rain intensity and puddle reflections),
    `GaussianBlurShader`, `WaveShader` / `WiggleEffectRuntime` (senpai/thorns
    background distortion), and `MosaicEffect`.
- **Execution order:** 6G requires its own behavior-freeze round from the native
  `funkin/graphics/shaders/*.hx` and `assets/shaders/*.frag` sources before
  implementation, and may proceed after 6E (or alongside 6F).

## 2026-11-09 — 6E: ab_glyph dependency + dialogue fixture establishment (human-approved)

- Dependency: `ab_glyph = "0.2.32"` in `oxide-game` (Apache-2.0/MIT, pure
  Rust, no C/native dependencies, already vendored in the local registry).
  Rationale: the shipped `assets/fonts/pixel.otf` ("Pixel Arial 11", Bold)
  is a TTF/OTF raster font; the conversation text (size 32, fill #3F2021,
  shadow #D89494 at +2/+2, letterSpacing −1, greedy word wrap) is dynamic
  per line, so a runtime glyph rasterizer is required. The engine bakes
  deterministic glyph pages per line at load from the approved dependency;
  the alternative (an offline bitmap atlas) cannot cover arbitrary dialogue
  text. The raster is pinned by a two-pass byte-identity test.
- The dialogue fixture family is the human-approved third ADR-0020
  establishment: `render-slice-2` gains the `dialogue` family (senpai/
  normal, the synthesized CUTSCENE_ADVANCE playthrough, trigger-relative
  samples). Digests are llvmpipe engine output, not native pixel parity.
- Dialogue data taxonomy: the dedicated `DialogueKind` surface (rather than
  extending the global `DataKind`/`Typed`) avoids shifting the parse-all
  corpus inventory counts mid-phase; unification is a recorded follow-up
  with the equivalence check `implied_dialogue_kind(path) ==
  implied_kind(path).map(DialogueKind::from)`.

## 2026-11-09 — 6E review resolutions (M6-CONTRACT §14.5)

- Dialogue Opening advances are no-ops (native box.skip() is inert before
  typing), the dialogue text uses letterSpacing 0 + linear glyph sampling
  (the score text's −1/OUTLINE is a different surface), the roses/thorns
  scripted box animations (`sentenceEnd`/`click`/`speaking`/reversed `exit`
  via frameIndices) are wired, CUTSCENE_ADVANCE coalesces to one per tick
  after the event rows, and the weekend-1 z-300 `Can Shot` prop gained its
  missing draw path.
- The 6E trigger rows are gated by the new story-entry toggle:
  `build_chart_events_with_story(..., false)` keeps the frozen bopeebo/school
  render families byte-identical (verified trace-digest identity at the
  establishment); the dialogue fixture replays with the story rows.
- The dialogue fixture family is established (senpai/normal, chart taps +
  the fixed advance cadence, 16 trigger-relative samples) and re-established
  after the review fixes; digests are engine output on llvmpipe, not native
  pixel parity. Pause 'Restart Dialogue' remains the M7 pause-surface item;
  `AnimationData` unknown-key retention is a stated pre-existing M1 limit.

## 2026-11-11 — 6F full_chart_gameplay gate (M6-CONTRACT §6F.1)

- The gate is a separate fail-closed xtask subcommand rather than an
  extension of `gameplay-sim --corpus` (which stays a diagnostic): the gate
  aggregates ALL failures, exits nonzero on any run error, and includes the
  per-run scene inventory load; no fixture writes and no skips. The
  catalogue entry runs it at milestone 6 with the portable baseline as
  dependencies and the deferred parity obligations named in the coverage
  text (native rendered-output traces, Windows/hardware, CI artifacts/
  dual-platform receipts, native camera/input probes, M4 replay/input-jitter).
  The gate also fails closed on non-distinct corpus inventories (duplicate
  chart-id lines or (id, difficulty) plans) so count-preserving drift cannot
  produce a false green.

## 2026-11-11 — 6G: WGSL gameplay shader pipeline + 6th ADR-0020 fixture establishment (human-approved)

- **Behavior preserved:** the six shipped gameplay shader families frozen in
  `docs/M6-CONTRACT.md` §15 (`AdjustColorShader`, `DropShadowShader` +
  `useAltMask`/`EnableMask`, note `HSVShader`, `WiggleEffectRuntime`,
  `RuntimeRainShader` in screen and sprite modes, and `SserafimShader` +
  `sserafimDark`/`sserafimLights`/`sserafimPulseLights`/`sserafimCover`).
  Dead native shaders (`MultiplyShader`, `WaveShader`, `PuddleShader`, rain
  puddle/light blocks) are omitted; M7 UI shaders (`GaussianBlur`, `Mosaic`,
  `LeftMask`, `Stroke`, `AngleMask`, `ColorSwap`) stay deferred to M7.
- **Simpler option / selected approach:** instead of a runtime shader-registry
  or multiple `Material2d` plugins, plain layers stay on Bevy's `ColorMaterial`
  (preserving byte-identical `bopeebo`, `school`, and `dialogue` digests) and
  shader-active layers bind a single private `LayerShaderMaterial` backed by
  one embedded WGSL fragment (`layer_shader.wgsl`, registered synchronously via
  a fixed asset UUID). Screen-mode rain renders stage/character layers (`0..rain_idx`)
  on `RenderLayers::layer(1)` through an offscreen `Camera { order: -1 }`
  `game_target` image, then composites the rain post quad plus unfiltered
  strumline/HUD/dialogue/cutscene layers on `RenderLayers::layer(0)`. Non-rain
  stages spawn no extra camera or target.
- **Tradeoffs & model assumptions (`[D]`):** (a) `rain_duration_ms` uses the
  deterministic chart last-note end `max(note.time + note.sustain_ms).max(1.0)`
  because the scene renderer does not decode audio streams; (b) on game over,
  screen-rain sets `u_intensity = 0.0` (an identity pass over `game_target` so
  the offscreen stage stays visible without mutating `RenderLayers`); (c)
  `fixture_note_desaturation` is disabled only for the legacy `bopeebo` fixture
  entry point (whose scripted negative phase injects `Bad`/`Shit` hits) and
  enabled for `PlaySession` and all non-bopeebo songs; (d) animate-atlas
  `DropShadow` binds per page layer (`uFrameBounds = [0, 0, 1, 1]`) rather
  than an intermediate per-character render texture.
- **ADR-0020 6th establishment (human-approved 2026-11-11):** extended
  `tests/fixtures/render-slice/_manifest.json` (`render-slice-2`, `llvmpipe`)
  from 3 to 7 families by adding `philly_streets_erect` (`darnell-erect`
  `erect`), `school_evil` (`thorns` `normal`), `tankman_erect` (`stress-pico`
  `hard`, with extra samples `[61900, 61982, 62232]` pinning `EnableMask`),
  and `sserafim` (`spaghetti` `hard`, with extra samples `[37800, 39000, 47500]`
  pinning `sserafimDark`/`PulseLights`/`Lights`; `39000` refreshed after the
  8-digit `0xAARRGGBB` `parse_sserafim_color` review fix). Two-run SHA-256
  equality asserted at capture; `bopeebo`, `school`, and `dialogue` digests
  remain byte-identical. Engine fixtures on `llvmpipe`, NOT native pixel parity.

## 2026-11-12 — Phase 6H added to M6: PlayState visual polish & `--song` CLI (human product decision)

- **Decision (human-approved 2026-11-12):** Before entering Milestone 7 (Meta/UI
  menus), complete the remaining PlayState visual elements deferred from §13.2
  as **Phase 6H** (`docs/M6-CONTRACT.md` §3): note splashes (`Sick!`), hold covers
  (player + opponent sustains), judgement popups (`Sick`/`Good`/`Bad`/`Shit`) +
  combo digits (`num0..9`), visual + audio countdown (`3/2/1/GO`), and a
  song-parameterized `oxide play --song <id> [--difficulty ...] [--story]` CLI
  so any corpus song/variation can be playtested interactively with audio,
  dialogue, and WGSL shaders.
- **Execution order:** Requires a read-only native behavior freeze (`NoteSplash`,
  `NoteHoldCover`, `PopUpStuff`, `Countdown`, `Strumline`/`PlayState`) before
  Rust implementation. Existing `render-slice-2` fixture preservation vs
  ADR-0020 fixture updates will be resolved explicitly during the 6H freeze.

## 2026-11-12 — Block-A native-parity fixes + 7th ADR-0020 re-establishment (human-approved)

- **Five verified systemic fixes:** (1) `Sheet::Sprite` characters now
  compose the camera projection (previously stuck at raw world coordinates —
  senpai/gf-pixel/bf-pixel/spirit/tankman-bloody); (2) native character
  anchor: `stagePos − origin(idleDims@finalScale) + globalOffsets`,
  omitted per-animation `offsets` default `[0,0]` (native `Bopper.hx:325`
  assigns null → `[0,0]`), idle-dimension basis replaces
  first-document-frame/main-sheet bounds (measured misalignments otis-speaker
  `[-13,-132]`, nene `[4,-89]`, darnell `[18,31]`, pico-playable `[19,1]`,
  spirit `[31,40]`); (3) HealthIcon flixel `updateHitbox`+`centerOrigin`
  scaling (pixel icons were pushed 59-73 px below the bar, normal icons
  ~15 px into the score text on bops); (4) oxide-atlas `M3D` 4×4 → 2D affine
  fallback (native `FlxAnimateJson.hx:713-717`; MX priority preserved so the
  frozen bf/dad/gf parses stay byte-identical; the brief's "M3D translation
  == TRP x/y" premise was FALSE — corpus 690/80,770 matches — and native
  uses M3D translation only) + `LT:"Clp"` mask layers never draw, masked
  quads carry a mask-AABB clip rect consumed by a new `LayerShader::Clip`
  discard pass (named AABB approximation of native per-pixel masks); (5)
  DropShadow on atlas characters composes the character into a per-group
  transparent 1280×720 offscreen target before the rim shader samples it
  (native `useRenderTexture = true`; named approximations: screen-pixel-scale
  `dist`, altMask mapped over the limb AABB). SserafimShader audit: zero
  findings.
- **Approach:** wave 1 = two parallel workers (oxide-game render/{scene,
  load,hud}; oxide-atlas), wave 2 = one worker (clip consumption + composed
  pass + Sserafim audit) that hit its step limit mid-refactor; the
  orchestrator finished the specified `ClipPlan` build-loop integration
  (clip slots emitted before the page layer when the scanned frames only
  clip lower quads; `after` requires positive evidence) and validation.
- **Re-establishment:** 211 frame digests across all seven
  `render-slice-2` families re-established after visual PNG inspection of
  all six gameplay families (alignment, icon centering, no `#0000FF` mask
  rect over sserafim-gf, tankman `#DFEF3C` rim, coherent M3D characters,
  pulse-light tint). ALL trace digests unchanged (sim untouched); only the
  sserafim geometry section changed (kazuha M3D/mask-aware bounds).
- **Deferred:** `ABotSpeaker` = own follow-up slice after Block A (new
  component needing its own freeze); M8 HScript behaviors (cars, tankmen
  runners, extra sserafim members, dust sprites) out of scope; scene
  camera-focus parity vs native `resetCameraFocusPoint` = named follow-up
  (frozen §12.7 sim algebra kept).

## 2026-11-12 — Block B (`ABotSpeaker`) + 8th ADR-0020 partial re-establishment (human-approved)

- **Scope & design (`docs/M6-CONTRACT.md` §18):** Added behavior-frozen
  `ABotSpeaker` support in `oxide-atlas` (`animate_stage.rs` verbose Adobe
  Animate JSON schema with `Matrix3D` fallback) and `oxide-game`
  (`render/abot.rs`, wired into `render/load.rs` and `render/scene.rs`) for the
  five shared Animate-Atlas speaker characters (`nene`, `nene-christmas`,
  `nene-tankmen`, `nene-dark`, `otis-speaker`; `nene-pixel` remains excluded
  for its separate pixel-art speaker setup).
- **Observables & parity rules:** Sublayers sort behind the speaker character
  at `[char_z - 20, char_z - 19, char_z - 15, char_z - 11, char_z - 10]`
  (`eyeWhites`, `stereoBG`, `pupil`, `abotViz`, `abot`), keeping `otis-speaker`
  and A-Bot behind `tankBricks` (`zIndex: 101`) while matching Block-A speaker
  anchoring (`anchor + dataOffsets * scale + root_offset`), beat-restarted
  body animation (`play(1..=15)`), focus/note-kind pupil playback (`[0..=16]`
  left / `[17..=30]` right), silent headless `abotViz` hiding at `0.0` levels
  (`ABotVis.hx:152`), and variant shaders/tints (`phillyStreetsErect` /
  `nene-dark` / `otis-speaker`).
- **ADR-0020 8th re-establishment:** Visually inspected updated PNGs in
  `target/render-diff/philly_streets_erect/erect/` and
  `target/render-diff/tankman_erect/hard/` (Nene & Otis seating on A-Bot,
  `eyeWhites` + left/right `pupil` alignment inside visor cutout, stage shader
  application, `tankBricks` z-order in front of Otis + A-Bot). Refreshed only
  the two affected families in `tests/fixtures/render-slice/_manifest.json`:
  `philly_streets_erect/erect` (14 frames + `run_sha256`) and
  `tankman_erect/hard` (19 frames + `run_sha256`). All `trace_sha256` digests,
  `geometry` blocks, and the remaining five families (`bopeebo`, `school`,
  `dialogue`, `school_evil`, `sserafim`) remain byte-identical.

## 2026-11-12 — AnimateAtlas whole-timeline bounds + character feet/camera_focus parity (human-approved)

- **Problem & root cause:** Animate-atlas characters (`bf`, `dad`, `gf`,
  `darnell`, `nene`, `otis-speaker`, `pico`, `tankman-bloody`, `sserafim/*`)
  used the first idle frame's quad AABB (`startup_frame_size` via
  `atlas_frame_bounds`) as `idle_size` instead of the active timeline's whole
  bounds (`FlxAnimateController.updateTimelineBounds` reading
  `timeline._bounds`), shifting their vertical origin (`anchor()`) and sinking
  their feet below stage ground. In addition, `oxide-atlas` `compute_bounds`
  evaluated unclipped leaf quads rather than `flixel-animate`'s
  `Timeline.getWholeBounds` (`Frame.getBounds` + `Timeline.maskBounds` on
  `Clpb`-clipped layers and `Timeline.applyMatrixToRect` on nested symbols),
  `CharacterRig::draw` / `StageProp::draw` used main `sheet.bounds` instead of
  the played symbol's bounds on `animType: "symbol"` animations, and
  `CharacterRig::camera_focus` used `main_sheet.bounds[3]` and omitted
  `data_offsets` compared to `BaseCharacter.resetCameraFocusPoint`.
- **Approved cleanroom exception & changes:** Per explicit user approval,
  ported `flixel-animate`'s `Timeline.getWholeBounds`, `Timeline.getBounds`,
  `Layer.getBounds`, `Frame.getBounds` (`mask_bounds`), `SymbolInstance.getBounds`,
  `AtlasInstance.getBounds`, and `Timeline.applyMatrixToRect` directly into
  `crates/oxide-atlas/src/animate_stage.rs` and added
  `AnimateStage::symbol_bounds`. Note: local untransformed frame bounds are
  memoized before `apply_matrix_to_rect`, matching native `flixel-animate`'s
  bottom-up `Timeline._loadJson` caching order. Updated `crates/oxide-game`
  (`AtlasSheet::symbol_bounds` / `timeline_bounds`, `startup_frame_size`,
  `CharacterRig::from_loaded` `camera_focus`, `CharacterRig::draw`, and
  `StageProp::draw`), while preserving `Sheet::Sprite` first-frame `dest_size`
  semantics.
- **Fixture handling:** Per explicit user direction,
  `tests/fixtures/render-slice/_manifest.json` was left untouched; fresh
  screenshots were regenerated under `target/render-diff/` for direct
  visual comparison against native Funkin prior to any future ADR-0020
  digest re-establishment.
