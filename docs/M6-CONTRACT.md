# M6 contract — full-chart gameplay (phase 0 approved 2026-11-08: scope, corpus observations, phased plan)

Human kickoff 2026-11-08 (M5 batch committed as `32295ac`). Phase-0 decisions
APPROVED 2026-11-08: (a) sserafim content stays OUT of acceptance (parses
without error, unknown events preserved, no acceptance rule exercised);
(b) the week-6 in-game dialogue gate is IN M6 (6E), meta/UI/save overlap stays
M7; (c) native parity evidence follows the phase-by-phase scoped-probe pattern
(each behavior-freeze phase gets one approved observation round before its
implementation lands). This contract freezes the M6 *plan and method*, not yet
per-phase gameplay semantics: each implementation
phase gets a behavior-freeze amendment (observed → derived → engine decisions)
before its code lands, following `WORKFLOW.md` §2. Nothing here claims full-game
parity. `tools/oracle/corpus.txt` (61 chart-ids) remains the measured acceptance
inventory authority.

Evidence labels used below: **observed** (files/runtime with citations), **derived**
(invariant consequences), **engine decision** (Oxide choices, not observed facts).

## 0. Observed corpus facts (checkout `/home/alex/Descargas/fnf/Funkin/assets`, read 2026-11-08)

- 61 chart files under `preload/data/songs/*/` matching `corpus.txt` chart-ids
  exactly (27 base songs: tutorial + 26; variations erect/pico; bf/lit-up
  one-off ids `darnell-bf`, `lit-up-bf` for the player-perspective Pico/BF
  swaps). 61 metadata files incl. per-variation metadata; 61 distinct `songName`
  values because variations carry distinct display names.
- **Stages: 21 distinct values in `metadata.playData.stage`** and 21 files in
  `preload/data/stages/*.json` (1:1 with the goal's editorial count). Includes
  erect variants as stage values (`mainStageErect`, `phillyTrainErect`,
  `mallXmasErect`, `schoolErect`, `tankmanBattlefieldErect`, `phillyStreetsErect`,
  `limoRideErect`, `spookyMansionErect`, `schoolEvilErect`) beyond the M5 slice's
  `mainStage`.
- **Characters: 43 distinct values** across the three `playData.characters`
  roles in the 61 metadata files (bf/dad/gf family incl. car/christmas/dark/pixel/tankmen
  holders, nene variants, sserafim holders, otis/parents/pico-playable/speaker,
  senpai/senpai-angry/spirit/spooky/monster/tankman). 46 character JSONs on disk
  (`preload/data/characters/*.json`; unlisted extras are deliberate/untracked).
  **Observed renderType distribution** over character files: `animateatlas` 24,
  `multianimateatlas` 10, `multisparrow` 3, `sparrow` 1, `packer` 1,
  `null` 7 (renderType field absent — sparse-data characters; default TBD in the
  character-parsing phase, NOT assumed here).
- **Chart song events (61 charts, all difficulties in-file):** total counts in
  the `events` arrays
  `FocusCamera 1534, ZoomCamera 1509, SetCameraBop 407, PlayAnimation 69,
  ScrollSpeed 16, SetHealthIcon 16` plus the newer-build extras
  `sserafimKick/Show/Sing/Dark/GuitarVibration/PulseLights/Lights/Cover/Flash/
  Beautiful/End` (2..24 each) and a single `EnableMask {}`.
  Values are polymorphic: `FocusCamera` `{char: 0|1|2, x?, y?, duration?, ease?}`
  (117 distinct values; `char` may be number OR string `"1"` per the game's own
  coercion), `ZoomCamera` `{zoom, duration?, ease?, easeDir?, mode?}`
  (237 distinct), `SetCameraBop` `{intensity, rate}` (74 distinct), `PlayAnimation`
  `{anim, target: "boyfriend|girlfriend|bf|...", force?}`, `ScrollSpeed`
  `{scroll, absolute, duration, ease, strumline}`, `SetHealthIcon`
  `{char, id, flipX?}`.
- **Note kinds: every keyed note in all 61 files has `kind` = null** (91,567
  notes). Observed consequence for M6: no chart in the acceptance corpus
  exercises kind-based alt animations / note-skin overrides. The `k`/`kind`
  field stays parsed and preserved (M1 contract), but no positive-path gameplay
  observable depends on it in this checkout; the schema keeps the hook and the
  honesty text records the corpus limitation (rather than inventing a kind list).
- **Note styles: `funkin` 53 / `pixel` 8** metadata files; pixel style drives the
  week-3 (school / philosophies/schoolEvil) + weekend-1/… pixel-side presentation.
- **sserafim content** (`sserafim` stage, character JSONs, 11 custom event
  names, `sserafimShow/Sing` array-valued payloads): CORRECTION (2026-11-08
  review round): the sserafim CHART-IDS are NOT in `corpus.txt`, but the
  `sserafim` STAGE and the three `sserafim-*` characters ARE referenced by the
  corpus chart-id `spaghetti-default` (observed: spaghetti-metadata.json
  playData.stage = sserafim; characters player sserafim-sakura, gf
  sserafim-gf, opponent sserafim-kazuha). They are therefore part of the
  measured inventory's LOAD surface; the sserafim custom EVENTS remain
  outside the acceptance behavior (no chart in corpus.txt carries them). `oxide-data` already records unknown event names (`events.rs:
  `unknown_event_names``) and preserves the raw payload `Value`.

## 1. Derived requirements (from the cumulative M1-M5 contracts)

- Data layer (already landed, reuse as-is): chart note fields `{t,d,l?,k?}`,
  raw song events, metadata (playData, time_changes, variations, album),
  stage props/slots, character data + renderType, note-style schema, VFS
  layer/ResKind resolution (erect/pico variant file names). Unknown events are
  never silently discarded.
- Gameplay observables carry over M5 §1/§2 (PBOT1, 160 ms window, 1 ms grid,
  `Std.int` judgement, health clamp/death order, hold drop stale-snapshot
  semantics, trace schema scoping). The trace schema "1" add-only rule
  (M5 §3) extends: M6 bumps the schema by adding fields/variants; nothing is
  removed/redefined.
- Opponent side: M5 explicitly deferred opponent auto-hit/sing scheduling to
  M6 ("phase 2 derives it from the same chart notes"). Weekend-1-style
  PlayAnimation choreography rides the same event dispatch.
- Rendering carries over RENDER-BEHAVIOR.md + ADR-0020 (adaptation fixtures,
  no digest regeneration, establishment path). Pixel style (nearest/box
  filtering + pixel backgrounds) must be added as a new sanctioned note-style
  behavior, not a renderer flag retrofit (phase-level amendments decide).

## 2. Engine decisions (1-2 approved, 3-5 approved by the 2026-11-08 decision round)

1. **ACCEPTED — acceptance scope = exactly `corpus.txt`'s 61 chart-ids.** The
   sserafim content parses without error (preserve raw events) and exercises no
   acceptance rule; it is treated as data, not doctrine.
2. **The deterministic gameplay model stays Bevy-free** (oxide-game `slice`
   remains the trace authority); rendering consumes the trace/events in the
   established scene/layer pipeline. PlaySession continues to reuse it
   (ADR-0021 pattern).
3. **Trace schema strategy:** schema `2` = schema `1` + per-field additions
   (opponent rows, event rows, accuracy/results aggregates, camera/bop state
   rows), serialized with the same field-order/serde discipline and two-run
   sha256 gates. `GameplaySimState::build` becomes song-parameterized; the
   bopeebo fixture paths move from constants to explicit parameters.
4. **Phasing:** seven phases below (6A–6G); at most three workers; one heavy queue job;
   xtask/gates ownership stays with the orchestrator per phase, contract
   amendments are written by the phase's analysis round.
5. **Fixture growth:** render-diff fixtures beyond bopeebo follow the ADR-0020
   establishment path per added song/stage — explicitly listed as acceptance
   obligations, never regenerated during `verify`.

## 3. Phased plan (objectives; each phase = analysis freeze → implementation → evidence)

### 6A — song-agnostic gameplay sim + opponent side + trace schema 2
- Objective: `GameplaySimState` runs ANY corpus chart-id (default/erect/pico/
  bf/lit-up), player AND opponent derivation, chart events consumed at the
  trace level; two-run sha256 per chart; negatives from M5 kept green.
- APPROVED (2026-11-08): sserafim stays outside the acceptance inventory;
  its charts must still parse without error and preserve their raw events.
- Write set: `crates/oxide-game/src/slice*`, `crates/oxide-data` additions only
  if a parsing gap blocks a chart; tests in-crate/gated tests.
- Acceptance: new `gameplay_sim_corpus` gate (61/61 chart-ids, difficulty
  coverage per metadata; difficulty list derived from metadata + corpus-id) —
  wired in a later phase or immediately if ready.
- Native obligation (approved pattern): one scoped gameplay/opponent probe
  round before the opponent-row implementation freezes.

### 6B — character/stage rendering for the four renderTypes + pixel notes
- Objective: load/render any of the 21 stages (animated props/Boppers) and all
  editorial characters under sparrow/multisparrow/packer/animateatlas
  (multianimateatlas exists for bf only; generalizations share it); pixel
  note style rendering (`noteStyle.pixel` nearest filtering etc.).
- Write set: `crates/oxide-atlas` (extensions), `crates/oxide-game/src/render*`.
- Acceptance: render-diff establishments for a representative fixture set
  (proposal: mainStage/funkin — already frozen; phillyTrainErect or
  phillyStreets (weekend-1); school pixel (senpai); derivable after 6A/6B
  load-inventory checks) + a structural load-inventory check (21 stages, 43
  characters load without error).

### 6C — song event semantics (camera/zoom/bop/animation/scroll/icon)
- Objective: FocusCamera/ZoomCamera/SetCameraBop/SetHealthIcon/PlayAnimation/
  ScrollSpeed consumption in scene + camera pipeline; EnableMask preserved;
  unknown-event recording completed (sserafim events recorded, unimplemented).
- Behavior freeze BEFORE implementation: original source analysis of the
  CameraForestSet (ease names, duration units = beats, stage/character zoom
  modes, default values) — pending observation obligation, no transcription
  allowed before the freeze.
- Evidence: native reference observations for camera choreography (needs the
  M5-probe-style approval workflow) OR explicit derived-only labeling with
  pending-parity honesty. Two-run determinism gates on camera state rows.

### 6D — gameplay HUD set + game-over/results lifecycle
- Objective: countdown (3/2/1/GO), judgement popups, combo numbers, note
  splashes, hold covers; GameOverSubState boundary per M5 §1.4; results
  (score, accuracy, tallies) semantics frozen from observed scoring formulas.
- Write set: `crates/oxide-game/src/play/` + `render/`; `oxide-data` save
  fields ONLY if a real consumer needs them (M7 otherwise).
- Acceptance: visual fixtures for one or two representative states + trace
  additions (countdown ticks, splash triggers) per schema add rule; human
  windowed playtest re-playable.

### 6E — Weekend-1 choreography + week-6 pixel style + dialogue (dialogue gate APPROVED for M6)
- Objective: weekend-1 (darnell/lit-up/2hot) scripted choreography incl.
  PlayAnimation-driven sequences and pixel+school presentation for week-6;
  the in-game dialogue gate for week-6 (senpai/thorns) — the dialogue data
  format and the pre-song dialogue box, per the approved phase-0 decision.
- IMPORTANT scope boundary (APPROVED 2026-11-08): in-game dialogue only;
  dialogue meta/UI/save overlap stays M7.
- Behavior freeze required from original source (dialogue parser, weekend-1
  sequence scripts); native-game observations pending the probe approval.

### 6F — acceptance wiring for `full_chart_gameplay`
- Objective: implement the M6 gate in `xtask` (corpus mode, 61/61 sim +
  inventory load checks + representative render/perf evidence), keep
  fail-closed semantics, cumulative profile wiring (--milestone M6), and
  record deferred parity obligations (native rendered-output traces,
  Windows/hardware, CI artifacts).
- Write set: `xtask/` only (orchestrator-assigned tooling task); fixture
  manifests stay ADR-protected.

### 6G — Native visual shaders & stage/camera post-processing effects (APPROVED 2026-11-09)
- Objective: reimplement the native V-Slice gameplay shaders and visual effects
  in Bevy/WGSL (`funkin/graphics/shaders/*` + stage/character script bindings):
  - Per-sprite / per-character shaders: `AdjustColorShader` (hue, saturation,
    brightness, contrast for erect stages/characters), `DropShadowShader` (rim
    lighting / drop shadow with optional mask threshold on erect stages),
    `HSVShader`, `MultiplyShader`, `StrokeShader`, and `LeftMaskShader` /
    `SserafimShader` (`EnableMask` event consumption).
  - Stage / post-processing shaders: `RuntimeRainShader` and `PuddleShader`
    (Philly Streets / Weekend-1 dynamic rain intensity and puddle reflections),
    `GaussianBlurShader`, `WaveShader` / `WiggleEffectRuntime` (senpai/thorns
    background distortion), and `MosaicEffect`.
- Behavior freeze required from original source (`funkin/graphics/shaders/*.hx`,
  `assets/shaders/*.frag`, and the stage/character `.hxc` script parameters)
  before implementation.
- Write set: `crates/oxide-game/src/render*` (WGSL shaders, Bevy material/post-process
  pipelines, scene shader uniforms); `crates/oxide-vfs` if shader asset resolution
  requires extensions.
- Acceptance: deterministic shader uniform progression in scene steps, visual
  render-diff fixture coverage for representative shader-active stages (e.g.
  erect adjust-color/drop-shadow, weekend-1 rain, thorns wiggle) under ADR-0020,
  and interactive playtest verification.

### 6H — PlayState visual polish (splashes, hold covers, popups, countdown) & `--song` playtest CLI (APPROVED 2026-11-12)
- Objective: close all remaining PlayState visual exclusions from §6D/§13.2
  before starting Milestone 7 (UI/Menus), and generalize `oxide play` beyond
  `bopeebo`:
  - **Note Splashes:** `noteStyle.noteSplash` spawning on player `Sick!` note
    hits across funkin/pixel styles (randomized/deterministic variant selection,
    offsets, scale, fps).
  - **Hold Covers:** `noteStyle.holdNoteCover` lifecycle (start, hold loop,
    release/end burst) for player and opponent sustain notes.
  - **Judgement Popups & Combo Digits:** `sick`/`good`/`bad`/`shit` rating
    sprites + `num0..9` combo counter sprites (`PopUpStuff` native velocity,
    gravity/acceleration, fade out, pixel vs funkin styles).
  - **Countdown Visuals & Audio:** `ready` / `set` / `go` sprites +
    `intro3`/`intro2`/`intro1`/`introGo` audio stems aligned with Conductor
    countdown ticks (respecting §14 story cutscene cancellation on
    `senpai`/`roses`/`thorns`/`darnell`).
  - **Song-Parameterized `oxide play` CLI:** generalize `PlayConfig`,
    `PlaySession`, `play/audio.rs`, and `src/main.rs` (`--song <id>` and
    `--story`) to load and interactively play any corpus song/variation with
    dynamic Inst/Voices audio, 6E cutscenes/dialogue, and 6G shaders while
    keeping `bopeebo` as the default.
- Behavior freeze required from original source (`NoteSplash.hx`,
  `NoteHoldCover.hx`, `PopUpStuff.hx`, `Countdown.hx`, `Strumline.hx`,
  `PlayState.hx`) before implementation. Existing `render-slice-2` frozen
  fixture digests must remain preserved or explicitly updated under ADR-0020
  human approval once the 6H freeze determines layer/fixture impact.
- Write set: `crates/oxide-game/src/render*`, `crates/oxide-game/src/play*`,
  `src/main.rs` (plus `xtask/` if 6H fixture/gate coverage is added).

## 4. Dependencies and constraints

- M4 pending obligations (input replay/jitter, native conductor corpus traces)
  are NOT M6 deliverables and stay listed on M4 acceptance.
- One heavy queue job at a time; do NOT set `CARGO_BUILD_JOBS` or `-j` CPU caps during Cargo compilation (superseding the earlier `CARGO_BUILD_JOBS=2` limit per human instruction; let Cargo use all available CPU cores).
- Foundation crates never depend on oxide-game; new dependencies need the
  AGENTS rationale (no additions anticipated; existing serde/kira/bevy stack).
- Every fixture/tolerance/acceptance-meaning change needs its ADR + human
  approval; nothing here pre-authorizes one.

## 5. Pending behavior-freeze obligations (analysis before the related phase lands)

1. Event semantics (Camera/ease conventions, ZoomCamera modes, SetCameraBop
   rate units, ScrollSpeed easing) — native source analysis.
2. Opponent sing/idle scheduling derived from behavioral observations (hit
   animation send-time etc.) — probe extension approved workflow.
3. Countdown/audio-start interaction (when the song actually starts relative
   to the countdown) — observation; the M4 conductor timeline may already
   cover the countdown offset (verify against `countdown-1ms-playback-1ms-v1`).
4. Note splash effect parameters + judgement-popup sprite parameters —
   observation from the style sheets + source.
5. Weekend-1 sequence scripts + week-6 dialogue format/content — observation.
6. Results accuracy formula + save-adjacent outputs — observation (save files
   remain M7).
7. Native gameplay shaders and post-processing effects (`AdjustColorShader`,
   `DropShadowShader`, `RuntimeRainShader`, `PuddleShader`, `WiggleEffect`,
   `EnableMask` / `LeftMaskShader`, and stage `.hxc` uniform parameters) —
   observation from `funkin/graphics/shaders/*.hx` and `assets/shaders/*.frag`
   before 6G lands. → **FROZEN 2026-11-11 as §15** (EnableMask targets
   DropShadowShader, not LeftMaskShader; PuddleShader is dead code).

## 6. Honesty boundary summary

This contract pre-declares NO parity beyond the frozen per-phase behavior
amendments and their gates. Full-chart *acceptance* additionally requires:
measured corpus inventory evidence, native reference traces (already a
standing M4/M5 obligation pattern), CI artifacts for the aggregate profiles,
Windows/platform evidence — all pending, never inferred locally.

## 10. Phase 6A behavior freeze — song-agnostic sim, opponent rows, schema 2

Labels follow `docs/WORKFLOW.md` §2. Observed citations read the installed
checkout `$HOME/Descargas/fnf/Funkin/source` (read-only, 2026-11-08); no
modification of the original. The M5 frozen trace schema "1" and the round-1
`note-native` fixtures stay byte-frozen; opponent semantics add a NEW probe
file set and a schema "2" addition path (M5 §3 add-only rule).

### 10.1 Observed opponent pipeline (processNotes, PlayState.hx L2799-2871)

- `processNotes(elapsed)` runs four loops in order: opponent notes → opponent
  holds → player notes → player holds (L2805-2919). The M5 round-3 frame
  ordering (super.update FIRST → mirror holds → conductor update →
  `processInputQueue` L1327 → `processNotes` L1332) therefore covers the
  opponent loops ENTIRELY: they run inside the same `processNotes` call,
  after the conductor is at the current tick's songPosition.
- Opponent note loop (L2806-2833): `if (note == null || !note.alive) continue;`
  then `GRhythmUtil.processWindow(note, false)` (GRhythmUtil.hx L69-112):
  - window miss → `hasMissed = true; mayHit = false`, and the opponent loop
    has NO `note.hasMissed` handling (only the hold loop reacts; no opponent
    miss event exists on this path).
  - botplay hit: `songPosition >= windowCenter` → dispatched
    `HitNoteScriptEvent(note, 0.0, 0, 'perfect', false, 0)` (L2812). If NOT
    canceled: the legacy-voice branch (L2819-2827,
    `vocals.opponentVolume = opponentVocalsVolume` when
    `vocals.legacyVoiceSystem && !vocals.legacyVoiceUsesPlayer`),
    `opponentStrumline.hitNote(note)` (receptor confirm + cleanup) and
    `playNoteHoldCover(note.holdNoteSprite)` when the note has a hold.
  - The opponent loops NEVER touch score, health, combo or tallies (comment
    at L2923: scoring happens in the player hold loop only).
- Opponent hold loop (L2840-2869): `hitNote && !missedNote && sustainLength >
  0` → IF `currentStage.getDad().isSinging()` THEN `holdTimer = 0` (keeps the
  opponent singing while the hold runs). Separately
  `holdNote.missedNote && !holdNote.handledMiss` → `handledMiss = true` and,
  IF `holdNote.scoreable`,
  `currentStage.getOpponent().playSingAnimation(direction, true)` (miss
  form). NO score/health penalty exists for dropped opponent holds.
- Strumline split (PlayState.hx L2521-2562): `builtNoteData` notes in chart
  array order (the shipped path has no mods, so the SongLoad dispatch
  returns the chart as parsed); notes before `startTime` are skipped;
  `getStrumlineIndex() = floor(data / 4)` (SongData.hx L1155-1158) → 0 =
  player / 1 = opponent; `Highscore.tallies.totalNotes++` counts SCOREABLE
  PLAYER notes only; `mirrorSongData` is false in the shipped pipeline
  (PlayState.hx L733; only the DEBUG freeplay SHIFT/CONTROL path sets it,
  FreeplayState.hx L2818), so `GRhythmUtil.mirrorNoteDirection` is NOT
  applied to gameplay chart notes.
- Direction on each strumline: `getDirection() = data % 4` (SongData.hx
  L1129-1131): opponent notes with data 4..7 map to strumline directions
  0..3 WITHOUT mirroring.
- Note kinds (PlayState.hx L2537-2541): `NoteKindManager.getNoteKind(kind)`
  non-null → `scoreable = noteKind.scoreable`; on this checkout every corpus
  note has `kind = null` (contract §0), so `scoreable` stays true for all
  recorded notes (limitation carried in the gate labels).
- Player side is unchanged from M5 §1/§2; not re-derived here.

### 10.2 Probe extension (NOTE-BEHAVIOR amendment; native evidence for 10.1)

- New probe round (driver `native-note-probe-2`, NEW fixture directory
  `tests/fixtures/note-native-opponent/`): round-1 fixtures, hashes and rows
  remain byte-frozen (fail-closed); round-2 files live beside them, hashed in
  their own `_manifest_native_opponent.json`.
- Retained-code additions: the opponent note/hold loops (PlayState.hx
  L2806-2869) become REAL retained bodies for round 2; they read the same
  whitelisted fields plus the `currentStage.getDad()/getOpponent()` shells
  and the legacy-voice branch surface. Driver: opponent notes/holds ARE
  constructed from the same chart JSON (lanes 4..7), chart array order,
  1:1 with the player construction; NOTE-BEHAVIOR §5 otherwise unchanged.
- Row schema: NEW row kind `opponent_hit` (schema `2`) = the M5 §2 shared
  header exactly — `schema:"2"`, `kind:"opponent_hit"`, `sim`,
  `input_ms:0.0`, `input_time_us:0.0`, `note_id`, `note_direction`
  (strumline direction 0..3 after `data % 4`), `judgement:"perfect"`
  (observed literal), `raw_note_diff:0.0`, `note_diff_int:0` — with NO
  score/health/combo/tallies fields. Dropped opponent holds and `holdTimer`
  resets produce NO trace rows ([D] exclusion; scene-level semantics belong
  to 6B/6C).
- Fail-closed rule amendment (round 2 ONLY): multiple dispatched events per
  tick are allowed when all of them are `opponent_hit` rows (their fields
  carry no post-event state); the round-1 one-stateful-dispatch rule stays
  for player-path rows ([D], recorded).
- The probe consumes the SAME exported stream format (§4 `note-stream-1`);
  opponent enablement is a round tag in the manifest; stream sha256s are
  re-frozen for the round-2 run.

### 10.3 Engine decisions (Oxide sim integration)

1. Trace schema "2" adds variant `OpponentHit` (serde field order per 10.2);
   `record()` order follows the native frame order: opponent rows BEFORE the
   player-path rows processed in the same `step()` tick (the opponent loops
   run first inside `processNotes`), shared-header field order unchanged.
2. `GameplaySimState::build` gains chart-id/selection parameters over
   `(song, variation)` with corpus.txt-style ids (`bopeebo-erect` → song
   `bopeebo`, `-erect` file suffixes); unknown/missing ids fail closed with
   a typed error. The bopeebo M5 fixture paths move from module constants
   to explicit parameters; the M5 gate output MUST stay byte-identical
   (two-run sha256 repr).
3. The opponent rows drive the renderer: `render/scene.rs` consumes
   `OpponentHit` rows instead of its internal `trigger_opponent_notes`
   approximation (`note.time <= now`); on the 1 ms grid both fire the
   receptor/character decisions at the same tick. The render-slice fixture
   digest equality is the acceptance check; a digest change pauses for
   analysis instead of a silent re-freeze.
4. Chart song events (`songEvents`) are NOT consumed in 6A beyond preserved
   parsing (M1); event CONSUMPTION (FocusCamera etc.) stays phase 6C; the
   M5 §8 phase-2b exclusion stays recorded for the current fixtures.
5. `holdTimer`/drop-miss-animation semantics need no trace rows ([D]): the
   sim has no character objects; the scene continues to derive sing
   schedules from the opponent hit rows (sing-time behavior unchanged
   until 6C/6B amends it).

### 10.4 Validation obligations for 6A

- Portable tests: synthetic multi-strumline fixtures (lanes 0..7, temp-dir
  based like the existing slice tests) covering: opponent auto-hit at
  `songPosition >= strumTime` on the 1 ms grid; opponent lanes never mutate
  score/health/combo/tallies; schema-2 round-trip serialization + two-run
  sha256 equality; chart-id lookup and error paths; bopeebo easy/normal/hard
  trace digests byte-identical after the constants→parameters move.
- Native probe round-2 run: frozen `opponent_hit` rows for bopeebo
  easy/normal/hard, compared field-wise by the comparator.
- Corpus prototype run (61/61 chart-ids over the explicit assets root):
  every id parses and the sim produces a canonical 1 ms-grid trace through
  the last note without error paths (prototype script; the `full_chart_
  gameplay` gate wiring itself remains 6F).
- Quick `cargo xtask verify` green; render-diff digest equality on the
  bopeebo fixtures after the scene change (10.3.3).

## 11. Phase 6B behavior freeze — all renderTypes, 21 stages, pixel note style

Observed facts read the installed checkout `$HOME/Descargas/fnf/Funkin` (source
+ assets, read-only, 2026-11-08) under WORKFLOW §2 labels; engine decisions are
Oxide choices. All corpus files referenced here are outside the acceptance
inventory; the sserafim stage and its three characters ARE loaded by the
corpus chart-id `spaghetti-default` (see the §0 correction), while the
sserafim custom events stay non-acceptance data.

### 11.1 Observed data facts

- **renderType inventory** (observed in the asset checkout, M6 §0): the 43
  referenced characters split as `animateatlas` 24, `multianimateatlas` 10,
  `multisparrow` 3, `sparrow` 1, `packer` 1, null 7 (gf-pixel, gf-tankmen,
  nene-pixel, nene-tankmen, senpai-angry, senpai, spooky,
  spooky-dark). **Default**: `CharacterDataParser` normalizes a null renderType
  AFTER parsing (`input.renderType = DEFAULT_RENDERTYPE`, CharacterData.hx
  L510-512) with `DEFAULT_RENDERTYPE = Sparrow` (L463) and
  `DEFAULT_ISPIXEL = false` (L457). The `default:` branch of the constructor
  switch (`Creating character with undefined renderType`, L277-280) is therefore
  UNREACHABLE for JSON-loaded characters on the shipped path; the 7 null files
  load as Sparrow characters. Engine decision: Oxide's loader implements
  exactly `animateatlas | multianimateatlas | multisparrow | sparrow | packer |
  (null -> sparrow)` and ANY OTHER string fails closed (typed error naming the
  file), rather than mimicking the Custom/script path (no scripts in the
  corpus).
  - Observed scale/isPixel pairing on the pixel characters:
    `pico-pixel`/`spirit`/`senpai`/`gf-pixel` carry `scale: 6` +
    `isPixel: true`; non-pixel characters carry `scale` absent/1.
- **Sparrow/Packer/MultiSparrow pipelines** (observed):
  - `SparrowCharacter.loadSpritesheet` → `Paths.getSparrowAtlas(assetPath)`
    (Paths.hx L177-180: `<image>.png` + `images/<key>.xml` via `getPath`),
    `antialiasing = !(isPixel ?? false)` (L88-97 — the M5 log9 rule),
    `setScale(_data.scale)` (L99; default scale 1).
  - `PackerCharacter.loadSpritesheet` → `Paths.getPackerAtlas(assetPath)`
    (Paths.hx L224-228: `<image>.png` + `images/<key>.txt`), same
    isPixel/scale block (PackerCharacter.hx L98-115).
  - `MultiSparrowCharacter`: main sheet from `_data.assetPath` (L66) PLUS one
    extra sheet per animation that sets `anim.assetPath` (L108); frames
    collections are combined (`FlxAnimationUtil.combineFramesCollections`);
    each animation is resolved by prefix on its OWN sheet.
  - **Animation registration** (FlxAnimationUtil.addAtlasAnimation,
    FlxAnimationUtil.hx L53-76, used by Sparrow/Packer/MultiSparrow characters
    AND sparrow/packer stage props): `frameRate = anim.frameRate ?? 24`,
    `looped = anim.looped ?? false`, per-anim `flipX/flipY ?? false`,
    `frameIndices != null && non-empty` → `addByIndices(..., frameRate,
    looped, flipX, flipY)`; otherwise `addByPrefix(...)` (prefix match on
    frame NAMES). `addByIndices` preserves the INDEX ORDER given in the data
    (gf-pixel's `danceLeft` 16-entry frameIndices list is the shipped case).
  - **Character file asset inventory**: 34 character `assetPath` values carry
    the explicit `shared:` prefix (plus 54 per-animation `shared:` values on
    animateatlas variants); 12 plain paths exist AND resolve under
    `shared/images`; one per-animation path carries `week7:`
    (tankman-bloody). All characters' sparrow/packer XML/TXT data files
    resolve (0 missing).
- **AnimateAtlas/MultiAnimateAtlas** machinery is the M5-observed/implemented
  one (RENDER-BEHAVIOR §4/§5); M5's bf slice uses `multianimateatlas`.
  Observed: per-animation `assetPath` (54 `shared:` values + 1 `week7:`)
  selects ANOTHER animate atlas collection; collections combine into one
  frames surface (MultiAnimateAtlasCharacter). M2's `AnimateStage` covers
  main-frame/symbol/label playback used by 6B exactly (no new semantics).
- **Stage prop surface** (observed across all 21 stage files): top level
  `{name, directory, cameraZoom, characters, props, version}`; per-prop
  fields seen: `zIndex (164/169)`, `danceEvery (125)`, `position (169)`,
  `scale (168)`, `name (169)`, `isPixel (108)`, `animType (116)`,
  `scroll (169)`, `assetPath (169)`, `animations (129)`,
  `startingAnimation (43)`, `alpha (41)`, `atlasSettings (3)`, `angle (2)`,
  `flipX (3)`; animTypes: sparrow 110, (none) 53, animateatlas 4, packer 2.
  danceEvery: 0 (98), 1/1.0 (27), null (44), 2 (2). 24 props carry
  `isPixel: true` (week6 school family + 2 others), 145 false. 3 props carry
  `atlasSettings {applyStageMatrix: true}` (animateatlas Erect props).
  12 props use `#RRGGBB` solid-color assetPaths.
- **Stage build semantics** (Stage.hx buildStage L181-300, observed):
  - prop class: `danceEvery != 0 || animations non-empty` → Bopper, else
    StageProp (L201-209).
  - animated sheets by `animType`: `packer` → loadPacker (TXT),
    `animateatlas` → loadTextureAtlas(assetPath, _data.directory,
    atlasSettings), default (also missing/none-with-animations) →
    loadSparrow (L212-222).
  - `#RRGGBB` assetPath → makeSolidColor(width, height, color) with width/
    height from `scale` (scalar or pair; default 1×1) (L224-244); static
    (non-animated, non-solid) → loadTexture + `active = false` (L248-251).
  - failing texture (null frames) SKIPS the prop with an error line, NOT a
    crash (L252-257; irrelevant — all 46 animated props resolve: 42
    sparrow/packer + 4 animateatlas directories confirmed present; the
    resolution order test found 0 misses).
  - non-solid: scale from data (scalar or pair; default 1), `updateHitbox()`,
    `x/y = position`, `alpha = dataProp.alpha`, `antialiasing = !isPixel`
    (L268: the log9 rule for props), scrollFactor from `scroll`, `angle`,
    `color`, `blend`, `zIndex`, `flipX`/`flipY` (L262-282). The
    pixel-perfect render overrides (L284-287) are COMMENTED OUT in the
    shipped build — not a parity obligation.
  - animation registration per animType (L284-300): packer →
    `propSprite.animation.add(name, frameIndices)` (+ Bopper offsets via
    setAnimationOffsets), animateatlas → addTextureAtlasAnimations,
    sparrow → addAtlasAnimations (+ Bopper offsets for every anim).
- **Bopper semantics** (Bopper.hx, observed): `onStepHit`: dance iff
  `danceEvery > 0 && step % (danceEvery * STEPS_PER_BEAT) == 0` (STEPS_PER_BEAT
  = 4; Constants) → `dance(shouldBop)` with shouldBop true → the dance restarts
  (forceRestart = true). `dance()` (L179-209): `shouldAlternate = hasAnimation
  ('danceLeft')`; alternating plays `danceRight`/`danceLeft` in sequence
  (hasDanced starts false → the FIRST dance plays `danceLeft`, then
  alternates); non-alternating plays `idle`. `correctAnimationName` strips a
  `-<suffix>` (e.g. `-scared`) when the name does not exist, recursively
  (L214-231). `startingAnimation` (stage props + gf) is the first animation
  played on creation (set in the constructed class/BaseCharacter).
  idleSuffix/-alt handling: the corpus uses NO idleSuffix (pixel variant
  swaps `-pixel` ASSETS and `valueSuffix`... observed: the bopper's
  `set_idleSuffix` (L42-70) only applies `-<suffix>`-suffixed animation names;
  no corpus prop/character data sets idleSuffix at parse level (the field
  exists only for bf-christmas style holiday handling? — deliberate EXCLUDED:
  engine decision: Oxide ignores idleSuffix in 6B; the corpus has none
  setting it, recorded as a limitation, not a positive behavior).
- **Camera**: per-stage `cameraZoom` (mainStage 1.1, others e.g. 1.0,
  phillyTrainErect etc.); the M5 camera algebra (RENDER-BEHAVIOR §3 + follow
  offsets) stays frozen; the per-stage cameraZoom is consumed as the stage
  zoom multiplier exactly as mainStage's 1.1 already is.
- **Note style (pixel)** (observed `preload/data/notestyles/pixel.json`):
  assets `note`, `noteStrumline`, `holdNote` exist with `scale: 6.0`,
  `isPixel: true` and `offsets` (`noteStrumline` `offsets [28, 32]`,
  `holdNote` `offsets [2, 1]`); asset paths carry EXPLICIT library prefixes
  (`week6:weeb/pixelUI/arrows-pixels`, `week6:weeb/pixelUI/arrowEndsNew`) that
  resolve under `assets/week6/images/...` (confirmed on disk). The funkin
  style (M5 frozen) has no library prefixes and no pixel flags.
  `NoteStyle.hx:87/225/253` (observed in M5 log9): `antialiasing =
  !(isPixel ?? false)` → pixel pages use NEAREST/box filtering; other pages
  stay linear. noteSplash/holdNoteCover/countdown/judgement/combo assets
  EXIST in the pixel and funkin styles; they stay 6D deliverables — 6B renders
  only note/receptor/hold trails.
- **Asset resolution chain** (Paths.getPath L44-58 + getLibraryPath L60-73):
  explicit `week6:`-style prefix → `week6:assets/<lib>/<file>`; plain key →
  current level directory first (`week6:assets/week6/...`), then `shared:`,
  then preload. Stage `directory` values observed: week1 (2 stages),
  week2 (2), week3 (2), week4 (2), week5 (3), week6 (4), week7 (2), weekend1
  (3), sserafim (1, loaded via the corpus chart-id spaghetti-default).
  Resolution CHECK (observed
  scan over all 169 props, keys `[directory → shared → preload]`): 157 file/
  dir hits, 12 solid-color, **0 misses**. Characters: plain paths resolve
  under `shared/images` (12/12), explicit prefixes cover the rest.

### 11.2 Engine decisions (Oxide integration)

1. ** loaders are renderType-driven in `render/load.rs`** with the frozen
   bopper/animation semantics of 11.1; the animate pipeline reuses the M5/M2
   machinery; sparrow/packer reuse the M2 parsers with prefix selection
   semantics (addByPrefix over frame NAMES sorted by document order;
   addByIndices reorders by the given indices list; frameRate default 24).
2. **Stage props** become a first-class scene item class: static texture,
   solid color, animated (sparrow/packer/animateatlas), with zIndex ordering,
   scroll factor, isPixel sampling, alpha/angle/flip; boppers dance on the
   frozen step rule. Prop draw order = the existing z-sorted painter chain
   (props before/behind characters by zIndex; ties keep document order).
3. **Pixel**: NEAREST sampler pages when the source data flags `isPixel`
   (note style assets, pixel characters, pixel props); scale fields (6.0)
   consumed `setScale`-style; NO pixel-perfect integer snapping (commented out
   in the native). The M5 geometry chain (content placement with the
   setup-hitbox term; hold trail geometry) stays FROZEN for all styles — the
   pixel style reuses it with its own scale/offsets (28/32, 2/1).
4. **Camera zoom/scroll extended**: per-stage cameraZoom; prop scrollFactor
   joins the camera matrix (camera scroll 0/1 handling unchanged; camera zoom
   application per-stage value replaces mainStage's constant).
5. **Wiring**: the interactive play mode and the render front end become
   stage/song-parameterized along the 6A `build_chart`-style id selection
   (engine decision; the M5 bopeebo fixture inputs stay byte-frozen; fixture
   GROWTH for other stages follows the ADR-0020 establishment path — listed
   in 11.4, not pre-approved).

### 11.3 Validation obligations for 6B

- **Structural load-inventory check** (new xtask gate candidate, wired when
  the tooling lands): with the explicit assets root, load every stage in
  `preload/data/stages` (21 files; sserafim parses too but is
  excluded from acceptance inventory) and every REFERENCED character
  (43) — fail closed on any missing asset/animation resolution error; print
  the per-stage prop counts (169 total) and per-renderType character counts.
- **Regression invariants**: the M5 bopeebo `render-diff` digests stay byte-
  identical; the M6 corpus gameplay prototype (61 chart-ids) stays green —
  the sim is out of the render path, but the play-session loader path runs
  under the same load surface.
- **Fixture growth**: ONE representative render-diff establishment draft
  (proposal: the `school` pixel stage, exercising the pixel note style +
  school props with default-Sparrow pixel characters; loader called via the
  senpai chart-id) run in establishment mode and FROZEN ONLY after the
  human approves the fixture-growth step (ADR-0020 path). Additional
  representative stages (mallXmasErect boppers; weekend1) can follow in a
  later round with the same approval path.

### 11.4 Approval items (human)

- The representative render-diff fixture growth (school pixel stage or
  equivalent) is a reference-fixture change: ADR-0020 establishment path,
  review and human approval before anything is frozen. Nothing in 6B
  pre-freezes new digests.
- Windowed human playtest of the generalized stages/characters remains the
  HR visual arbiter for anything not covered by digest equality.

### 11.5 Amendment (2026-11-08, 6B integration review): sserafim stage inventory + MD.W/H tolerance

- CORRECTION to §0: the `sserafim` stage and the three `sserafim-*`
  characters ARE part of the measured acceptance load surface via the corpus
  chart-id `spaghetti-default` (spaghetti-metadata.json observed). The
  sserafim CHART-IDS and custom EVENTS remain outside the acceptance
  behavior.
- OBSERVED (sparrow sakura CharacterAn): `shared/images/characters/sserafim/
  sakura/Animation.json` carries `MD: {"FRT":24}` WITHOUT `W`/`H`; the
  sibling `spritemap1.json` `meta.size = {w: 2043, h: 1950}` carries the page
  size. The shipped data must stay loadable (acceptance inventory), so:
- ENGINE DECISION (approved): `AnimateStage` tolerates missing
  `MD.W`/`MD.H` by falling back to the spritemap page size
  (`meta.size.w/h`) — data-equivalent to the MD fields on every file where
  both exist. Strictness preserved: missing `MD.FRT`, missing BOTH sources
  of size, or malformed JSON still fail closed; present `MD.W/H` win over
  the fallback. The equivalence check is: existing frozen parse fixtures
  (with MD.W/H) emit identical parse results, plus a ship-shaped
  `MD={"FRT":24}` regression test.

### 11.6 Amendment (2026-11-08, 6B integration review round): partial MD, applyStageMatrix, solid props

- ENGINE DECISION (MD pair consistency): a `MD` object carrying EXACTLY ONE of
  `W`/`H` is treated as pair-absent for size selection (the spritemap
  `meta.size` fallback applies; fail closed when that is missing too). The
  shipped corpus has only full or absent MD pairs (observed); partial pairs
  appear only in synthetic/mod data. Regression tests pin both partial
  cases.
- ENGINE DECISION (`atlasSettings.applyStageMatrix`): 3 corpus props carry
  `{"applyStageMatrix": true}` (mallXmasErect `bottomBoppers`,
  tankmanBattlefieldErect `sniper`/`guy`; observed). The native placement
  matrix semantics (FlxAnimate symbol-instance matrix) are NOT frozen yet —
  implementing them derives geometry from an unpinned native algebra. Until
  the behavior freeze lands, props with `applyStageMatrix == true` are
  LOADED (inventory green) but NOT DRAWN, and the scene records a structured
  deviation line naming the prop (same honesty pattern as the `bfFakeOut`
  exclusion). This is a named 6C follow-up, not a silent skip.
- Review-found defects to fix (observed data cited): solid-color props must
  apply `dataProp.alpha` unconditionally (sserafim solidCover: alpha 0.0,
  zIndex 9999 — currently drawn opaque) and `scale` must be consumed ONCE
  (integer makeSolidColor dimensions; native Stage.hx skips scale.set for
  solids). Bopper classification vs the loader's ANIMATED branch are separate
  decisions (native: Bopper iff `danceEvery != 0 || animations non-empty`;
  loader uses a static texture when `animations` is empty).

## 12. Phase 6C behavior freeze — chart song event consumption

Observed facts read the installed checkout `$HOME/Descargas/fnf/Funkin` (source +
`.haxelib` flixel git + assets, read-only, 2026-11-08) under WORKFLOW §2 labels.
Event-kind counts over ALL `preload/data/songs/*/*-chart*.json` `events` arrays
(3638 events, 0 unsorted time pairs, 471 same-time-pair events): FocusCamera
1534, ZoomCamera 1509, SetCameraBop 407, PlayAnimation 69, ScrollSpeed 16,
SetHealthIcon 16, EnableMask 1 (ONLY `stress-chart-pico`, empty `v`), and 106
`sserafim*` custom kinds (excluded from the acceptance inventory; recorded).
Chart event `t` is in milliseconds (contract §0; M1 parsing unchanged).

### 12.1 Observed dispatch pipeline (PlayState.hx + SongEventRegistry.hx)

- Per-tick order in `PlayState.update` (L1035-1332, extends the M5 §10 frame
  chain): `super.update` FIRST (camera follow scroll, camera-zoom tween,
  scrollSpeed tweens, animations use the PREVIOUS tick's state) → mirror holds
  → `Conductor.instance.update` (L1147/1186) → camera bop decay block
  (L1217-1228) → `processSongEvents()` (L1324) → `processInputQueue` (L1327) →
  `processNotes` (L1332). New 6C insertion point: events are consumed AFTER the
  decay block; their state changes take effect in the camera motion of the
  NEXT tick's `super.update`.
- `processSongEvents` (L1472-1509): `SongEventRegistry.queryEvents(events,
  Conductor.instance.songPosition)` walks the events array from the persistent
  `nextEventIndex` (SongEventRegistry.hx L141-172), returns every event with
  `time <= songPosition` before the first future event (skipping `activated`
  ones), and stops. For each returned event: `eventAge = songPosition -
  time`; if `eventAge > 1000` and the handler's `processOldEvents` is false,
  the event is silently SKIPPED (`activated = true; continue`, L1485-1495).
  Otherwise `dispatchEvent(SongEventScriptEvent)` runs (script cancellation;
  no scripts on the shipped path [D: no cancel surface]) then
  `SongEventRegistry.handleEvent(data)` (L113-127), which always sets
  `activated = true` — even when the kind has NO handler (trace warning only).
- Handlers `processOldEvents = true` (constructor params): FocusCamera
  (FocusCameraSongEvent.hx L54-56), ZoomCamera (ZoomCameraSongEvent.hx
  L27-29), SetCameraBop (SetCameraBopSongEvent.hx L35-37), ScrollSpeed
  (ScrollSpeedEvent.hx L31-34), SetHealthIcon (SetHealthIconSongEvent.hx
  L30-34). PlayAnimation has NO params (PlayAnimationSongEvent.hx L19-21) →
  `processOldEvents = false` → PlayAnimation events older than 1000 ms are
  skipped. On the 1 ms sim grid with normal (non-skipping) playback the age
  rule never fires (events arrive the tick they are due) — the rule is
  implemented honestly anyway, exercised by synthetic tests.
- Ordering and ties: `resetEvents` sorts by time ascending
  (SongEventRegistry.hx L200-207, `SortUtil.eventDataByTime` =
  `FlxSort.byValues(a.time, b.time)`); the shipped chart arrays are already
  time-ascending (0 unsorted pairs observed), so the sort is a no-op on the
  corpus. Native `Array.sort` stability is NOT guaranteed; for same-time
  events Oxide keeps PARSE order ([D] engine decision; native tie order not
  frozen). Events appear in `PlayState.songEvents` via
  `currentChart.getEvents()` (Song.hx L825-827) directly from parsing.
- Field coercion (SongData.hx L795-895, OBSERVED): `getDynamic` = raw field;
  `getBool` = unchecked cast; `getInt` = field; String → `Std.parseInt`;
  other → unchecked cast; `getFloat` = field; String → `Std.parseFloat`;
  other → unchecked cast; `getString` = unchecked cast to String; all return
  null for missing fields/null value. Corpus uses STRING fields on observed
  events (`"char": "0"`, `"x": "0"`, `"y": "0"` on 46 FocusCamera events), so
  string coercion is REQUIRED. `data.getFloat('rate')` carries `?? default`
  always.
- Event time skip/back-jump handling (`handleSkippedEvents`, L179-194) only
  runs on song restart/skip paths; normal corpus playback never calls it.
  Oxide does NOT implement it in 6C (named limitation; replay/skip is a
  standing M4 obligation).

### 12.2 Observed per-event semantics

- **FocusCamera** (FocusCameraSongEvent.hx L65-158):
  - Fields: `x`/`y` floats default 0.0 (L59-60); `char` via `getInt('char')`,
    falling back to `cast data.value` when null (scalar-int values 0/1 exist
    on 136 events) then `DEFAULT_TARGET = 0` (L63, L78-81); `duration` default
    `4.0` steps (L61, L83-84); `ease` default `'CLASSIC'` (L62, L85-86);
    `easeDir` default `SongEvent.DEFAULT_EASE_DIR = 'In'` (L88), CLEARED to ''
    when `EASE_TYPE_DIR_REGEX / (In|Out|InOut)$/i /` matches ease or ease ==
    'linear' (L88-89). EFFECTIVE ease function = `FlxEase[ease + easeDir]`.
  - Target: `char -1` = literal (posX,posY); `0` = bf `cameraFocusPoint`;
    `1` = dad; `2` = gf (L97-137). `cameraFocusPoint` = `originalPosition +
    (width/2, height/2) + _data.cameraOffsets[0..1]` (BaseCharacter.hx
    `resetCameraFocusPoint` L350-358) plus the stage character slot
    `cameraOffsets` (Stage.hx L113-117; the M5 frozen placement already adds
    char+stage offsets). A null character (`getBoyfriend/getDad/
    getGirlfriend() == null`) traces and RETURNS WITHOUT any state change
    (L103-107 etc.). Any OTHER char value (e.g. 3+) hits the `default`
    branch which only traces (L135-137) and then STILL falls through to the
    ease switch with the literal (posX, posY) target — OBSERVED quirk: an
    unknown char id tweens the camera to (x, y) unshifted.
  - `duration` → seconds: `Conductor.instance.stepLengthMs * duration /
    Constants.MS_PER_SEC` with the conductor step length AT FIRE TIME
    (L149, L68 zoom). `duration` is IGNORED by CLASSIC and INSTANT.
  - `ease == 'CLASSIC'` (L142-145): `resetCamera(false, false, false)`
    (re-follow the follow point with LOCKON `DEFAULT_CAMERA_FOLLOW_RATE =
    0.04`, `targetOffset` reset, NO zoom reset, NO snap) →
    `cancelCameraFollowTween()` → `cameraFollowPoint.setPosition(target)`.
    The camera scroll then converges by the follow lerp on following ticks
    (frozen M5 algebra). NO zoom change.
  - `ease == 'INSTANT'` (L146-147): `tweenCameraToPosition(target, 0)` →
    `tweenCameraToFollowPoint(0)`: cancel follow tween; duration 0 →
    `resetCamera(false, false)` — re-follow + `focusOn(followPoint)` SNAP:
    camera.scroll jumps EXACTLY to `target - (640, 360)`.
  - Other (FlxEase lookup, L148-157): an unknown ease name makes
    `easeFunction == null` → trace + EARLY RETURN, no state change. Valid
    name: `tweenCameraToPosition`: set follow point, cancel follow tween,
    `camera.target = null` (follow DISABLED during the tween — no follow
    lerp), tween `camera.scroll` to `followPoint - (640, 360)` over
    `duration / playbackRate` seconds ([A] playbackRate = 1.0 on the shipped
    path), with onComplete → `resetCamera(false, false)` = re-follow + SNAP
    to the follow point (same INSTANT snap at tween end).
- **ZoomCamera** (ZoomCameraSongEvent.hx L37-79):
  - Fields: `zoom` default 1.0 (L32, L45); `widescreenScaleX/Y` default 0.0
    (L33, L47-48) — corpus NEVER sets them; at 16:9 desktop
    `FullScreenScaleMode.wideScale = (1, 1)` (FullScreenScaleMode.hx L76/425;
    overflow scaling branches L463/501), so `scaledZoom = zoom + zoom *
    ((1-1)*sx + (1-1)*sy) = zoom` ([D] widescreenScale accepted and ignored
    at 16:9; corpus always 1:1-equivalent). `duration` default 4.0 steps;
    `mode` default `'direct'` (L35) — 17 corpus events OMIT mode → direct;
    observed modes: stage 1429, direct 63. `ease`/`easeDir` defaults
    'linear'/'In' with the same regex suppression (L57-60).
  - `ease == 'INSTANT'` → `tweenCameraZoom(scaledZoom, 0, isDirect)` →
    `currentCameraZoom = scaledZoom * (direct ? FlxCamera.defaultZoom :
    stageZoom)` INSTANTLY (PlayState.hx L3999-4014; `FlxCamera.defaultZoom =
    1.0` flixel 6.1.2). Other ease unknown → early return, no state change;
    valid: tween `currentCameraZoom` to `scaledZoom * (direct ? 1.0 :
    stageZoom)` over the converted duration; per-frame rendered zoom stays
    `currentCameraZoom * bopMultiplier` (M5 frozen decay algebra).
  - `stageZoom` = `currentStage.camZoom` (PlayState.hx L323-330; the 6B
    per-stage `cameraZoom` values, mainStage 1.1; null stage → 1.05).
- **SetCameraBop** (SetCameraBopSongEvent.hx L40-53):
  - `rate = getFloat('rate') ?? Constants.DEFAULT_ZOOM_RATE = 4` (Int cast
    to Float); `offset = getFloat('offset') ?? Constants.DEFAULT_ZOOM_OFFSET
    = 0` (L251); `intensity = getFloat('intensity') ?? 1.0`; then
    `cameraBopIntensity = (1.015 - 1.0) * intensity + 1.0`;
    `hudCameraZoomIntensity = 0.015 * 2.0 * intensity`; `cameraZoomRate =
    rate`; `cameraZoomRateOffset = offset`. OBSERVED values: rates 4/1/2,
    offsets 1 or absent, intensities 0..1.8.
  - The bop gate (PlayState.hx L1846-1859, M5-frozen algebra): on stepHit,
    `Preferences.zoomCamera && camHUD.zoom < 1.35 * defaultHUDCameraZoom &&
    cameraZoomRate > 0 && (currentStep + cameraZoomRateOffset * 4) %
    (cameraZoomRate * 4) == 0` → `cameraBopMultiplier = cameraBopIntensity`,
    `camHUD.zoom += hudCameraZoomIntensity * defaultHUDCameraZoom`.
    `currentStep` is a FLOAT; the modulo compares float remainder == 0.
    `Preferences.zoomCamera` defaults TRUE (Preferences.hx L137-141) ([A]
    default save options). camHUD state is NOT rendered in 6C; the scene
    keeps a virtual HUD zoom value feeding ONLY this gate ([D]; the real HUD
    camera is 6D).
  - **DEFAULT-ZOOM-OFFSET CONTRADICTION (resolved this phase, do not edit
    RENDER-BEHAVIOR silently):** RENDER-BEHAVIOR §2 froze
    `cameraZoomRateOffset = 1 (PlayState.hx L364 default)`;
    `geometry.rs CAMERA_ZOOM_RATE_OFFSET = 1`. The on-disk source (Constants
    L251 `DEFAULT_ZOOM_OFFSET:Int = 0`, PlayState.hx L364 = that constant
    with the comment "Zero beats (on-beat)", SetCameraBopSongEvent.hx L46)
    says **0**. No native camera-row probe exists to discriminate. Resolution:
    the 6C event-driven bop state uses the OBSERVED default (0) for the
    mutable field; `geometry.rs` keeps the frozen constant 1 until the
    render-diff re-establishment (12.4/12.5) is approved — the delta is part
    of the same human-approved fixture path, never a silent edit.
- **PlayAnimation** (PlayAnimationSongEvent.hx L25-76):
  - Fields: `target` default 'boyfriend' (L21); `anim` default 'idle' (L22);
    `force` default false (L23). Target resolution (L41-57): 'boyfriend'/'bf'
    /'player' → bf; 'dad'/'opponent' → dad; 'girlfriend'/'gf' → gf; else
    `getNamedProp(target)` (stage prop names; 6B named props). Observed
    corpus targets: boyfriend 34, bf 18, dad 15, girlfriend 1, gf 1 (no
    named props; no unknown). Null target → trace only, no state change.
  - Character application (L59-66): `tempVocals = force`;
    `playAnimation(anim, force, force)` = restart=force, ignoreOther=force
    through the 6B animation surface (suffix stripping, dance semantics
    frozen in §11). Properties (`else` branch): plain `animation.play(anim,
    force)`. Unknown animation names keep the character's current animation
    (FlxSprite behavior) with a warning ([D]).
  - Post-animation behavior (observed BaseCharacter.hx L323-341): when the
    played animation finishes (name ends `'-end'` = Constants
    ANIMATION_END_SUFFIX L346, or starts 'combo'/'drop' and is not
    idle/dance-prefixed), the character forcibly `dance(true)` (returns to
    idle/dance). `tempVocals` silences/brings back the per-role vocal —
    AUDIO interplay NOT modeled in 6C rows ([D] named limitation).
  - The 1000 ms age rule lets stale PlayAnimation events get SKIPPED
    (processOldEvents = false) — implemented, exercised synthetically only.
- **ScrollSpeed** (ScrollSpeedEvent.hx L42-92):
  - Fields: `scroll` default 1.0 (L37); `duration` default 4.0 steps;
    `ease` default 'linear'; `easeDir` 'In' with regex suppression;
    `strumline` default 'both' → target NAMES `['playerStrumline',
    'opponentStrumline']` (L60-74; other values map to `<v>Strumline`;
    corpus always 'both'); `absolute` default false (L39).
  - `!absolute` → `scroll = scroll * (currentChart.scrollSpeed ?? 1.0)` —
    the per-difficulty chart scrollSpeed (observed per-chart metadata:
    easy/normal/hard ~1.5..3.2; erect 2.5..2.7). OBSERVED: absolute true on
    15/16 events (direct target), absolute false once (1.2x base,
    expoIn).
  - INSTANT → `tweenScrollSpeed(scroll, 0, ...)` → per target strumline:
    cancel ALL scrollSpeed tweens (PlayState.hx L4092-4104), snap every
    previously listed strumline to its PREVIOUS event target (prevScrollTargets
    L4059-4066/4078), then set each listed strumline.scrollSpeed = value.
    Non-instant: same cancel+snap, then FlxTween each listed strumline's
    `scrollSpeed` to `value` over `duration / playbackRate` seconds with the
    ease ([A] playbackRate 1.0). `prevScrollTargets` PUSHES the new `value`
    for every listed strumline after applying (L4078).
  - Strumline.scrollSpeed semantics unchanged from M5: `resetScrollSpeed`
    at strumline init = `chart.scrollSpeed ?? 1.0` (Strumline.hx L92/L99/
    L262); note geometry multiplies via `getNoteY` (frozen M5 algebra).
  - Evaluated scrollSpeed per tick = the in-flight tween's eased current
    value; note Y positions are re-derived from it per frame (already the
    M5 chain's per-frame geometry).
- **SetHealthIcon** (SetHealthIconSongEvent.hx L45-83):
  - Fields: `offsets = [offsetX ?? 0.0, offsetY ?? 0.0]` (L53-56);
    `id ?? Constants.DEFAULT_HEALTH_ICON = 'face'`; `shouldBop ?? true`;
    `scale ?? 1.0`; `flipX ?? false`; `isPixel ?? false`; `char ?? 0`
    (L37-43, L58-65). `char 0` → `iconP1.configure(data)`; `char 1` →
    `iconP2`; other → trace warning, no state change (L67-82). The HUD icon
    RENDERING and `HealthIcon.configure` sprite semantics are 6D deliverables
    (the HUD does not exist in the 6C scene): 6C records the normalized row;
    [D] no scene state beyond the row.
- **EnableMask / unknown kinds / sserafim custom kinds**: no handler exists
  in the shipped source (grep over `source/` for 'EnableMask': zero matches;
  the sserafim kinds likewise) → native `handleEvent` traces
  `'WARNING: No event handler for event with kind: X'` and sets
  `activated = true` (SongEventRegistry.hx L113-127). OBSERVED: NO engine
  state change. Oxide records a row and applies nothing (record-only),
  matching native trace-surface behavior ([D] row shape below).

### 12.2b Built-in handlers OUTSIDE the shipped inventory (scope note)

`SetTargetBopSpeed` (SetTargetBopSpeedEvent.hx), `SetStage` (SetStageSongEvent.hx)
and `SetCharacter` (SetCharacterSongEvent.hx) are built-in SongEvent subclasses
on the shipped source, with handlers, but appear ZERO times in the corpus
inventory. Engine decision: 6C treats them as the unknown-kind path
(record-only `unknown_event` rows) — their native handlers are NOT translated
in 6C; mod charts using them are outside the acceptance inventory (recorded
limitation, reversible if later phase evidence requires).

### 12.3 Observed ease functions (flixel 6.1.2, FlxEase.hx L36-244)

- The effective function is ALWAYS a concrete `FlxEase.<base><dir>` name
  (built as `ease + easeDir`; the regex-suppressed `easeDir = ''` yields the
  bare name). Corpus-used resolved names: FocusCamera — CLASSIC, INSTANT,
  linear, quadIn ('quad'), quadInOut, sineOut ('sineOut'), expoIn ('expo'),
  expoOut, smoothStepIn ('smoothStep'), smoothStepInOut;
  ZoomCamera — linear, INSTANT, quadIn/quadOut, quartIn ('quartIn'),
  circIn ('circ'), expoIn/expoOut, elasticIn ('elastic'), elasticOut,
  elasticInOut; ScrollSpeed — linear, 'cubeInOut' and 'quartInOut' (both end
  in a direction suffix, so the regex clears easeDir and the name is used
  directly), expoIn. Mods may use any name → Oxide implements the
  FULL family with fallback semantics: linear, quad/cube/quart/quint
  (In/Out/InOut), sine, expo, circ, back, bounce, smoothStep, smootherStep,
  elastic — formulas transcribed with citations from FlxEase.hx (Haxe
  `Float` semantics; Oxide keeps f64, matching the M5 policy). Unknown names
  fail the lookup → event no-op (observed at FocusCameraSongEvent.hx L151-156,
  ZoomCameraSongEvent.hx L70-74, ScrollSpeedEvent.hx L83-88).

### 12.4 Engine decisions (Oxide integration)

1. **Trace schema "3" (add-only, M5 §3 rule):** new row kinds applied at the
   dispatch tick, sharing the M5 §2 header exactly
   (`schema:"3"`, `sim`, `input_ms:0.0`, `input_time_us:0.0`, then kind
   fields):
   - `camera_focus` — fields: `target_char` (int), `target_kind`
     ('bf'|'dad'|'gf'|'position'|'unknown' — 'unknown' covers the
     fall-through quirk), `x`, `y` (floats), `world_focus` ([x, y]: the
     character focus point when it exists, otherwise the literal (x, y)),
     `duration` steps, `ease_kind` ('classic'|'instant'|'named'), `ease`
     (resolved function name for 'named', else null), `ease_dir`
     ('In'/'Out'/'InOut'/'' for 'named', null otherwise).
   - `camera_zoom` — `zoom` (float target BEFORE stage/direct multiply),
     `mode` ('stage'|'direct'), `resolved_target_zoom` (float),
     `duration` steps, `ease` (resolved name incl. composite), `ease_dir`.
   - `camera_bop` — `intensity`: resolved `camera_bop_intensity`,
     `hud_intensity` (0.03*intensity), `rate` (float), `offset` (float).
   - `play_animation` — `target` (string), `anim`, `force` (bool),
     `resolved_kind` ('bf'|'dad'|'gf'|'prop:<name>'|'none').
   - `scroll_speed` — per-row ONE strumline either `player`/`opponent`
     ('both' produces TWO rows in player/opponent order), fields
     `target` (resolved absolute value), `absolute` (bool), `base_scroll`
     (current strumline value BEFORE), `duration` steps, `ease`
     (resolved), `ease_dir`.
   - `health_icon` — `char` (0|1), `id`, `should_bop`, `scale`, `flip_x`,
     `is_pixel`, `offsets` [x, y].
   - `unknown_event` — `kind` (string), `value` (raw preserved JSON).
   All rows are emitted in array order (after processing), each exactly
   once per dispatched event (multiple events per tick allowed; fields
   carry no post-event lock-in state, mirroring the 10.2 round-2 rule);
   `schema` stays "3" for ALL these rows. The M5/M6 frozen fixture stream
   formats "1"/"2" stay byte-frozen; new-camera-schema fixtures follow the
   approved establishment path only.
2. **Scene consumption:** the renderer consumes these rows like OpponentHit
   (6A pattern): camera follow point retarget, follow-scroll tween state
   (native camera.target = null during eased follow tweens), camera zoom
   tween, mutable bop state (intensity/rate/offset), per-strumline
   scrollSpeed tween state, PlayAnimation handoff into the 6B animation
   surface, health-icon rows recorded for 6D consumption. Rows absent from
   a stream (all existing fixtures) ⇒ digest-neutral; as with the 6A
   precedent, a digest change pauses for analysis, never silent re-freeze.
3. **Sim event walker:** the sim follows native queryEvents semantics
   (persistent cursor scan, age rule, activate-once), emits rows in the
   same per-tick order as the native dispatch (§12.1), and records ALL
   possible outcomes as rows (no silent skips): a PlayAnimation age-skip
   emits a row with `skipped_old: true` (schema-3 added field) — visible
   accounting beats silence.
4. **Bop-state symmetry:** when a `camera_bop` row applies, rate/offset/
   intensity state replace the frozen constants in the scene gate; the
   frozen `geometry.rs` constants stay for the fixture paths (rowless),
   keeping the M5 digests valid (the §12.2 DEFAULT-ZOOM-OFFSET
    contradiction resolution stays bound to the approval path).
5. **Health icon / EnableMask / sserafim rows are record-only** for 6C.

### 12.5 Validation obligations for 6C

- In-crate tests: per-kind parse+application regressions (defaults, string
  coercion, null-target early return, unknown char quirk, unknown-ease
  early returns, INSTANT paths, stage/direct zoom multiply, bop
  state/intensity algebra, scrollSpeed absolute × chart base and target
  resolution, prevScrollTargets snap-on-next-event semantics via
  two-event sequences), PlayAnimation age-rule skip (synthetic chart with a
  >1000 ms-late event only in a synthetic frame schedule), EnableMask
  no-op, `unknown_event` raw preservation, two-run sha256 equality.
- Corpus prototype: run the 61-id sim with event consumption over the
  explicit assets root: 1. verify row TOTALS equal the parsed corpus event
  counts per kind (FocusCamera 1534, ZoomCamera 1509, SetCameraBop 407,
  PlayAnimation 69, ScrollSpeed 16, SetHealthIcon 16, EnableMask 1,
  sserafim* 106; the per-id tabulation recorded in the session temp file is
  the reference, no further freezing); 2. two-run sha256 equality over ALL
  61 ids / 167 runs; 3. zero row-parse errors, zero unknown record failures.
- Existing invariants: M5/M6 fixture digests byte-identical (the sim +
  scene are additive over rowless streams); quick verify green.
- Native parity: the camera eases/state rows are DERIVED-only (labeled) —
  a native camera-probe round (focused FocusCamera/ZoomCamera/SetCameraBop
  rows via the M5-probe pattern) REMAINS a pending acceptance obligation;
  matching the derived rows does NOT claim native parity.
- Human approval item: the render-side establishment FOR a chart with
  observable camera events (bopeebo has FocusCamera events on the M5
  render-slice path — the phase-2b contract excluded them; replaying with
  rows would change digests) needs the ADR-0020 establishment approval
  together with the school-pixel/school-test fixtures from 11.4; neither is
  pre-authorized.

### 12.6 Amendment (2026-11-08, 6C integration): row-shape resolution + corpus accounting

- ENGINE DECISION (`unknown_event` row fields, resolving the §12.4.1
  self-contradiction): the shared header already owns `kind` (the row-kind
  discriminator), so the schema-3 unknown row uses `event_kind` for the
  SOURCE event name plus `value` = the raw preserved `v` payload (null →
  `null`). Consumed only as: `kind = "unknown_event"` and `event_kind`
  = source name (EnableMask / sserafim* / any no-handler kind).
- ENGINE DECISION (ease endpoint honesty): the transcribed flixel 6.1.2
  formulas (§12.3) are pinned by endpoints INCLUDING the native
  overshoots: expoIn(0) = 2^-10, expoOut(1) = 1 - 2^-10, expoInOut edges at
  ±2^-11, elasticIn(0) = -2^-10 (the `t -= 1` phase lands on a 5.5π sine),
  elasticOut(1) = 1 + 2^-10 (a 4.5π sine); elasticInOut and all other
  families hit 0/1 within 1e-12. No idealized endpoint invariant is used.
- ENGINE DECISION (corpus event-pass accounting): the 6C corpus prototype
  runs a SECOND sim pair (`build_chart_events`); its pass end is
  `max(last note end + 2000 ms tail, last event tick)` because native
  gameplay keeps dispatching song events through the song outro (observed
  trailing events on philly-nice-chart-pico, blammed-chart-erect,
  bopeebo-chart-erect, spookeez-chart-erect). Per (id,difficulty) run the
  schema-3 row totals per kind MUST equal the parsed chart-file `events`
  counts with ScrollSpeed 'both' (the default) contributing one row per
  strumline; the schema-1/2 note-pass end and digests stay byte-frozen.
- OBSERVED correction inside this phase's accounting: the 61 corpus ids
  yield **166 difficulty runs** (the earlier 6A PROGRESS line quoted 167,
  a header-inclusive count); per-kind row totals across all 166 runs are
  recorded in the run output (PlayAnimation rows can NEVER fire more than
  once per event in normal playback — the age rule does not trigger).
- Review-round resolutions (2026-11-08, @oxide-reviewer approve-with-comments):
  1. row-consumer limitation (find 1): the `camera_focus` row alone cannot
     distinguish a native null-character EARLY RETURN from a valid
     character target (both record the kind and a `world_focus`); when the
     6C scene consumption lands it must consult its own stage rigs
     (a missing character = no retarget, per §12.2). Named here so no later
     consumer infers the retarget from `world_focus` alone.
  2. record-only divergence (find 2): `health_icon` rows coerce through the
     observed getString/getInt path, so a string-valued `char: "1"` would
     record an applicable char where the native SetHealthIcon switch
     (direct field access, no coercion) would take the warning/no-op
     branch. The CORPUS always uses int/bool typed fields (observed), so
     the shipped accounting is exact; mod payloads outside the corpus can
     differ (named limitation, not a behavior change).
  3. deduplication note (find 3): the focus-point helpers in
     slice/events.rs intentionally do not reformat render/load.rs yet; the
     shared headless helpers must be factored during the 6C scene
     consumption task (which owns render/) — abstracting now would touch a
     file this task deliberately left alone.
  4. ease transcription verified (validator check): all 37 rust functions
     were diffed VERBATIM against the installed flixel 6.1.2
     FlxEase.hx (quart/quint inOut, smooth/smootherStep, sine, bounce
     constants B1-B6, circ, expo, back double-decrement, elastic
     `t -= 1` / `t -= 0.5` phase with `asin(1/A)`); no deviation.
  5. unknown ScrollSpeed strumline values record ZERO rows (the sim never
     invents a target); regression test
     `unknown_scroll_strumline_value_records_no_row` pins it and the
     corpus per-kind count assertion fails closed on such a chart.

### 12.7 Amendment (2026-11-08, 6C scene consumption): world_focus correction, bop defaults, fixture plan

- **`world_focus` row correction (sim-side fix, resolved before the scene
  integration froze digests):** FocusCameraSongEvent.hx L113-134 adds the
  literal `x`/`y` fields ON TOP of the character focus point
  (`targetX += bfPoint.x`); the §12.4.1 frozen row instead recorded
  "the character focus point when it exists, otherwise the literal (x, y)",
  silently dropping nonzero offsets. Corpus scan: 317 FocusCamera events carry
  nonzero literal x/y (e.g. blammed-chart-pico `{char:1,x:80,y:0}`,
  2hot `{char:2,x:100,y:105}`; senpai/bopeebo use 0 defaults), so the
  corrected row is: `world_focus` = for `target_kind` bf/dad/gf WITH a
  resolvable character `[x + point.0, y + point.1]`, otherwise the literal
  `[x, y]` (also when the character is missing — the native early return
  never computes a target). Row VALUES change; no fixture digests depend on
  schema-3 row content (the gameplay-sim corpus pass asserts per-kind COUNTS
  only, and its three-run sha256 equality compares the sim against itself).
  The sim's camera/zoom/bop rows stay DERIVED-only (§12.5); the scene applies
  rig-computed targets per the §12.6 finding-1 rule and never trusts
  `world_focus` numerically.
- **Scene camera consumption (integration decision under the frozen §12.2
  semantics):** the scene owns a private `CameraEventState`: follow point
  (initialized to the opponent focus; retargeted per row), follow tween +
  follow-enabled flag (native `camera.target = null` while a follow tween is
  active), zoom tween on `currentCameraZoom`, mutable bop config
  `(intensity, rate, offset)` + virtual HUD zoom (starts
  `defaultHUDCameraZoom = FlxCamera.defaultZoom * 1.0`; the native decay block
  `camHUD.zoom = lerp(default, hud, 0.95^dt)` feeds ONLY the gate
  `Preferences.zoomCamera && hud < 1.35 * default`), and per-strumline
  scrollSpeed tweens with the native `prevScrollTargets` snap order. Per tick
  the state advances in native order: tweens (super.update position) →
  conductor (bop gate may pulse) → decay/zoom-assign block gated
  `cameraZoomRate > 0` → rows (processSongEvents position) → follow lerp at
  end-of-step. FocusCamera target resolution CONSULTS THE SCENE'S OWN RIGS
  (§12.6 finding 1): a bf/dad/gf row without that character applies NOTHING;
  the target is `rig.focus + (x, y)`; 'position'/'unknown' rows target the
  literal `(x, y)`. CLASSIC = cancel follow tween + follow active, retarget,
  no snap; INSTANT = snap scroll to `target - (640, 360)` + follow active;
  named ease with a valid resolution = follow disabled + eased scroll tween
  to `target - (640, 360)` with onComplete = follow active + snap to the same
  point; an unresolved ease row (`ease = null` fields) applies NOTHING.
  ZoomCamera consumes `resolved_target_zoom` (INSTANT sets `currentCameraZoom`
  directly; a valid ease tweens it; an unresolved ease applies nothing);
  scroll_speed rows reproduce the sim's cancel/snap order and the scene
  pre-assignment value must match the row's `base_scroll` (fail closed);
  `play_animation` rows consult the scene rigs via `resolved_kind` and apply
  `playAnimation(anim, force, force)` semantics — restart = force, and
  `ignoreOther` = force locks the character against other animations (native
  `Bopper.canPlayOtherAnims`, reopened by onAnimationFinished) until the
  fired animation finishes; `skipped_old: true` rows, prop targets resolving
  to nothing, and health_icon/unknown rows apply nothing (record-only,
  §12.4.5).
- **Bop-state defaults become the OBSERVED native constants this phase
  (approved change, §12.2 contradiction resolution now active):** with the
  fixture establishments below human-approved, the scene camera bop config
  defaults ARE the on-disk source values (`Constants.DEFAULT_BOP_INTENSITY =
  1.015`, `DEFAULT_ZOOM_RATE = 4`, `DEFAULT_ZOOM_OFFSET = 0` — zero beats,
  on-beat, Constants.hx L246/L251): `geometry.rs` `CAMERA_ZOOM_RATE_OFFSET`
  moves 1 → 0 and the bop gate/decay block adopt the mutable config. The
  rowless render-slice digests are re-established together with the schema-3
  establishments in this same approved step; in-crate bop tests update with
  the observed-source comments. `Preferences.zoomCamera` defaults true ([A]
  default save options).
- **Fixture establishments (human-approved 2026-11-08; ADR-0020 path):** the
  render-diff gate is generalized per chart-id family: (1) the bopeebo
  easy/normal/hard entries are RE-ESTABLISHED on the schema-3 event-driven
  canonical stream (event rows consumed by the scene; the old rowless
  `render-slice-1` digests are superseded — git history keeps the old
  record), and (2) a NEW `school` family on the `senpai` chart id exercises
  the pixel note style + school stage + repeated CLASSIC focus rows (26
  FocusCamera events with alternating char 1/0). Same gate rules: two-run
  sha256 equality, the frozen sample-plan recipe per family, fail-closed
  manifest comparison, never regenerated during verification. The manifest
  schema token moves to `render-slice-2` for both families (rows enabled).
  The honesty wall stands: engine fixtures on the pinned llvmpipe adapter,
  NOT native pixel parity; the native rendered-output trace remains the
  separate pending obligation.
- **2026-11-09 review round (APPROVE WITH COMMENTS) resolutions:** (1) the
  post-GameOver decay follows the native `subState == null` gate — the scene
  freezes both the bop decay and the virtual HUD zoom after a GameOver row
  (scene test `game_over_row_freezes_the_decay_and_hud_zoom_block`);
  (2) the §12.6 finding-3 headless-helper factoring is DEFERRED to a later
  slice (it spans `slice/events.rs` focus/asset resolution and `render/load.rs`
  internals wide enough to deserve its own session) with an in-crate
  asset-gated equivalence gate instead:
  `sim_and_scene_agree_on_the_sampled_focus_points` asserts every resolvable
  `camera_focus.world_focus` row equals the scene rig target + the literal
  offsets for bopeebo/senpai; (3) the only corpus chart with ScrollSpeed rows
  (south-chart-pico) replays through the event-enabled stream and the real
  `SliceRenderer` asserting zero `ScrollBaseMismatch` (xtask test
  `south_pico_scroll_speed_rows_replay_through_the_real_scene`); the variant-
  chart provenance metadata-path derivation (`-chart-pico.json`) was fixed in
  the process.

### 12.8 Amendment (2026-11-09, playtest-verified animation parity fixes)

- **OBSERVED** (`PlayState.hx` L2838-2852, inside `processNotes`): on every
  frame while any opponent hold note has `hitNote && !missedNote &&
  sustainLength > 0`, if `getDad() != null && getDad().isSinging()`, then
  `getDad().holdTimer = 0` — the opponent's sing animation survives long
  sustains (playtest: dad stepped off mid-hold on the bopeebo easy
  t=52800/d=7/l=1875 ms note around 54000-54675 ms). Scene wiring: the reset
  runs after the opponent hold prune and before the per-character scheduler
  updates (`CharacterScheduler::reset_hold_timer_under_opponent_hold` keeps
  the native `isSinging()` guard).
- **OBSERVED** (`BaseCharacter.hx` L174 + `Bopper.hx` L261-282):
  `BaseCharacter.new` sets `ignoreExclusionPref = ['sing']`; a locked
  Bopper's `playAnimation` early return is BYPASSED for any animation whose
  name starts with a listed entry (`sing{DIR}` and `sing{DIR}miss` both
  play), while `ignoreOther = force` keeps the lock closed and a restart of
  a non-current non-sing animation stays dropped. Consequently a note-hit
  sing DOES play through a forced chart animation (playtest: `hey`
  force:true blocked `sing*` on bopeebo). The lock reopens when the
  currently-playing animation finishes regardless of which animation owns
  it when it finishes.
- **Fixtures (approved ADR-0020 path, 2nd establishment pass):** sample
  lists unchanged in all six captures; every frame/run/trace digest
  refreshed (both fixes change animation state inside sampled windows;
  school normal/hard still share one run digest). The 2026-11-09 fixture
  freeze reasons are recorded in the manifest honesty line; in-crate tests
  pin both fixes (playAnimation sing-bypass + opponent hold reset).

## 13. Phase 6D behavior freeze — play HUD core (health bar, health icons, camHUD)

Analyst round read the installed `$HOME/Descargas/fnf/Funkin` checkout read-only
(2026-11-09; native evidence first). The M5 playtest HUD (`crates/oxide-game/src/
play/hud.rs`, ADR-0021 decision 4) already froze the health-bar PLACEMENT and the
icon swap placement algebra; 6D freezes the remaining items and consumes the
frozen §12.4.1 `health_icon` rows.

### 13.1 Observed facts (with native citations)

- **HUD camera** (`PlayState.hx` L1186-1228 + Observation §12.2): all HUD
  elements (`healthBar`, `healthBarBG`, `scoreText`, `iconP1/2`, popups) use
  `cameras = [camHUD]` (L2014-2016, L2160, L2195); `camHUD` is a separate
  1280x720 (game-size) camera with `bgColor.alpha = 0`, initial zoom = 1.0
  (`camHUD = new FlxCamera()` create; `defaultHUDCameraZoom =
  FlxCamera.defaultZoom * 1.0` L337). The bump/decay algebra already frozen in
  §12.2 applies to `camHUD.zoom` exactly; the 6D scene applies the REAL per-tick
  `camHUD.zoom` to the HUD layer composition (the §12.7 virtual gate value
  becomes the real zoom).
- **Health bar** (PlayState.hx L810-811, L1452, L1990-2016, L2766-2772):
  - `healthBarBG = FunkinSprite.create(0, 0, 'healthBar')` — asset
    `shared/images/healthBar.png` (dimensions in the image file); placement per
    frame in PlayState (not just init — L2000-2004 is inside the create call
    block but `updatePosition` per-frame equivalents run at init; revisit: the
    M5 hud.rs placement is FROZEN and carries this).
  - `healthBar = FlxBar(0, 0, RIGHT_TO_LEFT, bg.width-8, bg.height-8, min 0,
    max 2)`; `x = bg.x+4`, `y = bg.y+4`; `createFilledBar(COLOR_HEALTH_BAR_RED,
    COLOR_HEALTH_BAR_GREEN)` (no border; Constants L?? red/green pair; the M5
    placement tests froze the same colors); `zIndex` 801 vs bg 800.
  - `healthLerp = FlxMath.lerp(healthLerp, health, 0.15)` (fixed factor 0.15,
    L2772) and `healthBar.value = healthLerp` (L1452) every frame.
  - Fill algebra (flixel 6.1.2 `FlxBar.hx` L757-830): fraction =
    (value - min)/range; width = floor(fraction * barWidth / (barWidth/100))*barWidth/100
    (= floor(fraction*100) percent slices at `numDivisions = 100` default L88),
    Std.int quantization; RIGHT_TO_LEFT anchors the fill rect at
    `x = barWidth - fillWidth` (so the FILLED (green) side grows leftward from
    the right edge at initial 1.0 health the two sides split at the health
    fraction).
- **Health icons** (HealthIcon.hx full file + BaseCharacter.hx L174/L363-387 +
  PlayState.hx L2155-2196):
  - Built in create: `iconP2 = new HealthIcon('dad', 1)` then `dad.initHealthIcon(true)`;
    `iconP1 = new HealthIcon('bf', 0)` then `boyfriend.initHealthIcon(false)`;
    both zIndex 850, on camHUD, `y = healthBar.y - height/2` (recomputed
    per-frame by `updatePosition` L896-935).
  - `initHealthIcon(isOpponent)`: `iconP1.configure(_data?.healthIcon)` PLUS
    `iconP1.flipX = !iconP1.flipX` (player faces the other way; L376);
    `iconP2.configure(_data?.healthIcon)` with NO additional flip.
  - `configure(data)` (null → 'face' default, isPixel false, size (1,1),
    iconOffset (0,0), flipX false, updatePosition; data → id, isPixel,
    shouldBop, loadCharacter, size = scale both axes, iconOffset = offsets or
    (0,0), flipX = data.flipX ?? false, updatePosition).
  - `loadCharacter`: missing `icons/icon-<id>` image → WARN + 'face';
    `isLegacyStyle = !exists('icons/icon-<id>.xml')`; legacy =
    loadGraphic 32/32-px frames (isPixel ? 32 : 150) with anims
    Idle [0]/Losing [1]/(Winning [2] iff ≥3 frames) at fps 0; new =
    loadSparrow + addByPrefix idle/winning/losing (24 fps looped) +
    toWinning/toLosing/fromWinning/fromLosing (looped false); antialiasing =
    !isPixel. `loadCharacter` RELOADS on every configure.
  - Per-frame `update` (L896-935): `angle = smoothLerpPrecision(angle, 0,
    elapsed, HEALTH_ICON_ANGLE_HALF_LIFE)` (bopAngle defaults 0, so the shipped
    sprites never rotate [D] — the angle machinery is accepted and inert), then
    `updatePosition()`.
  - `updatePosition` (L896-935): use health (p0: `health`; p1:
    `MAXIMUM_HEALTH - health` = `2 - health`) and the bar geometry X
    and the frozen M5 algebra + `POSITION_OFFSET = 26`;
    `y = healthBar.y - height/2`.
  - `updateHealthIcon(health)` state machine (L951-1023): thresholds
    `WINNING = 1.6`, `LOSING = 0.4`; idle → toLosing(toWinning) →
    losing/winning → fromLosing/fromWinning → idle; unknown/'' → idle always
    (no restart when already valid: `playAnimation(name, fallback)` with the
    same name is a no-op because animation.play requires no restart).
  - `onStepHit(curStep)` (L941-970): `bopEvery = STEPS_PER_BEAT = 4`, step %
    4 == 0 && shouldBop → cancel tween, `setGraphicSize(width + 150*size.x*0.2,
    0)`, tween back to `150*size.x` over `min(stepLengthMs * 0.002, .175)`
    seconds with `updateHitbox()` + `updatePosition()` on every tween tick
    (FlxTween.num); `angle += bopAngle * (p0 ? +1 : -1)` (0 on the corpus).
  - `snapToTargetSize` (L918-930): `width > height → setGraphicSize(150*size.x,
    0) else (0, 150*size.y)`; `updateHitbox()` after each size change; `offset
    += iconOffset` inside `updateHitbox` (L935).
  - Player-icon XOR flip (PlayState init L2201 + hud.rs M5): the M5 frozen
    sprite mirror (`flipX = !getDataFlipX()` for load.rs L931) already matches
    this path for the STAGE sprites; the ICON mirror is separate (iconP1).
- **SetHealthIcon rows** (event consumption; SetHealthIconSongEvent.hx L37-90):
  `offsets = [offsetX ?? 0, offsetY ?? 0]`, id ?? 'face', shouldBop ?? true,
  scale ?? 1.0, flipX ?? false, isPixel ?? false; `switch (data?.value?.char ??
  0)`: 0 → iconP1.configure(healthIconData) (NO flip surgery — the event sets
  flipX directly), 1 → iconP2.configure(healthIconData), other → warning
  no-op. NOTE (matches the M6-CONTRACT §12.6 finding 2 direct-field access):
  the native `char` uses DIRECT value access (no getInt coercion); the sim row
  coerces — the corpus uses int-typed fields so the accounting is exact
  (named limitation for mod payloads).
- **Score text** (L2028-2038 init): `scoreText.x = healthBarBG.x + bg.width-190`,
  `y = bg.y + 30`, RIGHT-aligned with black outline, `letterSpacing = -1`;
  per-frame `updateScoreText` (L2748-2760): `'Score: ' + formatMoney(songScore,
  false, true)` (comma-separated integer with NO decimals; bot play →
  'Bot Play Enabled'). Exception: font typography (Angel VCR font stack) is
  NOT frozen in 6D (engine decision — a bare font surface landed in M4; the
  score text render consumes it in a follow-up, named below).
- **Icon assets inventory (observed over the shipped corpus
  `preload/images/icons/`)**: 23 PNG files, ALL legacy (no `icon-*.xml` files
  shipped), including pixel icon PNGs (icon-bf-pixel, icon-pico-pixel with 32
  px half-size frames? to verify at implementation from the actual file sizes)
  and the sserafim set (chaewon/eunchae/kazuha/sakura/yunjin).

### 13.2 Engine decisions (Oxide integration)

1. **HUD composition:** the Scene gains screen-space HUD layers appended AFTER
   all stage/strumline layers (scrollFactor (0,0) equivalent), with the REAL
   camHUD zoom applied per tick (`camHUD.zoom` from the §12.7 state).
   camHUD is 1280x720, so HUD geometry == game geometry (no window scale in
   the slice).
2. **HUD elements in the 6D slice:** health bar bg + filled bar (FlxBar
   algebra, numDivisions 100 quantization), both health icons (legacy +
   new-style sparrow pipeline, state machine, configure, per-bop tween),
   camHUD zoom; score text CONTENT is frozen but rendering is DEFERRED
   (font surface already exists; the typography render is a named 6D
   follow-up item so the pixel freeze of the HUD core stays reviewable).
3. **`SetHealthIcon` row consumption:** rows apply `configure semantics`
   (id/isPixel/shouldBop/scale/flipX/offsets) to iconP1/iconP2 per the char
   mapping; `char` outside {0,1} = no-op; the icon state machine restarts on
   the configured id change (loadCharacter reload).
4. **healthLerp** drives the bar fill AND both icon X positions (native);
   the play mode hud.rs consumes the same algebra from the sim's health.
5. **Scope exclusions (named):** judgement pops, combo pops, note splashes,
   hold covers, countdown display, ring date text — later slices; bf's
   death icon variation is NOT part of the 6D icon (use the plain
   configured icon; game-over HUD behavior is a death-SubState item).

### 13.3 Validation obligations for 6D

- In-crate tests: icon state-machine transition table (1.6/0.4 thresholds and
  the winning/losing clamp at exactly the boundaries), configure defaults +
  reload semantics, legacy vs new sparrow pipeline selection (xml existence),
  pixel icon 32 px legacy + NEAREST, bop tween endpoint values
  (min(stepLengthMs*0.002, 0.175)), updatePosition X for both players at
  health 0/1/2 (POSITION_OFFSET 26), FlxBar fill quantization at 0/1/2 and
  slice boundaries, healthLerp 0.15 update, camHUD zoom application to HUD
  layer composition, SetHealthIcon row consumption incl. unknown char no-op.
- Corpus: the M6 play-mode HUD (`play/hud.rs`) renders the SAME algebra; run
  the bopeebo `play`-mode with SetHealthIcon active (visual HR arbiter).
- Render-diff: HUD pixels enter the frozen frames — the render-slice-2
  fixtures MUST be re-established a THIRD time on the ADR-0020 path with the
  human approval recorded BEFORE any digest freeze (the existing approved
  establishments covered the event-driven scene without HUD). Sample-plan
  recipe unchanged (hud adds no classes).
- Named pending: native rendered-output trace; score text typography.

### 13.4 Amendment (2026-11-09, 6D integration review round)

- **Score text is KEPT in the 6D slice (human directive resolved the
  deferral):** the implementation includes the vcr-bmp Angel-font score text
  with the 8-offset OUTLINE border. The native frame-head ordering is
  preserved: `updateScoreText` runs before `processSongEvents`
  (PlayState.hx L1035-1038), so the visible string is the PRE-ROW snapshot
  (HudState.score_snapshot at begin_tick; a same-tick row becomes visible
  one frame later). The auto-sized single-line field makes RIGHT alignment
  a no-op (frozen; FlxBitmapText L1005-1160).
- **The icon state machine consumes the native frame-head-clamped health**
  (PlayState.hx L1208-1210 clamps before any sprite update, so clamped ==
  native input; the earlier "raw health" comment was wrong and is amended).
- **New-style Sparrow icon pipeline (review finding 1):** `icon-<id>.xml`
  sibling selection (native `isNewSpritesheet`) selects the Sparrow sheet
  with the seven `loadAnimationNew` clips (addByPrefix startswith, 24 fps,
  looped idle/winning/losing + one-shot to*/from*); rotated pages fail
  closed ([D]). The loader cannot list directories (Vfs has no listing
  API), so sibling XMLs are probed for the measured corpus inventory ids
  only; mod-only icon ids fail closed with the recorded limitation.
- **GameOver divergence documented (review finding 3):** the HUD freezes the
  last display value while `ScoreTracker` reports 0.0 at death; on the true
  native death path health is already exactly 0.0 so the two agree; a
  differential test pins both rule sets.
- **Fixtures:** the third human-approved establishment pass was refreshed a
  FOURTH time after these fixes (same ADR-0020 authority; sample lists,
  trace digests and geometry unchanged; all frame/run digests refreshed on
  the pinned llvmpipe adapter; school normal/hard still share one run
  digest with distinct trace digests).

### 13.5 Amendment (2026-11-09, playtest-verified 1-frame icon parity fix)

- **OBSERVED** (flixel 6.2.1 `FlxAnimationController.hx` L247-270):
  `add(..., indices, ...)` DISCARDS indices >= numFrames; on a 1-frame
  legacy icon (icon-dad/gf/parents, 150x150) the `loadAnimationOld`
  `losing` index 1 is dropped, so `playAnimation(toLosing, losing)` finds
  neither clip and the icon stays on `idle` (frame 0). The previous
  implementation registered `losing` frame 1 unconditionally, so the icon
  selected an out-of-range frame and DISAPPEARED from the screen. Fix:
  `IconSheet::legacy` registers `losing` only when `frames.len() >= 2`
  (mirroring the `winning >= 3` guard), and `HealthIconState::quad`
  degrades a hypothetical out-of-range clip index to frame 0 instead of
  drawing nothing (defensive parity, the native controller can never
  select such an index). Unit test `single_frame_icon_stays_idle_and_is_
  drawn_under_losing_health` pins the 1-frame behavior.
- **Fixtures:** the approved render-slice-2 establishment was refreshed
  (same ADR-0020 authority; sample lists/trace digests/geometry unchanged;
  frame/run digests updated). `render-diff` PASS, quick `verify` exit 0.

### 13.6 Amendment (2026-11-10, playtest-verified max-health bar fill parity fix)

- **OBSERVED / DERIVED (`ec3e00a`):** in 64-bit IEEE-754 arithmetic,
  `healthLerp = lerp(healthLerp, 2.0, 0.15)` stagnates at
  `1.9999999999999993` (`2.0 - 3 * 2^-52`) because `(2.0 - healthLerp) * 0.15 <
  0.5 * 2^-52`, which caused `flx_bar_fill_width` (`numDivisions = 100`
  `floor` quantization) to truncate `(healthLerp / 2.0) * 100.0 =
  99.99999999999997` to `99` divisions (`587.0` px instead of `593.0` px),
  leaving a permanent 6-pixel red sliver on the left edge of the health bar
  at max health. Fix (`crates/oxide-game/src/render/hud.rs`): `HudState::begin_tick`
  snaps `health_lerp = health` when `(health - health_lerp).abs() < 1e-4`, and
  `flx_bar_fill_width` treats `(HEALTH_MAX - value).abs() < 1e-7` as fraction
  `1.0`. Unit tests pin both convergence and direct `flx_bar_fill_width` full
  fill.
- **Fixtures:** the approved `render-slice-2` manifest (`tests/fixtures/render-slice/_manifest.json`)
  was refreshed on the same ADR-0020 authority for the 29 max-health frames
  and 6 run digests affected by the 100% green bar fill (sample lists, trace
  digests, and geometry unchanged). `cargo xtask render-diff` PASS (exit 0).

## 14. Phase 6E behavior freeze — week-6 in-game dialogue + weekend-1 choreography + school presentation

Analyst round read the installed `$HOME/Descargas/fnf/Funkin` source + assets
read-only (2026-11-09). SCOPE BOUNDARY (per the human phase directive): the
in-game dialogue box flow and gameplay choreography are 6E deliverables; the
meta/UI/save side of dialogue replay and the video cutscene PLAYER belong to
the meta/media phases (GOAL keeps "video/subtitles" as a release item —
separately scheduled); freeplay/story difficulty filtering and the save
mechanics the scripts consult are M7 surfaces.

### 14.1 OBSERVED facts (native; labels OBS/DER where hybrid)

- **Trigger chain (OBS).** The shipped content scripts
  `preload/scripts/songs/{senpai,roses,thorns,senpai-pico,roses-pico,…}.hxc`
  (Polymod-loaded Haxe script classes, DATA for this engine) hook
  `onCountdownStart`: on the story-mode FIRST entry
  (`PlayStatePlaylist.isStoryMode` + `hasPlayedCutscene == false`) they
  `event.cancel()` — native `Countdown.performCountdown` returns false
  immediately when the first event propagates cancelled
  (Countdown.hx L44-54), so NO countdown runs — then launch the dialogue:
  senpai: black solid (1.5x viewport, `camCutscene`, zIndex 1000000) for
  0.25 s + 2.0 s linear `FlxTween.num(0..1)` (stepped-composite tween body)
  before `startConversation('senpai')`; roses/thorns: immediate
  `startConversation(target)` with `target += "-censored"` when
  `Preferences.naughtyness` is false (default TRUE, Preferences.hx L74-82)
  preceded by `ANGRY_TEXT_BOX` SFX. Weekend-1 darnell (first entry): VIDEO
  branch `VideoCutscene.play('darnellCutscene[-censored]')` at camera zoom
  1.3 direct (CENSOR_EXPLETIVES compile flag default false,
  Constants.hx L660); SECOND story entry (`hasPlayedCutscene &&
  !hasPlayedInGameCutscene`): the in-game can choreography below; freeplay:
  neither. `onSongRetry` sets both flags true (no repeats per session);
  `onCreate` resets them.
- **In-game dialogue pipeline (OBS, Conversation.hx full state machine).**
  `PlayState.startConversation(id)`: `isInCutscene = true`; the
  `Conversation` (registry entry `data/dialogue/conversations/<id>.json`) is
  put on a dedicated `camCutscene` camera (translucent bg over the game,
  PlayState.hx L1961/L1971) at zIndex 1000. State machine
  Start → (DIALOGUE_START) Opening → (box anim finished) Speaking → (typing
  complete) Idle → (advance) DIALOGUE_LINE → next entry/line, … →
  (count exhausted) DIALOGUE_END → Ending → (outro tween complete) destroy +
  `completeCallback` → `onConversationComplete` (isInCutscene = false).
  `advanceConversation()` on `controls.CUTSCENE_ADVANCE` — default keyboard
  binding `[Z, ENTER]` (Controls.hx L1014 area) — dispatched under
  `isInCutscene && !disableKeys` (PlayState.hx L1329: during dialogue the
  scripts set `disableKeys` false/true around it); cancellation surfaces
  (`eventCanceled`) skip the transition. Playback per entry: speaker swap
  (`showCurrentSpeaker`, skip when same id; kill/revive), box swap
  similarly; `setText` (first line of an entry) vs `appendText` (subsequent
  lines of the SAME entry; the prefix is the already-typed text); per-entry
  `boxAnimation`/`speakerAnimation` play through the observed `correct
  AnimationName` suffix-strip→'idle' fallbacks (DialogueBox/Speaker
  classes). Outro: data `outro: {type: fade, fadeTime}` → episode alpha
  tween with `EaseUtil.stepped(8)` + music volume fade; `PauseSubState`
  offers "Restart Dialogue" (`resetConversation`) while cutscene-paused.
- **Dialogue data (OBS inventory).** conversations: senpai 3 entries/3
  lines, roses (+censored 4/4, pico 7/7, pico-censored 7/7), thorns 4/6,
  senpai-pico 17/19 (the nene "bwuh" gag switches music off/on per speaker —
  senpai-pico scripted override). boxes: default (speech_bubble_talking,
  isPixel, scale 5.4, offsets [-660,-340], text width 900, color #3F2021,
  shadow #D89494 w2), roses (dialogueBox-new, width 800, anims
  enter/speaking/sentenceEnd/idle/click/exit/angrySenpai), thorns
  (dialogueBox-evilNew, width 900, WHITE text, transparent shadow, width 0).
  speakers: 9 files, ALL `weeb/portrait-*`, scale 5.4, isPixel TRUE,
  flipX false, each `talkEnter`/`talk`. Text font family is
  `Pixel Arial 11 Bold` = the embedded `assets/fonts/pixel.otf`
  (family "Pixel Arial 11"/Bold — PIL-verified), box text at
  `(offsets[0],offsets[1])`, fontSize 32, LEFT-aligned with the SHADOW
  border style at `shadowWidth` (FunkinTypeText writes per flixel-added
  shadow pass). Typing sound `FunkinSound.load('pixelText', 0.6)` per typed
  char batch. Typewriter cadence (OBS, FlxTypeText.hx: `delay` default
  0.05 s/char = 20 cps; per frame `_timer += elapsed`; when `_timer >=
  delay`: `_length += Std.int(_timer/delay)` then `_timer %= delay`;
  `skip()` completes instantly; complete callback → `Idle` state).
  The scripts override box animations: roses box plays `sentenceEnd` on
  typing complete, `click→speaking` on lines (its scripted subclass),
  `exit` reversed on dialogue end.
- **Weekend-1 in-game choreography (OBS; darnell.hxc introCutscene + 2hot.hxc
  note-kind systems).** darnell second-entry chain (cutsceneDelay = 2.0 s,
  timers truncated to exact values; camera tweens = the frozen 6C algebra
  `tweenCameraToPosition/tweenCameraZoom`, direct mode): 0.1 s camera to
  pico focus +250 x / zoom 1.3 (direct) quadInOut; 0.7 s cutscene music
  `darnellCanCutscene/darnellCanCutscene` starts + 2 s alpha fade of the
  black overlay (startDelay 0.3); 2.0 s camera to darnell focus +100 x /
  zoom 0.66 over 2.5 s; 5.0 s dad `lightCan` + Darnell_Lighter SFX;
  6.0 s player `cock` + Gun_Prep + camera +180 x over 0.4 s backOut;
  6.4 s dad `kickCan` + Kick_Can_UP + the can prop plays `up` (can sprite
  created at `spraycanPile.x+30, y-320`, zIndex pile-1, `wked1_cutscene_1_
  can` sparrow with `can kick quick0`/`can kicked up0` prefixes);
  6.9 s dad `kneeCan` + Kick_Can_FORWARD + can `forward`; 7.1 s player
  `intro2` (force restart + ignoreOther semantics — the 6C scheduler
  semantics) + random-shot SFX + camera +100 x over 1 s + the SpraycanAtlas
  prop at pile.x-10, y-550 plays `Can Shot` (its frame callback spawns a
  `SpraypaintExplosion` `Explosion 1 movie0` sprite at +150/-250 px and the
  1/24 s later `darkenStageProps()` pass — every stage prop EXCEPT
  bf/dad/gf and the picoFade zIndex term gets `color = 0xFF111111`, then
  0xFF222222 + `FlxTween.color` back to 0xFFFFFFFF over 1.4 s [the engine
  currently renders no prop TINT — an engine addition]); 7.9 s dad
  `laughCutscene`; 8.2 s gf `laughCutscene`; 10.0 s camera zoom 0.77 +
  camera +180 x over 2 s sineInOut, `startCountdown()`, music stop, pause
  unlocks. A dedicated cutscene Conductor (own metadata
  `darnellCanCutscene/-metadata.json`) drives a per-beat dance-if-finished
  rule on dad/gf (dance(true) whenever the animation finished, never while
  `lightCan` current for the dad).
- **2hot note kinds (OBS).** The chart notes carry data kinds
  `weekend-1-lightcan/kickcan/kneecan/cockgun/firegun[/firegun-hip/
  firegun-far]` (hard: 18 each). Engine decision downstream: kinds must
  reach gameplay; behavior — kickcan spawns a `SpraycanAtlasSprite` (own
  animate-prop state machine ARCING 2/SHOT 3/IMPACTED 4, `Can Start` →
  `Hit Pico` → explosion EZ at `+750/-100`; `Can Shot` → kill at the
  explosion spawn 150/-250 note frame 3) at pile.x-10/y-550 zIndex pile-1;
  firegun with `gunCocked` (set by cockgun for 1 s) fires the ARcing can
  (STATE_SHOT + darken pass again); un-acked firegun notes are CANCELED
  (event.cancelEvent — no score/health side effects); MISS of
  firegun/hip/far: sets `gunCocked=false`, no base health loss
  (event.healthChange = 0.0), decrement can → `takeCanDamage()`:
  `health -= 0.25*2 = 0.5` (!) — a missed can costs half a health bar and
  kills at <= 0 with the pico-explode variant suffixes (game-over
  variations are a GOAL-covered later slice). Cockgun MISS also zeroes the
  health change. 2hot story flow: first entry plays the ENDING video
  cutscene (`2hotCutscene.mp4` — video PLAYER [deferred]) after the song;
  retry path re-enables a NORMAL song end; nene `danceEvery = 0` during
  the cutscene + `hasDanced=false; dance(true)` resync; camera tweens to
  (1539, 833.5) zoom 0.69 after the end. lit-up ships no choreography.
- **School/pixel presentation (OBS).** The dialogue reused the week-6
  pixel rules (6B): isPixel sheets → NEAREST pages at scale 5.4;
  speaker/box sprites sit fullscreen (screenCenter + offsets); the pixel
  text uses the embedded pixel.otf design at 32 px with the SHADOW style.
  No pixel-arial bitmap pre-page ships — text glyphs raster at runtime.

### 14.2 Engine decisions (Oxide integration; [D])

1. **The shipped `.hxc` song/character/stage scripts are CONTENT, not
   engine surface:** their observed per-song behavior is reimplemented as
   Rust content modules keyed by song id (a private
   `play/choreography/` table consulted at the song-start/countdown/note
   boundaries). The Rhai script surface (M8) will call the same
   engine-internal hooks; nothing in the engine interprets Haxe/C
   scripts at runtime.
2. **Dialogue engine:** a private conversation module consuming the
   observed JSON surface (conversations + boxes + speakers via the M1
   data registry pattern, fail-closed); the typewriter/state machine/
   advance rules transcribed 1:1 from the cited classes; the input uses
   the existing ADR-0021 action surface (CUTSCENE_ADVANCE [Z, ENTER]) and
   the PauseSubState 'Restart Dialogue' path.
3. **Text raster:** a runtime glyph rasterizer for pixel.otf (and later
   shared font families) is required — dependency approval item (pure
   Rust, MIT/Apache, e.g. `ab_glyph` or `fontdue`) BEFORE 6E
   implementation; the engine bakes glyphs per (font,size,shadow) into
   pages at load. No other new dependency is anticipated.
4. **Prop tint chain** (darken/blacken passes): scene stage items gain a
   static color-multiply quad parameter (the frozen M6 frozen algebra
   otherwise unchanged).
5. **Video cutscenes (darnell first-entry video, 2hot ending video, and
   the weekly story videos) are DEFERRED to the media encoder phase** with
   the engine decision recorded there (the 6E choreography implements the
   story RETRY in-game paths where the video would not run and in-game
   branches; the video branch is named in the code and treated as
   played-on-first-entry for accounting).
6. **Cutscene camera:** the camCutscene compositional layer renders above
   the HUD (viewport equivalent, bgColor transparent over the game);
   disableKeys + isInCutscene gate inputs just like native
   (PlayState.hx L1238/L1328-1332 order pinned).

### 14.3 Validation obligations for 6E (before implementation)

- In-crate tests: conversation state-machine walk-through (entry/line
  transitions incl. appendText vs setText boundaries), typewriter cadence
  (20 cps batch accumulation `_timer %= delay` and skip-consistency),
  advance/skip/restart paths (advance while Opening/Speaking/Idle; the
  censored variantalkers), speaker/box swap logic (same id skips), the
  suffix-strip→idle animation fallbacks, darnell timer-chain camera
  targets with the 6C tween algebra, the darnell per-beat dance-if-finished
  custom-conductor rule, 2hot kind table (kickcan spawn geometry, gunCocked
  1 s window, firegun cancel path, takeCanDamage -0.5 + death gate),
  and the prop-tint pass (affected exclude lists: bf/dad/gf, picoFade
  zIndex, cans).
- Corpus/artifact checks: the shipped 9 conversations parse with per-entry
  speaker/box/animation resolution counts (senpai-pico's 17 entries and
  the 9 speaker files); 2hot kinds inventory (18x5 on hard + per-difficulty
  counts from the real charts); the darnellCanCutscene metadata conducts
  on the native beat boundary.
- Visual/reference: dialogue render snapshots (llvmpipe) freeze per-line
  frames of the three shipped conversations under a NEW render fixture
  family (ADR-0020 path; separate approval item) — the conversational
  frames include the pixel.otf raster so the fixture lock pins the text
  raster pipeline.
- Explicit NON-goals for 6E: video playback itself (player owned by the
  media phase), save-progress gating (`Save.hasBeaten*` filters), any
  Chart Editor debug path; game-over scripted suffix variants.
- Approval items: (a) the glyph rasterizer dependency; (b) the new
  dialogue render fixture family third/fourth pass approval (like the 6D
  HUD establishment); (c) D5-gameover explosion-suffix variants stay out
  of 6E (game-over surface is its own slice).

### 14.4 Amendment (2026-11-09, 6E integration): implementation record + fixture establishment

- **Delivered (commits 1973679..e4a9cd5 + the integration pass):**
  - `oxide-data`: dialogue schemas (`conversations/*.json`, `dialogue/boxes/*.json`,
    `dialogue/speakers/*.json`) with a dedicated `DialogueKind` taxonomy
    (the global `DataKind`/`Typed` extension is a recorded follow-up: adding
    variants shifts the parse-all corpus inventory counts), fail-closed
    `#RRGGBB[AA]` colors, unknown keys retained, round-trip + corpus tests.
    CORRECTION to §14.1: the shipped checkout carries **7 conversations**
    (roses, roses-censored, roses-pico, roses-pico-censored, senpai,
    senpai-pico, thorns), not 9; 3 boxes, 9 speakers as frozen.
  - `oxide-game` trace surface: `InputEvent::Advance` and the schema-2/3
    `note_kind` field are ADD-ONLY-IF-PRESENT (kind-free streams stay
    byte-identical; the frozen bopeebo/school digests are structurally
    unchanged — pinned by the gated render tests).
  - Conversation engine: the five-state machine, the exact FlxTypeText
    batch rule (`delay 0.05`, `_timer %= delay`, `skip()` instant), the
    per-entry `setText`/`appendText` boundary, the suffix-strip→idle
    animation fallbacks, the outro `stepped(8)` alpha + [D] virtual music
    volume, the camCutscene layer group after the HUD group
    (backdrop 10 < speaker 200 < box 300, identity projection), the
    ab_glyph pixel.otf raster (deterministic pages, shadow pass first,
    letterSpacing −1, greedy word wrap) and the native onCreate kick
    (DIALOGUE_START fires on the trigger tick; the first advance only
    skips the typing).
  - Weekend-1: the 2hot note-kind rules in the SIM (cockgun 1 s window,
    un-cocked firegun hit consumed with no row/score/health, firegun miss
    `takeCanDamage` −0.5, cockgun miss health 0.0 — score/combo unchanged),
    the scene can/explosion runtime (ARCING/SHOT/IMPACTED, `Can Start` →
    `Hit Pico` → EZ explosion, `Can Shot` frame-3 big explosion, the
    darkenStageProps tint pass with the native exclusion lists and the
    0x11→0x22→1.4 s fade), the darnell 11-step can-cutscene chain with the
    cutscene-local conductor per-beat dance rule and the folded overlay
    surface, `ending_video` consumed as the named [D] no-op (video player
    deferred to the media phase).
- **Dialogue fixture family (ADR-0020 third establishment, human-approved):**
  a third `render-slice-2` family `dialogue` (chart `senpai`, difficulty
  `normal`): the schema-3 event replay with the synthesized
  `CUTSCENE_ADVANCE` playthrough (fixed 4 s cadence from the trigger),
  rendered through `SliceRenderer::build_for_song_with_dialogue`, with the
  deterministic trigger-relative sample plan frozen in the manifest.
  Two-run SHA-256 equality is asserted at capture; the established digests
  are engine output on the pinned llvmpipe adapter, NOT native pixel parity.
- **Approval item resolution:** `ab_glyph = "0.2.32"` was added to
  `oxide-game` (pure Rust, MIT/Apache-2.0, zero C/native deps) under the
  human approval; rationale in `docs/DECISIONS.md`. The dialogue fixture
  establishment is the approved third ADR-0020 pass.

### 14.5 Amendment (2026-11-09, 6E review round): findings + resolutions

Two focused reviews covered the phase (dialogue pipeline / weekend-1 + xtask).
Resolutions below; the crate fixes are validated by the in-crate tests and a
re-established dialogue fixture (frames change — the approved ADR-0020 path).

- **Dialogue Opening advance (BLOCK, fixed):** native advancing while the box
  animation still plays dispatches DIALOGUE_COMPLETE_LINE →
  `DialogueBox.skip()` → `FlxTypeText.skip()` is a NO-OP before typing starts,
  so the state REMAINS Opening (Conversation.hx L338-343, DialogueBox.hx
  L172-175). The Rust machine had moved Opening → Idle without a setText,
  corrupting multi-line entries; Opening advances are now no-ops and a
  real-surface multi-line regression pins it.
- **Dialogue text style (fixed):** `letterSpacing` is 0 for the dialogue text
  (the −1 belongs to `PlayState.scoreText` with OUTLINE); the box `isPixel`
  flag affects the box SPRITE sampling only, not the `FunkinTypeText` glyphs
  (default sampling → the dialogue glyph pages use the LINEAR sampler). The
  score text keeps its −1/OUTLINE surface unchanged.
- **Box script animations (fixed):** the shipped scripted box subclasses
  (roses/thorns) are wired: `sentenceEnd` on typing complete, `click` →
  `speaking` on line advances, hidden text + `exit` (REVERSED via the data's
  `frameIndices`, addByIndices ordering) on dialogue end.
- **Advance coalescing/order (fixed):** at most ONE `CUTSCENE_ADVANCE` per
  scene tick (native boolean check) and it applies AFTER the event rows
  (native processSongEvents → input queue → cutscene keys).
- **Weekend-1 `Can Shot` prop draw (fixed):** the 7.1 s z-300 prop had a
  runtime but no draw path; a `Weekend1CanShotProp` stage item/layer is now
  registered and drawn.
- **Named limitations recorded:** the PauseSubState 'Restart Dialogue' path is
  the M7 pause-surface deliverable (owner named; not silently dropped); the
  shared `AnimationData` schema retains no unknown keys inside animation
  objects (pre-existing M1 behavior, now stated instead of implied); the
  `fontFamily` field is fail-closed to the shipped "Pixel Arial 11 Bold";
  the headless cutscene music start remains a play-session audio seam [D].
- **Story-entry flag (integration correction):** `build_chart_events_with_story`
  exposes the 6E trigger toggle; the frozen bopeebo/school render families
  replay with `story_entry = false` (freeplay shape — byte-identical trace
  digests verified at the establishment), the dialogue family with `true`.
- **Dialogue fixture family:** `render-slice-2` gains the `dialogue` family
  (senpai/normal; chart taps + the fixed CUTSCENE_ADVANCE cadence — an
  advance-only plan GameOvers at ~7.7 s on the real chart, so the taps keep
  the sim live; the equivalence claim is the same advance ticks, 30 s end,
  no GameOver, two-run determinism). Frames/digests are llvmpipe engine
  output, NOT native pixel parity. The dialogue frames were re-established
  after the text-style/box-animation fixes (the fifth ADR-0020 pass covers
  this family).

### 6F.1 Implementation record (2026-11-11)

- The `full_chart_gameplay` gate is implemented as a fail-closed xtask
  subcommand (`full-chart-gameplay --corpus --assets <dir>`, exit 0 only
  when every run passes) and wired into the acceptance catalogue at
  milestone 6 (`Action::FullChartGameplay`, dependencies = the portable
  baseline, 3600 s timeout). The gate covers:
  1. every corpus chart-id (61) × its declared difficulties (166 runs) —
     two-run schema-2 note-pass determinism (self sha256 equality, zero
     misses/game-over for the synthesized-tap pass);
  2. two-run schema-3 event-pass determinism with the per-kind row totals
     equal to the parsed chart-file event counts (ScrollSpeed 'both' = one
     row per strumline; the §12.6 accounting);
  3. a per-run scene inventory LOAD check
     (`SliceRenderer::build_for_song`) exercising the stage/character/
     note-style/HUD page resolution end-to-end (no per-tick rendering in
     this gate).
- The existing `gameplay-sim --corpus` prototype remains a diagnostic
  (self-checked evidence, not the gate); the new subcommand is the
  acceptance surface.
- **Deferred parity obligations preserved (NOT covered by this gate):**
  native rendered-output traces (the render fixtures remain engine output
  on llvmpipe, not native pixel parity), Windows/hardware evidence, CI
  artifacts/dual-platform receipts (the `dual_platform_ci` gate), the
  native camera/input probes and the M4 replay/input-jitter obligations.
  Render/perf remain their own gates (`render_diff`, `perf_budget`).
- Milestone acceptance stays cumulative and explicit: a green
  `full_chart_gameplay` gate does not by itself certify M6; the remaining
  required gates and the human/orchestrator CI provenance checks still
  apply.

## 15. Phase 6G behavior freeze — native gameplay shaders & post-processing (WGSL)

Analyst round read the installed `$HOME/Descargas/fnf/Funkin` source + assets
read-only (2026-11-11); original files untouched. Path abbreviations:
`SHADERS/` = `source/funkin/graphics/shaders/`, `FRAG/` =
`assets/preload/shaders/`, `SCRIPTS/` = `assets/preload/scripts/`.

**Scope boundary (APPROVED 2026-11-09, re-affirmed by this freeze):** the 6G
deliverable is the GAMEPLAY/STAGE shader set. Several native shader classes
are used only by M7 UI surfaces (Freeplay, character select, Title, results):
`GaussianBlurShader`, `MosaicEffect`, `AngleMask`, `StrokeShader`,
`LeftMaskShader`, `ColorSwap` — they are recorded here as OBSERVED data, but
their engine integration (if any) belongs to the meta/UI phases, not 6G.

### 15.1 OBSERVED facts (native citations)

**Attach mechanisms (global).** Gameplay sprites get shaders via plain
`sprite.shader = X` (OpenFL runtime shader on the FlxSprite); whole-screen
effects attach as `ShaderFilter` on `FlxG.camera.filters`. `FlxRuntimeShader`
loads its fragment at runtime via `Paths.frag(key)` → `shaders/$key.frag`
(Paths.hx:90-92) — this covers `adjustColor.frag`, `hsv.frag`, `rain.frag`,
`wiggle.frag`, `gaussianBlur.frag`, `mosaic.frag`, `puddle.frag`;
`DropShadowShader`, `SserafimShader`, `MultiplyShader`, `StrokeShader`,
`LeftMaskShader`, `AngleMask`, `ColorSwapShader` carry inline
`@:glFragmentSource` GLSL in the Haxe instead. Erect stage scripts force
`character.useRenderTexture = true` so multi-limb atlas characters render
through one shader pass (mainStageErect.hxc:87, mallXmasErect.hxc:46);
`DropShadowShader`'s `attachedSprite` setter does the same
(DropShadowShader.hx:262-265). No shader in these families is constructed in
Haxe gameplay code (`PlayState`/`Stage`/`Character`); `Stage.setShader`
(Stage.hx:367-373) exists but no shipped script calls it.

**Shared post-effect base (`RuntimePostEffectShader.hx`).** Injects
`uScreenResolution` (= FlxG.width/height), `uCameraBounds` =
(camera.view left/top/right/bottom), `uFrameBounds` = frame UV rect, plus
screen↔world↔frame coordinate helpers. `updateViewInfo` / `updateFrameInfo`
are called MANUALLY by consumers (per frame / per animation frame), not by a
base update loop. No time uniform in the base; time is per subclass.

#### Per-sprite / per-character shaders

**AdjustColorShader** (`SHADERS/AdjustColorShader.hx` + `FRAG/adjustColor.frag`).
- Uniforms, built in Haxe (line numbers from the .hx): `hue` in DEGREES →
  W3C-feColorMatrix hue-rotation `mat3 hueMatrix` with luma weights
  0.299/0.587/0.114 (l.24, l.108-122); `saturation` in percent — positives ×3,
  then `1 + val/100` with Rec.709 luma 0.2126/0.7152/0.0722 (l.39-40,
  l.124-137); `brightness` on a 0-255 scale, uploaded `val/255` (l.51-57);
  `contrast` in percent, `1 + val/100`, positives additionally pushed through
  the exponential curve `(((0.00852259 * e^(4.76454*(val-1))) * 1.01) -
  0.0086078159) * 10 + 1` (l.72-77).
- Fragment composition order is FIXED: **brightness → hue → contrast →
  saturation**; input un-premultiplied (`rgb/a`), output re-premultiplied
  (adjustColor.frag:13-29).
- Shipped values (all static, set once at stage/character build; zero
  FlxTween/FlxTimer targets any AdjustColorShader):
  - mainStageErect.hxc:30-47,75-83 — BF brightness=-23/hue=12/contrast=7,
    GF -30/-9/-4, Dad -33/-32/-23 (saturation 0 everywhere).
  - mallXmasErect.hxc:24-42 — shared hue=5/saturation=20 (santa + all
    characters); boppers hue=15/brightness=20.
  - limoRideErect.hxc:241-267 — hue=-30/sat=-20/brightness=-30 (dancers, car,
    all characters).
  - phillyTrainErect.hxc:201-203,368-374,457-464 — hue=-26/sat=-16/
    brightness=-5 (train, pico pair, bloodPool, characters).
  - phillyStreetsErect.hxc:179-183,667 — hue=-5/sat=-40/contrast=-25/
    brightness=-20, all characters.
  - characters/otis-speaker.hxc:44-91 and nene-tankmen.hxc:20-41 — hue=-40/
    sat=-20/brightness=-40/contrast=-25; viz members brightness=-12/hue=-30.
  - characters/nene-pixel.hxc:161-188 — `noRimShader` hue=-10/sat=-23/
    brightness=-66/contrast=24.

**DropShadowShader** (`SHADERS/DropShadowShader.hx`, inline GLSL).
- Uniforms (defaults from ctor l.485-501): `dist` = `distance` 15 — PIXELS IN
  TEXTURE SPACE, not camera (comment l.47); `str` = strength 1; `thr` =
  threshold 0.1; angle degrees with semantics 0=RIGHT, 90=UP, 180=LEFT,
  270=DOWN (l.29-33), uploaded as `angCos`/`angSin` (l.139-147); `dropColor`
  (FlxColor /255, l.218); built-in HSBC `baseHue/baseContrast/baseSaturation/
  baseBrightness` with the SAME math as AdjustColorShader (l.91-106, 164-206);
  alt mask `altMask` (blue channel, l.70) + `useMask` + `thr2` =
  `maskThreshold`.
- Fragment: sampling offset `uv + vec2(dist*angCos*ratio.x,
  -dist*angSin*ratio.y)` (l.460-463); anti-aliased luma threshold via
  difference-based edge width and `smoothstep(threshold-delta, threshold+
  delta, luma)` (l.397-453); rim term `(1 - shadowAlpha*str) * intensity`
  adds `dropColor * rim` (l.465-475); premultiplied output. Alt mask:
  `useMask && altMask.b > 0` switches the threshold to `thr2` (l.418-431).
- `uFrameBounds` + `angleOffset = frame.angle` are re-uploaded PER ANIMATION
  FRAME CHANGE — scripts hook `animation.onFrameChange` (e.g.
  tankmanBattlefieldErect.hxc:81-87); no per-beat or tweened uniforms.
- Shipped values: tankmanBattlefieldErect.hxc:35-88 — every character
  `setAdjustColor(-46,-38,-25,-20)` (arg order b,h,c,s), color `0xFFDFEF3C`,
  BF/GF angle=90, DAD angle=25/threshold=0.3, gf/nene-tankmen
  `maskThreshold=0.4, useAltMask=true` with masks `erect/masks/gfTankmen_mask`
  / `neneTankmen_mask`; tankman-bloody loads `tankmanCaptainBloody_mask` but
  keeps `useAltMask=false` (saved as `tankmanRim`, l.72). schoolErect.hxc:36-113
  — base `setAdjustColor(-66,-10,24,-23)`, color `0xFF52351d`, distance=5; BF
  angle=90 + pixel masks (maskThreshold=1), GF re-set
  (-42,-10,5,-25)/angle=90/distance=3/threshold=0.3, DAD `senpai_mask`.
  props/TankmanSprite.hxc:57-82 — angle=135/threshold=0.4 (side-dependent
  comment). nene-pixel.hxc:161-176 — abot speaker (threshold=1,
  maskThreshold=0, `aBotPixelSpeaker_mask`).

**HSVShader** (`SHADERS/HSVShader.hx` + `FRAG/hsv.frag`). `_hue/_sat/_val`
MULTIPLICATIVE factors (defaults 1/1/1; not degrees), applied to the HSV
components plus the coupling `value *= (_hue * 0.5) + 0.5` (hsv.frag:40),
then back to RGB. Gameplay use is NOTE desaturation only:
`NoteSprite.desaturate()` sets saturation=0.2 and attaches the shader
(NoteSprite.hx:237-241); `Strumline.hx:964` calls it on a hit note that is a
transparent held note; `revive()` resets shader to null and factors to 1.0
to keep note batching (NoteSprite.hx:262-267). Static otherwise; also used by
M7 UI (Freeplay capsules, DifficultyStars, OptionsState menuBG).

**MultiplyShader** (`SHADERS/MultiplyShader.hx`): `uBlendColor` multiply
(`base*blend*opacity + base*(1-opacity)`). ZERO instantiation sites in
`source/` or `assets/` — dead code. No gameplay behavior to freeze against it.

**StrokeShader** (`SHADERS/StrokeShader.hx`): `size` (px per axis) + `color`;
single-pixel 4-neighbor outline on zero-alpha texels, no AA. Freeplay +
debug offset overlay only; NOT used on gameplay characters.

**LeftMaskShader** (`SHADERS/LeftMaskShader.hx`): uniforms `sprX`, `maskX`,
`uvFrameX/Y` (defaults 0); left-edge wipe — texels with `uv.x < (maskX -
sprX)/TextureSize.x + uvFrameX` become transparent. ResultState (results
screen) ONLY (ResultState.hx:65-66, 354-357, 811 per-frame `swagSprX`).
**NOT the EnableMask target and NOT gameplay.**

**SserafimShader** (`SHADERS/SserafimShader.hx`, inline GLSL; stage
`SCRIPTS/stages/sserafim.hxc`).
- Uniforms (ctor l.313-327): HSBC `baseHue` (DEGREES uploaded raw; `radians()`
  in-GLSL, l.171 — unlike AdjustColorShader), `baseSaturation` (percent math
  in-GLSL l.182-183), `baseBrightness` (0-255, /255 in-GLSL l.200),
  `baseContrast` (percent + the same exponential curve in-GLSL l.188-195);
  `darkAmt` = `darkenAmount` 0-1 (l.21); `lightColor` =
  `pulseLightColor` (WHITE, l.26/122); `pulseStrength` (l.31); 
  `truckStrength` (l.36); `isChar` (ctor arg, l.42) — changes the dark/light
  divisor (5 vs 2, l.279-282).
- Fragment: same HSBC order brightness→hue→contrast→saturation (l.197-212);
  `darkFactor = (truckStrength + pulseStrength) * 0.07 + darkAmt` with the
  light term sign-flipped when `darkAmt > 0.65` (l.279-282); multiply color
  = mix(white, black, darkFactor/5 for chars, /1 for stage) mixed toward
  `lightColor` scaled by pulseStrength (`lightMultiplier = 0.07`, l.268-304);
  premultiplied out.
- Applied to: floor (l.174), sserafim girls (l.175-177), every added
  character (l.346), every prop except light props (l.386-409), cutscene
  sprites (l.633-648); constructed `new SserafimShader(true)` /
  `new SserafimShader()` (l.91-92).
- Cadence (THE ONLY shader with per-event/per-beat uniforms in gameplay):
  - `sserafimDark` event → `setDarkenAmt(amount, duration)` tweens
    `darkenAmount` with `FlxEase.sineInOut` (l.413-420, events l.207-276).
  - `sserafimLights` → `flashTruckLights` sets `truckLightStrength = amount`,
    then per-frame `onUpdate` copies the tweened sprite alpha into the
    shaders, 0 on complete (l.422-449).
  - `sserafimPulseLights` → `setLightState(enabled, colors, durations,
    intensities)` stores per-beat arrays (l.456-464); `onBeatHit` (l.498-505)
    picks `lightsIntensities[beat % n]` → `flashBackLight` sets
    `pulseLightColor`/`pulseLightStrength` and tweens alpha→0, per-frame
    `onUpdate` copies into the shaders (l.466-496).
  - Cutscene adjust-color tweens: l.814-825 (`baseBrightness:55,
    baseHue:0, baseContrast:-30, baseSaturation:0`, sineOut, 49/24 s),
    l.835-846 (`66/10/-17/0`, expoIn, 15/24), `setAdjustColor(0,0,0,0)` at
    the crash (l.858-859); post-kick `resetClear()` =
    `setAdjustColor(-24, 6, -26, -74)` (l.953-954) fading back to 0 over
    `6.0*4` s sineOut (l.974-985). Song retry: `darkenAmount = 0` +
    `FlxTween.cancelTweensOf` (l.360-364).

#### EnableMask event (exact chain)

1. Chart: `data/songs/stress/stress-chart-pico.json:250` —
   `{ "t": 61982, "e": "EnableMask", "v": {} }` (the ONLY occurrence in the
   whole assets tree).
2. Dispatch: PlayState.hx:1498 `new SongEventScriptEvent(event);
   dispatchEvent(eventEvent)` (dispatch order l.1513: Module, Song, Events,
   Notes, Stage, Conversation, Characters); afterwards
   `SongEventRegistry.handleEvent` traces `WARNING: No event handler`
   (SongEventRegistry.hx:113-123) — there is no built-in handler.
3. Consumer: tankmanBattlefieldErect.hxc:97-100 —
   `if (scriptEvent.eventData.eventKind == "EnableMask" && tankmanRim !=
   null) { tankmanRim.useAltMask = true; }` — the whole effect is toggling
   DropShadowShader's `useMask` uniform ON (no position/fields are set).
   Reset to false on song retry (l.122-125); BF's shader cleared on game
   over (l.26-27).

#### Stage / post-processing shaders

**RuntimeRainShader** (`SHADERS/RuntimeRainShader.hx` + `FRAG/rain.frag`).
- Uniforms (defaults from the .hx): `uTime` 1 — seconds, accumulated by
  `update(elapsed)` (l.25, 125-128); `uSpriteMode` false (l.33); `uScale` 1
  — drop size (l.44); `uIntensity` 0.5 — 0 = no rain, 1 = max (l.53);
  `uPuddleY`/`uPuddleScaleY` (l.62/71); `uRainColor` = 0xFF6680cc normalized
  (l.95-122); `uMask`/`uBlurredScreen`/`uLightMap`/`numLights`+`lights[8]`
  are declared but UNUSED in the shipped `main()` (dead/commented paths,
  rain.frag:214-227, 264, 274-289).
- Fragment math: 4-layer drop grid, layer scales {1.0, 1.8, 2.6, 4.8}
  (l.244-249); grid sheared `p.x += p.y * 0.1`, falling
  `p.y -= uTime * 500.0 / scale` (≈500 px/s per scale unit, l.159); per-column
  Y stagger and per-cell X jitter via `rand` (l.164-170); drop shape
  `max(a.x*0.8, a.y*0.5) - 0.1` (l.173); INTENSITY = drop-cell probability:
  `empty = rand(index) < mix(1.0, 0.1, intensity)` (l.175); layer hits
  displace the texture sample (`wpos.x += v*10*uScale; wpos.y -= v*2*uScale`,
  l.256-257) and accumulate `rainSum += (1-rainSum)*0.75`; final
  `color = mix(color, uRainColor, 0.1 * rainSum)` (l.286). Sprite mode maps
  through `screenToFrame`/`screenToWorld` and samples the sprite texture
  displaced (l.237, 268-272). The puddle/ripple + reflection block inside
  rain.frag is COMMENTED OUT (l.274-283 dead) — `uPuddleY`/`uPuddleScaleY`
  are settable but visually inert.
- Usage: whole-screen `ShaderFilter` on `FlxG.camera` in
  phillyStreets.hxc:23,124,149-150 (scale = FlxG.height/200; per-song
  intensity remap darnell 0→0.1, lit-up 0.1→0.2, 2hot 0.2→0.4; puddle values
  set at l.527-528), phillyStreetsErect.hxc:25,126,152-153 (same scale;
  intensities 0→0.01, 0.01→0.02, 0.02→0.04; `rainColor = 0xFFa8adb5`),
  phillyBlazin.hxc:26,38-42 (intensity FIXED 0.5). Sprite-level
  `spriteMode = true` on the `bgTrees` prop in spookyMansionErect.hxc:24,39-45
  (scale = FlxG.height/200*2, intensity 0.4; `updateFrameInfo(frame)` per
  frame change at l.84 — the only `updateFrameInfo` consumer). Game over:
  `FlxG.camera.filters = []` (phillyStreets.hxc:498-503, phillyBlazin.hxc:49);
  retry restores the filter (phillyStreets.hxc:513, phillyBlazin.hxc:56).
- Cadence: `uTime` per frame via `update(event.elapsed)` in stage onUpdate
  (phillyStreets.hxc:382/388, phillyBlazin.hxc:103, spookyMansionErect.hxc:97);
  `uIntensity` per frame (NOT per beat) =
  `FlxMath.remapToRange(Conductor.songPosition, 0, music.length, start,
  end)` (phillyStreets.hxc:378-380; reset to start when no music, l.386);
  phillyBlazin multiplies elapsed by `rainTimeScale` which smooth-lerps to
  0.02 and INCREASES by 0.7 per note hit (l.103-104, 139). No beat-bop
  driver.

**PuddleShader** (`SHADERS/PuddleShader.hx` + `FRAG/puddle.frag`): the
fragment is `color.r = color.g = color.b = color.a` — a grayscale-from-alpha
passthrough, NOT a UV distortion. ZERO instantiation sites (only the class
definition); the real puddle mirror lives inside rain.frag and is commented
out there. Dead code.

**WaveShader** (`SHADERS/WaveShader.hx`): inline fragment is a pure
pass-through (identity). ZERO consumers. Dead code.

**WiggleEffectRuntime** (`SHADERS/WiggleEffectRuntime.hx` + `FRAG/wiggle.frag`).
- Uniforms: `effectType` int (enum: DREAMY=0, WAVY=1, HEAT_WAVE_HORIZONTAL=2,
  HEAT_WAVE_VERTICAL=3, FLAG=4, l.6-13); `uSpeed` (`waveSpeed`),
  `uFrequency` (`waveFrequency`), `uWaveAmplitude` (UV units), `uTime`
  seconds accumulated by `update(elapsed)` (l.80-85). No
  setDuration/setForce API in this runtime version.
- Fragment (DREAMY, l.34-48): pixel-grid QUANTIZED sine displacement —
  `pt.x = floor(pt.x/h)*h`; `offsetX = sin(pt.x*uFrequency + uTime*uSpeed) *
  uWaveAmplitude`; `pt.y += floor(offsetX/w)*w; pt.y = floor(pt.y/w)*w`;
  then the y-axis half-amplitude/half-speed pass. WAVY = plain
  `pt.y += sin(...)`; HEAT_WAVE_H/V single-axis; FLAG dual-axis with extra
  spatial terms (l.56-58).
- Usage (sprite-level on stage props): schoolEvil.hxc:24-32 —
  `wiggleBack = new WiggleEffectRuntime(2*0.8, 4*0.4, 0.011, DREAMY)`
  (uSpeed 1.6, uFrequency 1.6, amplitude 0.011) on `backTrees`;
  `wiggleSchool = (2, 4, 0.017)` on `school`;
  `wiggleGround`/`wiggleTrees` = (2, 4, 0.007) on `street`/`trees`.
  schoolEvilErect.hxc:24-32 — `wiggleBack = (2*0.8, 4*0.4, 0.011)` on
  `backspikes`, `wiggleSchool = (2, 4, 0.017)` on `school`,
  `wiggleSpike = (2, 4, 0.01)` on `backspike`, and
  `wiggleGround = (2, 4, 0.007)` on `evilstreet`.
- Cadence: ONLY `uTime` advances, per frame via `update(event.elapsed)`
  (schoolEvil.hxc:44-50, schoolEvilErect.hxc:44-50). uSpeed/uFrequency/
  uWaveAmplitude/effectType are set ONCE at construction and never re-set
  anywhere in the tree. NO beat-bop interaction (stage `onBeatHit` in these
  stages never touches wiggle uniforms).

**GaussianBlurShader** (`SHADERS/GaussianBlurShader.hx` + `FRAG/
gaussianBlur.frag`): class comment "not actually gaussian!". `_amount`
float, default 1 (radius ∝ `_amount*2` texels per axis). Effective output is
a two-pass `blur13` kernel (offsets 1.4118/3.2941/5.1765 × direction,
weights 0.19648/0.29691/0.09447/0.01038, gaussianBlur.frag:61-85); the
tilt-shift 3×3 loop computes a value that is DISCARDED
(`gl_FragColor = blurredShit`, l.113). Static (amount set once). ALL usage
is M7 UI: Freeplay capsules/rank/fav icons (CapsuleText.hx:35,
SongItemGroup.hx:15-26) and char-select/freeplay fades. NO pause/game-over/
level-start camera usage exists in gameplay.

**MosaicEffect** (`SHADERS/MosaicEffect.hx` + `FRAG/mosaic.frag`):
`uBlocksize` in TEXELS; `floor(coordv*blocks)/blocks` quantization.
Single usage: char-select Nametag (Nametag.hx:22-53, 85-113) with a
1/30 s step sequence of block sizes. Event-driven, never per frame/beat.
M7 UI only.

**Cadence summary (cross-cutting).** No gameplay shader in the shipped game
is driven by camera-zoom bops or SetCameraBop. Per-frame: rain uTime +
intensity remap, wiggle uTime, sserafim tween onUpdate copies. Per
animation-frame change: rain sprite-mode + DropShadow uFrameBounds/angle
sync. Per event: EnableMask → useAltMask, sserafim event tweens. Per beat:
only sserafim `sserafimPulseLights` backlight selection.

### 15.2 DERIVED requirements (Oxide engine integration; labelled DER)

1. **Per-sprite shaders map to per-layer materials (DER).** The scene's
   ordered `LayerSpec` quads currently render with a plain tint
   (`ColorMaterial`). Each gameplay shader family becomes one WGSL fragment +
   one uniform struct bound as a Bevy custom material on the affected LAYERS
   (a character or stage prop may own one). HSBC uniforms stay the SCENE's
   f64 state and convert to f32 once at material sync, matching the
   existing vertex-cast rule (§2 decision 1).
2. **Whole-screen post effects map to a post pass on the game-camera
   target, BEFORE HUD compositing (DER).** Native `FlxG.camera.filters`
   applies to the game camera only — camHUD is a separate camera not
   filtered by rain. In the layer stack the post pass must therefore cover
   the stage/character/overlay layers and NOT the HUD / dialogue layers
   (HUD layers start at the existing HUD index; dialogue/cutscene layers sit
   above and are likewise outside the game camera in native).
3. **Time driver (DER + [D]).** `uTime` accumulates in SECONDS from the same
   1 ms sim ticks (`TICK_SECONDS`) the scene already consumes — deterministic
   fixed-step accumulation replaces the native `update(elapsed)`. Rain
   intensity derives from the conductor's song position in ms via the same
   `remapToRange(start, end)` formula; `[D]` because the headless/scene
   renderer does not decode audio streams, `rain_duration_ms` uses the
   deterministic chart-end timestamp `max(note.time + note.sustain_ms).max(1.0)`
   in place of native `FlxG.sound.music.length`. phillyBlazin's `rainTimeScale`
   lerps toward 0.02 and gains 0.7 per note hit. On game over, the screen-rain
   pass sets `u_intensity = 0.0` (an identity pass over the offscreen game
   target so the unfiltered stage remains visible).
4. **Frame-change uniform sync (DER + [D]).** Wherever the anim scheduler
   already processes frame changes, that hook must re-emit the frame-dependent
   uniforms (rain sprite-mode frame info; DropShadow `uFrameBounds` +
   `angleOffset = frame.angle`). `[D]` For multi-limb animate-atlas characters
   under `DropShadowShader`, binding the shader per page layer samples the
   atlas page directly (`uFrameBounds = [0, 0, 1, 1]`); this is an engine
   approximation rather than an offscreen per-character render texture
   (`useRenderTexture = true`), so rim/shadow sampling across adjacent limb
   boundaries is a named limitation alongside native rendered-output parity.
5. **EnableMask (DER).** `EnableMask` remains a trace-recorded event whose
   only gameplay effect is setting the tankman DropShadow layer's
   `useMask` flag true (applies to the saved `tankmanRim` shader state),
   reset on retry; the event carries no fields. Sserafim-specific events
   (`sserafimDark`, `sserafimLights`, `sserafimPulseLights`) drive the
   SserafimShader tween/per-beat state as OBSERVED (15.1).
6. **WGSL uniform layout (DER).** Uniform structs must match WGSL
   alignment: scalars as f32, vec2/vec4 aligned, mat3 packed as
   `mat3x3<f32>` (16-byte columns). Percent/degree/unit conversions stay in
   the same place as native (hue degrees → matrix in shader or upload, per
   the OBSERVED class; brightness /255; saturation/contrast curves exactly
   transcribed). Composition order (brightness→hue→contrast→saturation) is
   an OBSERVABLE invariant.

### 15.3 Engine decisions

1. **One WGSL family per native shader family, no shader-registry
   framework.** Extend the existing per-layer material model with a private
   material variant (Bevy `Material2d` implementations) — an enum over the
   shipped families plus plain tint. Generic .frag loading or runtime GLSL
   parsing is NOT introduced; the shipped runtime-loaded fragments
   (adjustColor, hsv, rain, wiggle, plus the inline DropShadow/Sserafim
   math) are transcribed to WGSL constants in `oxide-game`. Equivalence
   check: identical uniform values must produce identical per-pixel output
   on the same sampled texture (validated on llvmpipe fixtures).
2. **Dead native code is recorded, not implemented (OBS→decision).**
   `MultiplyShader`, `WaveShader`, `PuddleShader`, and rain.frag's
   commented-out puddle/ripple/reflection/light paths have no shipped
   behavior; implementing them would invent observables. The gameplay-
   required set is: AdjustColorShader, DropShadowShader (+mask),
   HSVShader (note desaturation), RuntimeRainShader (screen + sprite
   modes), WiggleEffectRuntime, SserafimShader (+ its three events).
3. **M7 UI shader families are OBSERVED-only here** (GaussianBlur,
   MosaicEffect, AngleMask, StrokeShader, LeftMaskShader, ColorSwap): their
   integration is deferred to the meta/UI phases; no 6G work is spent on
   them.
4. **`useRenderTexture` approximation (`[D]`).** Native erect stages force a
   render texture so all limbs share one shader pass; Oxide binds the shader
   at each character page layer without introducing an intermediate per-
   character render texture. For `AdjustColor` and `Sserafim` (point operations)
   this is single-pass equivalent where limbs do not overlap; for `DropShadow`
   on animate-atlas characters, sampling the atlas page (`uFrameBounds =
   [0, 0, 1, 1]`) is an engine approximation recorded as a named limitation.
5. **No new dependencies** (Bevy 0.20 custom materials are built-in); no
   `oxide-vfs` change needed since shaders are source-level WGSL, not
   resolved assets.

### 15.4 Validation obligations & human approval items

- **Unit obligations (no fixture approval needed):** uniform stepping
  determinism in scene steps — uTime accumulation at fixed 1 ms ticks, rain
  intensity remap at known song positions (incl. the no-music reset and the
  phillyBlazin timeScale/note-hit rule), sserafim tween + per-beat light
  progression, EnableMask → useAltMask toggle + retry reset in trace/state,
  DropShadow frame-change sync. Two-run sha256 determinism on shader-active
  scene state rows per the established pattern.
- **Fixture establishment proposal (needs ADR-0020 path + HUMAN APPROVAL):**
  representative shader-active stages on the pinned `llvmpipe` adapter —
  phillyStreetsErect (rain + adjust-color), schoolEvil (wiggle),
  tankmanBattlefieldErect or schoolErect (drop-shadow + EnableMask) and
  sserafim (event-driven light/dark). Fixtures are established once after
  review, never regenerated during `verify`; per 6F they remain ENGINE
  fixtures on llvmpipe, NOT native pixel parity — native rendered-output
  traces stay a standing deferred obligation (§14.x / 6F record).
- **Approval items:** (a) the fixture-establishment ADR for the 6G shader
  families (HUMAN-APPROVED 2026-11-11); (b) none other — no new
  dependencies, no tolerance changes, no asset changes are requested.
  Implementation of the WGSL families may proceed after this freeze on the
  approved write set (`crates/oxide-game/src/render*`), with review before
  integration.

### 15.5 Phase 6G implementation record & review resolutions (2026-11-11)

- **Delivered (`crates/oxide-game/src/render/shaders.rs`, `layer_shader.wgsl`,
  `scene.rs`, `harness.rs`, `load.rs`, `play/window.rs`, `xtask/src/render_diff.rs`):**
  one private `LayerShaderMaterial` (`Material2d`) with a synchronously
  registered embedded WGSL fragment (`layer_shader.wgsl`) covering all six
  in-scope gameplay shader families while keeping un-shadered layers on the
  frozen `ColorMaterial` pipeline:
  - Kind 0 (`AdjustColor`): W3C hue matrix, Rec.709 saturation (positive ×3),
    brightness `/255`, contrast exponential curve above 1.0, fixed composition
    order brightness → hue → contrast → saturation.
  - Kind 1 (`DropShadow`): HSBC + texture-space rim offset + difference-based
    AA luma threshold + blue-channel `altMask` (`gfTankmen_mask`,
    `neneTankmen_mask`, `tankmanCaptainBloody_mask`, `bfPixel_mask`,
    `picoPixel_mask`, `gfPixel_mask`, `nenePixel_mask`, `senpai_mask`) +
    per-frame `uFrameBounds` sync + `EnableMask` event toggling
    `tankman-bloody`'s `use_alt_mask = true` at `61982` ms on `stress-pico` +
    `tankmanBattlefieldErect` `GameOver` clearing the player's shader.
  - Kind 2 (note `HSV`): multiplicative HSV factors (`1.0, 0.2, 1.0`, tint
    alpha `0.5`) on a dedicated overlay layer for player notes hit with
    combo-breaking `Bad`/`Shit` judgements. `[D]` Because the frozen `bopeebo`
    negative-phase replay injects threshold `Bad`/`Shit` hits,
    `fixture_note_desaturation(chart_id)` is `chart_id != "bopeebo"` on the
    `SliceRenderer::build_for_song` path (`false` on `SliceRenderer::build`),
    keeping the frozen `bopeebo` frame digests byte-identical while enabling
    the layer for `PlaySession` and all non-bopeebo songs (verified on
    `llvmpipe` by `hsv_desaturated_note_layer_renders_through_the_material_pipeline`).
  - Kind 3 (`WiggleEffectRuntime`): `DREAMY` pixel-quantized sine displacement
    (plus `WAVY`, `HEAT_WAVE_HORIZONTAL`, `HEAT_WAVE_VERTICAL`, `FLAG`),
    `u_time` advancing by `TICK_SECONDS` per 1 ms tick across `schoolEvil`
    and `schoolEvilErect` props.
  - Kind 4 (`RuntimeRainShader`): screen mode via a full-screen post layer
    immediately before `overlay_start` sampling an offscreen `Camera { order: -1 }`
    `game_target` (`RenderLayers::layer(1)`) so `camHUD`/strumline/dialogue/
    cutscene remain unfiltered (`phillyStreets`, `phillyStreetsErect`,
    `phillyBlazin`), plus sprite mode on `spookyMansionErect` `bgTrees`;
    deterministic `u_time` accumulation (starting at `1.0`), per-song
    intensity remap over `[0, rain_duration_ms]`, `phillyBlazin` `rainTimeScale`
    note-hit `+0.7` boost and `smoothLerpPrecision` decay, and game-over
    `u_intensity = 0.0` identity pass.
  - Kind 5 (`SserafimShader`): in-shader HSBC (with positive saturation ×3) +
    dark/light divisors (`5/3` for characters, `1/2` for stage props) +
    HSL-scaled pulse light; light/cover prop exemptions (`truckLight1/2`,
    `backLightColor/White`, `truckTest`, `solidCover`); `sserafimDark`
    (`sineInOut`), `sserafimLights` (`cubeInOut` + `truckLight1/2` alphas),
    `sserafimPulseLights` (per-beat `cubeInOut` + `backLightColor/White` tints
    with 8-digit `0xAARRGGBB` `FlxColor.fromString` parsing), and
    `sserafimCover` (`solidCover` alpha).
- **Review resolutions:** (1) `parse_sserafim_color` fixed to skip the first
  two alpha hex digits on 8-digit `0xAARRGGBB` strings (`0xFF6D43FF` → RGB
  `6D43FF`), refreshing the `sserafim/hard` `39000` ms pulse-light frame and
  run digest; (2) `sserafim_saturation_rows` triples positive saturation
  values before `1 + val/100`; (3) `schoolEvilErect` 4-prop wiggle table
  (`school`, `evilstreet`, `backspikes`, `backspike`) confirmed against
  `schoolEvilErect.hxc:24-32` and recorded in §15.1; (4) `rain_duration_ms`
  chart-end proxy and per-page-layer `DropShadow` atlas sampling explicitly
  labelled `[D]`; (5) `tankmanBattlefieldErect` `GameOver` player shader clear
  implemented and tested; (6) 6G establishment provenance appended to
  `_manifest.json` `honesty`; (7) `llvmpipe` end-to-end render test added for
  the `LayerShader::Hsv` desaturated-note pipeline.
- **Named limitations:** llvmpipe engine fixtures are NOT native pixel parity;
  WGSL runs on Bevy's linearized sRGB samples; animate-atlas `DropShadow`
  samples per page layer rather than a composed character render texture;
  sserafim stage-scripted extra characters (`yunjin`/`chaewon`/`eunchae`),
  perspective floor, dust backdrops and cutscene HSBC tweens remain outside
  the data-driven stage loader; retry resets are implicit via scene rebuild.

## 16. Phase 6H behavior freeze — PlayState visual polish & song-parameterized `oxide play`

Analyst round read the installed `$HOME/Descargas/fnf/Funkin` source + assets
read-only (2026-11-12); original files untouched. `Path correction:` in this
checkout the popup class is `source/funkin/play/components/PopUpStuff.hx`
(no `play/notes/PopUpStuff.hx`), and there is no `popupScore` API — the
entry points are `displayRating`/`displayCombo`. This freeze closes the
§13.2 exclusion #5 scope list (judgement pops, combo pops, note splashes,
hold covers, countdown display) before M7.

### 16.1 OBSERVED facts (native citations)

#### NoteSplash (`source/funkin/play/notes/NoteSplash.hx`)

- Fields: `splashFramerate = 24`, `splashFramerateVariance = 2`
  (l.10-11), overwritten from the noteStyle at build (`NoteStyle.hx:894-895`).
- `play(direction, ?variant)` (l.40-60): when `variant` is null, counts
  animations named `splash{DIRECTION}{i}` and picks
  `variant = FlxG.random.int(0, animationAmount)` (l.44-46); animation names
  are assigned by `NoteStyle.hx:983-989` from the JSON per-direction arrays
  (`toNamedArray("splashLEFT")` appends the index, `AnimationData.hx:28`).
  Then `frameRate = splashFramerate + FlxG.random.int(-variance, variance)`
  (l.56) and `offset.set(width * 0.3, height * 0.3)` (l.59). `onAnimationFinished`
  → `kill()` (l.62-66); no timers/tweens.
- **No tint/RGB shader** (no `extraTint|extraColor|RGBPalette` in
  `play/notes`): splash color is entirely per-direction atlas content.
- Spawn rule (PlayState.hx:3142-3151): `HitNoteScriptEvent.doesNotesplash`
  is `daRating == 'sick'` (default false, "true only on 'sick' notes",
  ScriptEvent.hx:156-159); splash fires on `playerStrumline` only, same tick
  as the hit, unless the event is cancelled (l.3147). **The opponent never
  gets splashes** (`playNoteSplash` has exactly one call site, PlayState.hx:3150;
  opponent/botplay paths only `hitNote` + `playNoteHoldCover`).
- Pool (`Strumline.hx:48, 236-237`): `noteSplashes` group with
  `NOTE_SPLASH_CAP = 6`, `zIndex = 50`. `playNoteSplash` (l.1076-1098) guards
  `showNotesplash` and `noteStyle.isNoteSplashEnabled()`; positioning
  (l.1087-1094): `x = strumline.x + getXPos(direction) + INITIAL_OFFSET +
  splashOffsets[0] * splash.scale.x`, `y = strumline.y - INITIAL_OFFSET +
  splashOffsets[1] * splash.scale.y` (INITIAL_OFFSET = -0.275 * 104, l.45;
  `getXPos` = index * (104+8) * noteSpacingScale * strumlineScale.x,
  l.1333-1352) — the strumline's own x/y, NOT the receptor sprite's.
- `constructNoteSplash` (l.1228-1257): create when below cap; else revive the
  first available; else **`FlxG.random.getObject(noteSplashes.members)`**
  evicts a random ACTIVE splash (l.1252). `destroyOnNoUse = false` (l.1096).
- Complete FlxG.random inventory for splashes: variant choice (NoteSplash.l.46),
  per-play fps jitter (l.56), pool eviction (Strumline.l.1252).

#### NoteHoldCover (`source/funkin/play/notes/NoteHoldCover.hx`)

- A `FlxTypedSpriteGroup` with ONE child `glow` (l.9-31); `sparks` is declared
  but never instantiated (l.16, null-checked l.77/88; `NoteStyle.hx:1011-1012`
  confirms only glow is implemented). NOT a per-direction slot pool; one
  cover instance per active hold note; group `zIndex = 40`, no size cap
  (Strumline.l.232-233).
- Animations per direction (3 each, built by `NoteStyle.hx:1009-1048,
  1094-1100`): `holdCoverStart{Color}`, `holdCover{Color}` (FORCED
  `looped = true`, l.1094), `holdCoverEnd{Color}`; color names
  purple/blue/green/red for LEFT/DOWN/UP/RIGHT (`NoteDirection.hx:65-80`).
- State machine `onAnimationFinished` (l.91-103): `holdCoverStart*` →
  `playContinue()`; `holdCoverEnd*` → `visible = false; kill()`.
  `playStart` does one `glow.setPosition(this.x, this.y)` (l.51); `update`
  is EMPTY (l.44-47) — the cover does NOT follow the sustain after that.
- `kill` (l.68-78) hides the group and clears `holdNote.cover`; `revive`
  (l.80-89) restores `visible = true; alpha = 1.0`.
- Lifecycle rules in `updateNotes` (Strumline.hx): sustain finished with
  `hitNote && sustainLength <= 0` → player: `cover.playEnd()`, non-player:
  cover hidden + killed immediately (l.671-680); hold dropped →
  `cover.visible = false; cover.kill()` (l.714-719) and the dropped hold
  itself gets `alpha = 0.0` (l.640). `SustainTrail.kill()`
  (SustainTrail.hx:435) also calls `cover.playEnd()` when the current
  animation is not already a `holdCoverEnd*`. `clean()` kills all splashes
  and covers (l.892-902).
- Spawn (l.1104-1132, positioning l.1119-1130): guards `showNotesplash` +
  `noteStyle.isHoldNoteCoverEnabled()`; positioning
  `x = strumline.x + getXPos(dir) + STRUMLINE_SIZE/2 - cover.width/2 +
  holdCoverOffsets[0]*scale.x - 12` and
  `y = strumline.y + INITIAL_OFFSET + STRUMLINE_SIZE/2 +
  holdCoverOffsets[1]*scale.y - 96` — the `-12`/`-96` are literal hardcoded
  adjustments ("we are evil" comment). Spawned from
  `playNoteHoldCover(holdNoteSprite)` on player hit of a hold note
  (PlayState.hx:3151), on opponent hits (l.2830-2835) and botplay (l.2888-2893).
- **No miss/red/gray cover variant exists** in this engine.

#### Judgement/combo popups (`source/funkin/play/components/PopUpStuff.hx`)

- Group setup: `comboPopUps = new PopUpStuff(noteStyle)` (PlayState.hx:815),
  `zIndex = 900` (2384), `cameras = [camHUD]` (2386). `popUpScore(daRating)`
  (PlayState.hx:3365-3380): early-returns on `'miss'`; calls
  `displayRating(daRating)` and `if (combo >= 10) displayCombo(combo)`.
  Combo break with combo >= 10 additionally calls `displayCombo(0)`
  (PlayState.hx:3351-3352). Only caller is the scoreable player hit
  (l.3163).
- `displayRating` (l.31-70): sprite from
  `noteStyle.buildJudgementSprite(rating)` (null rating → `"good"`, l.33);
  `zIndex = 1000`; position
  `x = FlxG.width * 0.474 - width/2`,
  `y = FlxG.camera.height * 0.45 - 60 - height/2` (l.40-43) + style offsets
  (l.47-49); physics: `acceleration.y = 550`,
  `velocity.y -= FlxG.random.int(140, 175)`,
  `velocity.x -= FlxG.random.int(0, 10)` (l.51-53); fade
  `alpha → 0` over `0.2 s` with `startDelay = beatLengthMs * 0.001`,
  ease `EaseUtil.stepped(2)` for pixel judgements else null
  (l.59-69), destroyed on complete. NO pooling, NO rotatePoint, NO
  random starting offsets (verified absent).
- `displayCombo` (l.72-124): digits zero-padded to 3 (l.82-83; the
  `reverse` is commented out — most-significant-first with `daLoop` from 1);
  per digit `x = FlxG.width * 0.507 - 36 * daLoop - 65`,
  `y = FlxG.camera.height * 0.44` (l.93-94) + style offsets; physics:
  `acceleration.y = FlxG.random.int(250, 300)`,
  `velocity.y -= FlxG.random.int(130, 150)`,
  `velocity.x = FlxG.random.float(-5, 5)` (l.102-104); fade `alpha → 0`
  over `0.2 s` with `startDelay = beatLengthMs * 0.002`, pixel ease
  `stepped(2)` else null (l.112-120). `displayCombo(0)` renders three
  `num0` digits.
- Assets (NoteStyle.hx:533-579 build + path resolution 606-628/794-823;
  `scrollFactor(0.2, 0.2)`, `antialiasing = !isPixel`, pixel-perfect for
  pixel): funkin.json — judgements `default:ui/popup/funkin/{sick,good,bad,shit}`
  scale 0.65 (lines 130-148), digits `default:ui/popup/funkin/num0..num9`
  scale 0.45 (150-198); pixel.json (fallback "funkin", line 5) — judgements
  and digits `default:ui/popup/pixel/...` scale 4.2, isPixel (141-209).

#### Countdown (`source/funkin/play/Countdown.hx`)

- Steps `BEFORE, THREE, TWO, ONE, GO, AFTER` (l.18, 285-293);
  `performCountdown` (l.47-103): `COUNTDOWN_START` first (script cancel ⇒
  return false, l.49-54 — the §14 story cutscene hook cancels this);
  `Conductor.update(startTimestamp + beatLengthMs * -5)` (l.60) — the
  countdown begins 5 beats before the song start; one `FlxTimer` at
  `beatLengthMs / 1000` s for **5** iterations driving
  BEFORE→THREE→TWO→ONE→GO→AFTER (l.66-100); each tick decrements,
  `showCountdownGraphic`, `playCountdownSound`, fires `COUNTDOWN_STEP`
  (cancel ⇒ pause, l.89-94); `AFTER` ⇒ stop (l.96-99). The only
  `COUNTDOWN_*` constant is `COUNTDOWN_VOLUME = 0.6`
  (`Constants.hx:680`).
- Graphics `showCountdownGraphic` (l.220-246): per-step sprite from
  `noteStyle.buildCountdownSprite(index)`; fade `alpha → 0` over
  `beatLengthMs / 1000` s with `cubeInOut` (pixel: `EaseUtil.stepped(8)`);
  `cameras = [camHUD]`, `screenCenter()` + style offsets. `funkin.json`:
  `countdownThree.assetPath = null` (**no "3" graphic**; sound
  `shared:gameplay/countdown/funkin/introTHREE`, lines 102-104),
  `ready`/`set`/`go` = `shared:ui/countdown/funkin/*` scale 1.0 (110-126);
  `pixel.json`: `shared:ui/countdown/pixel/*` scale 6.0 isPixel (117-139),
  sounds `shared:gameplay/countdown/pixel/intro*`. Files verified:
  `assets/shared/images/ui/countdown/{funkin,pixel}/{ready,set,go}.png`,
  `assets/shared/sounds/gameplay/countdown/{funkin,pixel}/intro{THREE,TWO,
  ONE,GO}.ogg` (+ mp3 twins; desktop ext = `ogg`, `Constants.hx:618`).
- Sound `playCountdownSound` (l.251-258): `Paths.sound` for the
  `library:path` value, `FunkinSound.playOnce(path, 0.6)` — fire-once, not
  looped music.
- Song start: during countdown Conductor advances manually (PlayState.hx:
  1144-1152); when `songPosition >= startTimestamp + combinedOffset` ⇒
  `startSong()` (l.1151); `startSong` (l.2634+) starts the instrumental
  (`playInst(1.0, instId, false)`) — music starts AFTER the countdown;
  `FlxG.sound.music.time = startTimestamp` (l.2659). Pause pauses/resumes
  the countdown timer (PlayState.hx:1373-1375/1697; Countdown.hx:136-155);
  `skipCountdown` stops the timer and `Conductor.update(0)` (l.175-181);
  retry re-runs `performCountdown` after a 0.5 s vwoosh
  (PlayState.hx:1118-1132). Video-cutscene end calls
  `startCountdown()` (VideoCutscene.hx:370).

#### Audio stem resolution (Paths.hx + Song.hx) and the 61 chart-id inventory

- Rules (`source/funkin/Paths.hx:142-159`; `project.hxp:1426-1427` maps
  `assets/songs` to the `songs` library; desktop sound ext = `ogg`):
  `songs:assets/songs/{song.toLowerCase()}/Inst{suffix}.ogg` and
  `.../Voices{suffix}.ogg`. `SongDifficulty.getInstPath` (Song.hx:830-850):
  a requested inst id present in `altInstrumentals` uses `-{instId}`;
  otherwise the default is `-{characters.instrumental}` (empty → none).
  Voice lists (Song.hx:900-999): variation suffix `-{variation}` unless
  empty/`'default'`; when metadata declares `playerVocals`/`opponentVocals`,
  EVERY declared `Voices-{id}{suffix}.ogg` must exist or the declared list
  falls through; the automatic path tries `Voices-{charId}{suffix}.ogg`
  stripping one `-segment` from the char id per miss (`bf-car` → `bf`),
  then retries the whole chain WITHOUT the variation suffix; `buildVoiceList`
  (l.886-898) finally tries bare `Voices{suffix}.ogg`; `buildVocals`
  (l.1006-1051) loads only existing paths and falls back to the legacy
  single bare `Voices{suffix}.ogg` as a player voice.
- Inventory (all 61 metadata chart-ids; charts only under
  `assets/preload/data/songs/<song>/`, stems under `assets/songs/<song>/`):
  every chart's declared inst stem exists (base and declared `-pico`
  alternates). Voice stem facts that must drive engine behavior:
  1. **`spaghetti:default`**: `opponentVocals: ['sserafim-kazuha']` declared
     but no `Voices-sserafim-kazuha.ogg`/`.mp3` and no suffix-strip/bare
     fallback exists → the resolved opponent voice list is EMPTY (music
     plays, no opponent vocals).
  2. **`blazin:default`**: metadata declares no vocals and
     `assets/songs/blazin/` has no `Voices*` at all — fully instrumental.
  3. **`monster:default`**: stray legacy `Voices.mp3` with no `.ogg`
     counterpart — unused on desktop (`EXT_SOUND = 'ogg'`).
  4. Segment-strip fallbacks are exercised widely: `bf-car`/`mom-car`
     (`high:default`, `milf:default`, `satin-panties:default`),
     `bf-pixel`/`senpai-angry` (senpai/roses/thorns defaults),
     `pico-playable` (many `-pico` player vocals strip to `Voices-pico*`),
     `pico-holding-nene`/`tankman-bloody` (stress:pico), and the weekend-1
     `darnell/lit-up:default` player voice `Voices-pico.ogg` (stripped from
     `pico-playable`). Christmas variants (`bf-christmas`, `monster-christmas`,
     `parents-christmas`) reuse the non-christmas stems on the default
     difficulties but have dedicated `-erect` stems.
  5. `test/test.json` is chart-editor data outside the 61 (stems exist but
     are not corpus).

#### noteStyle JSON schema (splash/cover/popup/countdown relevant)

`source/funkin/data/notestyle/NoteStyleData.hx`: shared asset fields
`assetPath` (required), `scale` default 1.0, `offsets` default `[0,0]`,
`isPixel` default false, `alpha` default 1.0, `animated` default false;
`UnnamedAnimationData`: `looped`/`flipX`/`flipY` default false, `frameRate`
default 24, `frameIndices` default all. noteSplash: `enabled` default true,
`framerateDefault` 24, `framerateVariance` 2, `blendMode` "normal",
per-direction `leftSplashes/downSplashes/upSplashes/rightSplashes` arrays;
accessor fallback chain noted (`NoteStyle.hx:342-345, 973-1007`).
holdNoteCover: `enabled` default true; per-direction
`{assetPath?, start?, hold?, end?}` with the fallback order
direction assetPath → root assetPath → fallback direction → fallback root
(`NoteStyle.hx:1141-1143`); `getHoldCoverOffsets` default `[0,0]`.
- `funkin.json`: splash `shared:noteSplashes`, alpha 0.8, offsets `[25,-5]`,
  2 variants per direction (the literal prefix `"note impact 1  blue0"`
  on line 64 contains a DOUBLE SPACE — OBSERVED shipped-file quirk, no
  scale/isPixel/framerate/blendMode → defaults). Cover: per-direction
  `shared:holdCover{Purple,Blue,Green,Red}` with
  `holdCoverStart*/holdCover*/holdCoverEnd*` prefixes.
- `pixel.json` (fallback "funkin"): splash `week6:pixelNoteSplash`,
  scale 4.0, alpha 1.0, isPixel, offsets `[27.5, 18.5]`, blend "screen",
  fps 33±2, 3 variants per direction; cover `week6:pixelNoteHoldCover`,
  scale 6.0, isPixel, offsets `[29, -4]`, all directions share the
  `loop0000`/`loop`/`explode` prefixes.

### 16.2 DERIVED requirements (Oxide engine integration; labelled DER)

1. **Ephemeral layer pools (DER).** All four polish families live on `camHUD`
   (strumlines get `cameras = [camHUD]`), i.e. ABOVE the existing HUD group
   in our layer stack. Native ordering among the ephemerals: covers
   (`zIndex = 40`) < splashes (`zIndex = 50`) < combo group (`zIndex = 900`)
   < rating sprites (`zIndex = 1000`); they slot relative to the HUD
   elements per the §13-frozen HUD zIndex values. The scene gains a
   FIXED-SIZE ephemeral layer set appended after the HUD group:
   covers (bounded by active player+opponent holds, one instance per hold),
   splashes (CAP = 6, evict per 16.1), 1 rating popup + up to 3 combo digits
   (fresh instances, no pooling). Each ephemeral layer owns a private state
   machine advanced on the 1 ms sim tick; empty layers stay hidden.
2. **Deterministic randomness (DER + [D]).** Native drives splash variant,
   splash fps jitter, pool eviction and ALL popup velocities with
   `FlxG.random` (non-deterministic across runs). For engine determinism
   (two-run sha256 fixtures) the scene uses a private PRNG seeded
   deterministically (e.g. xorshift64* seeded from a fixed constant, advanced
   in a fixed order at each spawn), substituting the native integer ranges
   exactly (`int(0, amount)`, `int(-var, var)`, `int(140,175)`, `int(0,10)`,
   `int(250,300)`, `int(130,150)`, `float(-5,5)`). Labels: the RANGES are
   OBSERVED; the seed/sequence is an ENGINE DECISION (native gives no parity
   obligation on which splash variant appears).
3. **Countdown integration (DER).** `PlaySession` currently starts audio at
   song position 0 with no countdown. 6H adds the observed 5-beat countdown:
   conductor ticks at `startTimestamp - 5 * beatLengthMs`, per-step sprite
   fade (`cubeInOut`, pixel `stepped(8)`) + fire-once stems at volume 0.6,
   music starts when songPosition reaches `startTimestamp` (respecting the
   §14 story-entry cancellation chain on senpai/roses/thorns/darnell and
   the dialogue `CUTSCENE_ADVANCE` flow). Pause/resume of the countdown
   timer integrates with the existing pause surface (M7 restart items stay
   named). The M4 conductor fixture `countdown-1ms-playback-1ms-v1` remains
   the timing authority.
4. **Audio generalization (DER).** `play/audio.rs` replaces the hardcoded
   `songs/bopeebo` path with the frozen resolution ladder: inst =
   `-{altInstrumentals id}` or `-{characters.instrumental}`; voices =
   declared vocals ladder / auto segment-strip / no-variation retry / bare
   fallback; missing tracks resolve to empty (spaghetti opponent vocals,
   blazin) rather than erroring; stems load per chart-id (`song` +
   variation) from `assets/songs/<song>/`.
5. **CLI (DER).** `PlayConfig` gains `song: String` (default `"bopeebo"`)
   and `--story`; `Difficulty` generalizes from the 3-value enum to the
   metadata-declared difficulty list (parse from `--difficulty`, validate
   against the chart's metadata like `build_for_song` already does).
   `--story` plays the level's chart sequence (level JSON, M7 surfaces the
   meta UI; the CLI keeps a minimal non-interactive path). 6E cutscene/
   dialogue payloads and 6G shaders flow through the existing
   `build_for_song(_with_dialogue)` loaders.
6. **Fixture impact (DER → APPROVAL).** Today NONE of these four families
   is rendered by `Scene`, so every `render-slice-2` family is untouched
   only if the ephemerals/countdown stay OFF the frozen fixture loaders.
   Two options requiring the human decision (§16.4):
   (a) gate the polish layers off on `build`/`build_for_song` (the dialogue
   and note-desaturation precedent) — zero fixture impact; or
   (b) enable splashes+popups on one NEW 6H family (or an existing one) and
   re-establish that family's digests under the ADR-0020 path.
   Covers follow holds, which the frozen families already exercise; enabling
   them changes frozen digests unless gated.

### 16.3 Engine decisions

1. **Fixed-size ephemeral pools + one WGSL-free material path.** Splashes,
   covers, popups and countdown sprites are plain tinted quads (no shader
   involvement); they reuse the existing `ColorMaterial`/`LayerShader::None`
   layer pipeline. Splash cap 6, digits bounded at 3, rating 1, covers
   bounded by live holds — no unbounded growth, no allocation churn
   per tick beyond existing refresh.
2. **Deterministic PRNG (fixed constant seed) is private** to the scene;
   the ranges match native. No new dependency (no rand crate).
3. **Countdown lives in PlaySession + a small scene state** (sprite fade),
   not in the sim trace: the sim/gameplay trace schema is NOT extended by
   6H (the sim's hit rows already carry the judgement needed to drive
   popups/splashes; countdown ticks are play-session scheduling, matching
   §14's precedent that cutscene scheduling stays out of the trace schema).
4. **`--song` accepts any corpus chart-id** (including variations like
   `darnell-erect`), reusing `split_song_variant` semantics; fail closed on
   unknown ids/difficulties with the same message style as the gates.
5. **No new dependencies**; `Difficulty` may become a string-backed parse
   validated against metadata (public surface change confined to
   `oxide_game::play` + `src/main.rs`).

### 16.4 Validation obligations & human approval items

- **Unit obligations (no fixture approval needed):** splash spawn table
  (sick-only, player-only, cap/eviction, position formula incl.
  INITIAL_OFFSET and splash offsets), cover state machine (start→loop on
  finish, end burst, drop kill, opponent immediate kill, sustain-follow
  absence), popup physics/fade constants (velocity/acceleration ranges,
  0.2 s fades, beat-relative start delays, 3-digit padding, `displayCombo(0)`),
  countdown step table (5 beats, step→sprite/sound mapping, no "3" graphic,
  volume 0.6, story-cutscene cancellation), and the full audio-stem ladder
  across the 61 chart-ids (incl. the spaghetti/blazin/monster edge cases and
  segment-strip fallbacks). Two-run sha256 determinism of the deterministic
  PRNG consumers.
- **Fixture policy question (HUMAN APPROVAL REQUIRED before implementation):**
  option (a) keep splash/popup/cover/countdown layers off the frozen
  `render-slice-2` loaders (zero digest impact, PlaySession-only visuals) —
  recommended as the default; or option (b) enable them on a designated
  family and re-establish that family's digests via the ADR-0020 path.
  This must be answered before 6H implementation lands.
- **Approval items:** (a) the fixture-policy decision above; (b) none other
  — no new dependencies, no tolerance changes, no asset changes are
  requested; `Difficulty` generalization is within the approved §6H write
  set.

## 17. Block-A native-parity fixes & 7th ADR-0020 re-establishment (2026-11-12)

Human-directed native-vs-engine comparison found five systemic bugs; all were
verified in code and native source before fixing, implemented across
`oxide-game`/`oxide-atlas`, and the affected engine fixtures re-established
under the ADR-0020 path after visual PNG inspection.

1. **Camera projection on `Sheet::Sprite` characters** (§11 amendment): the
   Sprite rig branch now composes `concat(projection, matrix)` exactly like
   the Atlas branch and the prop paths — Sprite-backed characters
   (`senpai`, `gf-pixel`, `bf-pixel`, `spirit`, `tankman-bloody`) follow
   stage camera pan/zoom.
2. **Native character anchor** (§11 amendment): the anchor is
   `stagePos − origin(idleDims@finalScale) + globalOffsets` (globalOffsets
   UNSCALED) with per-animation shift `−(animOffsets − globalOffsets)·scale`
   where omitted animation `offsets` default to `[0, 0]` (native
   `Bopper.applyAnimationOffsets` assigns null → coerced `[0,0]`;
   `Stage.hx:443-466` anchors from the idle animation; `setScale` re-anchors
   feet). Characters carry an `idle_size` (idle animation's first frame)
   replacing the first-document-frame/main-sheet-bounds basis.
3. **HealthIcon center-origin scaling** (§13 amendment): the icon draw
   matrix applies flixel's `updateHitbox`+`centerOrigin` chain — the
   previous top-left-origin scaling pushed pixel icons ~59-73 px below the
   bar and normal icons ~15 px into the score text during bops. §13's
   position algebra (POSITION_OFFSET 26, `y = bar.y − height/2`) is
   unchanged.
4. **Animate `M3D` matrices + `Clp` mask layers** (§11 amendment):
   `oxide-atlas` derives the 2D affine from element/`TRP` `M3D` 4×4 when
   `MX` is absent (native `FlxAnimateJson.hx:713-717` layout; MX keeps
   priority so the frozen bf/dad/gf atlas path is byte-identical), and
   `LT: "Clp"` mask layers never draw; masked quads carry the mask's AABB
   clip rect (native mask-before-clipped ordering verified: `Layer.hx:
   138-178`), consumed by the renderer through a new `LayerShader::Clip`
   discard pass with preallocated per-page clip slots and a named AABB
   approximation of native per-pixel masks. NOTE (brief correction): M3D
   translations do NOT match co-located `TRP x/y` in the shipped sserafim
   data (corpus-wide 690 matches / 80,080 mismatches); native uses the M3D
   translation only.
5. **DropShadow composed-character pass** (§15.3.4 supersedes the per-page
   approximation): characters owning `DropShadow` over atlas limbs render
   their limb layers into a per-group transparent 1280×720 offscreen target
   (existing offscreen-camera mechanism) and draw one output quad sampling
   it — the native `useRenderTexture = true` semantics
   (`DropShadowShader.hx:262-265`). Named approximations: `dist` is
   screen-pixel-scale (not native sprite-texture pixels) and the altMask is
   mapped over the character's current limb AABB.
6. **SserafimShader audit: zero findings** against `SserafimShader.hx` /
   `sserafim.hxc` / shipped `spaghetti-chart.json` payloads (HSBC, dark/light
   divisors, 0.07 light multiplier, `darkAmt > 0.65` flip, event wiring incl.
   `0xAARRGGBB` colors, per-beat selection).
- **Fixture re-establishment (7th ADR-0020 pass, human-approved):** 211
  frame digests across the six gameplay families + the `dialogue` family
  re-established after visual inspection of the generated PNGs (alignment,
  icon centering, no `#0000FF` mask rect, tankman `#DFEF3C` rim, coherent
  M3D characters, pulse-light tint). ALL trace digests UNCHANGED (the sim
  was untouched); only the `sserafim` geometry section changed (kazuha's
  M3D/mask-aware atlas bounds). Bopeebo `bf/dad/gf` atlas parses remain
  byte-identical (`MX`-priority).
- **Named limitation:** the scene camera-focus sampling keeps the frozen
  §12.7 sim algebra; native `resetCameraFocusPoint` derives from the
  corrected anchor, so camera-focus parity is a separate follow-up (it does
  not affect drawn characters).
- **Block-B decision:** `ABotSpeaker` (nene/otis-speaker speaker unit,
  visualizer bars, eye-look state) is scoped as its own follow-up slice
  after Block A stabilizes; it is a new component needing its own behavior
  freeze. The M8 HScript behaviors (background cars, running tankmen/Otis
  shooting, extra sserafim members, `dust1..dust4`) remain out of scope.

## 18. Block B — `ABotSpeaker` Speaker Unit Behavior Freeze

Status: **Behavior-frozen** before Block B Rust edits; supersedes §17's
deferral of `ABotSpeaker` (leaving M8 HScript extensions and Phase 6H out of
scope).

### 18.1 Observed Facts (Native Sources)

1. **Applicable corpus speaker characters** (`scripts/characters/{nene,nene-christmas,nene-tankmen,nene-dark,otis-speaker}.hxc`):
   - All five non-pixel A-Bot speaker characters share the exact same
     5-component Animate-Atlas speaker assembly attached to the `gf` stage
     character: `eyeWhites`, `stereoBG`, `pupil`, `abotViz`, and `abot`.
   - `nene-pixel` (`nene-pixel.hxc`, used on pixel `school`/`schoolErect`
     pico mixes) uses separate pixel-art assets (`aBotPixelBody`,
     `aBotPixelBack`, `aBotPixelSpeaker`, `abotHead`, `aBotVizPixel`,
     `aBotPixelSpeaker_mask`) and is **not** part of the Animate-Atlas
     `ABotSpeaker` slice (`render-slice-2` contains no `nene-pixel` family).
2. **Sub-layer composition and z-ordering** (`nene.hxc:188-215, 376-399`,
   `otis-speaker.hxc:58-84, 181-205`): relative to parent character's stage
   `char_z = stage.characters.<role>.zIndex`, final `setupAbot()` z-indices
   draw in strictly ascending back-to-front order:
   - `eyeWhites` (`makeSolidColor(160, 60)` opaque `#FFFFFFFF`): `zIndex = char_z - 20` (`abot.zIndex - 10`).
   - `stereoBG` (static PNG `characters/abot/stereoBG.png`): `zIndex = char_z - 18` (`abot.zIndex - 8`).
   - `pupil` (AnimateAtlas `characters/abot/systemEyes`, `applyStageMatrix: false`): `zIndex = char_z - 15` (`eyeWhites.zIndex + 5`).
   - `abotViz` (7-bar `ABotVis` group, Sparrow `characters/abot/aBotViz`, `viz{1..7}0`, antialiasing true, scale 1.0): `zIndex = char_z - 11` (`abot.zIndex - 1`).
   - `abot` (`ABotAtlasSprite`, AnimateAtlas `characters/abot/abotSystem` or `characters/abot/dark/abotSystem`): `zIndex = char_z - 10` (`this.zIndex - 10`).
   - Parent speaker character draws at `char_z` (in front of all five A-Bot
     layers). On `tankmanBattlefieldErect`, `gf.zIndex = 100` places A-Bot at
     `80..90` and stage prop `tankBricks` at `zIndex = 101` in front of both
     Otis and A-Bot (`tankmanBattlefieldErect.json:62-83`).
   - All five layers match parent visibility (`visible = this.visible`), use
     `scroll = [1.0, 1.0]`, `scale = [1.0, 1.0]`, `angle = 0.0`, and
     `flipX = false`.
3. **World placement and Block-A anchor alignment**
   (`Stage.hx:455-460`, `BaseCharacter.hx:277-288`, `Bopper.hx:322-341`,
   `nene.hxc:374-398`, `otis-speaker.hxc:181-205`):
   - `Stage.addCharacter` computes `this.x = stagePos[0] - idle_w * scale / 2 + globalOffsets[0]`
     and `this.y = stagePos[1] - idle_h * scale + globalOffsets[1]` (the
     exact `CharacterRig.anchor` computed by Block A).
   - Native computes `abot` top-left as:
     - Nene family (`nene`, `nene-christmas`, `nene-tankmen`, `nene-dark`):
       `abot.x = this.x - 95 - (-globalOffsets[0] * scale.x)`,
       `abot.y = this.y + 384 - (-globalOffsets[1] * scale.y)`
       (i.e. `anchor + data_offsets * scale + [-95.0, 384.0]`).
     - `otis-speaker`:
       `abot.x = this.x + 170 - (-globalOffsets[0] * scale.x)`,
       `abot.y = this.y + 455 - (-globalOffsets[1] * scale.y)`
       (i.e. `anchor + data_offsets * scale + [170.0, 455.0]`).
   - Child offsets relative to `(abot.x, abot.y)` (identical across all five):
     - `eyeWhites`: `(abot.x + 40, abot.y + 250)`
     - `pupil`: `(abot.x + 50, abot.y + 238)`
     - `stereoBG`: `(abot.x + 150, abot.y + 30)`
     - `abotViz`: group origin `(abot.x + 207, abot.y + 84)`; bars `0..7`
       (`viz10..viz70`) use step offsets `positionX = [0, 59, 56, 66, 54, 52, 51]`
       and `positionY = [0, -8, -3.5, -0.4, 0.5, 4.7, 7]` -> cumulative offsets
       `cumX = [0, 59, 115, 181, 235, 287, 338]` and
       `cumY = [0, -8, -11.5, -11.9, -11.4, -6.7, 0.3]` (`ABotVis.hx:30-49`).
4. **Body and pupil animation sync** (`ABotAtlasSprite`, `systemEyes`,
   `nene.hxc:197-203, 217-223, 267-358`, `otis-speaker.hxc:67-73, 123-131, 282-306`):
   - **Body (`abotSystem`):** Main timeline has **16 frames (`0..=15`)** at
     **24 fps** (`metadata.framerate: 24.0`), non-looped; frame `0` is
     non-bumping. Every beat-gated `dance()` restarts `abot` from frame `1`
     (`abot.anim.play("", true, false, 1)`), advancing `1..=15` and holding on
     `15`. Both `nene` and `otis-speaker` omit `danceEvery` -> default `1.0`
     (every beat).
   - **Pupils (`systemEyes`):** Main timeline has **32 frames (`0..=31`)** at
     **24 fps**, non-looped (`applyStageMatrix: false`, ignoring the top-level
     `StageInstance.Matrix3D` translation).
     - Frames `0..=15` glide center/right -> left; `16..=21` hold resting left
       (`m30 = -9, m31 = 17.35`); `22..=29` glide left -> right; `30..=31` hold
       resting right (`m30 = 0, m31 = -0.95`, visually identical to frame `0`).
     - Initial pose at scene start is frame `0` (paused, right-looking).
     - `FocusCamera` `char == 0` triggers `movePupilsRight()` (plays from
       frame `17` forward); `char == 1` triggers `movePupilsLeft()` (plays from
       frame `0` forward, pausing at frame `16` via `onFrameChange`); other
       `char` values leave pupils unchanged.
     - Additionally, `NeneBaseCharacter` (`nene`, `nene-christmas`,
       `nene-tankmen`, `nene-dark`, NOT `otis-speaker`) triggers
       `movePupilsLeft()` on `"weekend-1-lightcan"` and `movePupilsRight()` on
       `"weekend-1-cockgun"` in both `onNoteHit` and `onNoteMiss`.
     - Note on `nene.hxc:338-358`: `otis-speaker.hxc` omits Nene's redundant
       `onUpdate` `pupilState` pause check (`if pupilState == RIGHT && curFrame >= 17`)
       and pauses left at frame `16` via `onFrameChange` while letting right
       play `17..=31`. Both variants hold the identical resting-left frame `16`
       and resting-right pose `30..=31`.
5. **Visualizer bars (`ABotVis.hx:24-165`):**
   - `nene.hxc:214, 429` starts `abotViz.alpha = 0` and sets `1` on
     `SONG_START`; `otis-speaker.hxc` starts `alpha = 1`.
   - Every draw computes `animFrame = (volume == 0 || muted) ? 0 : round(level * 6)`
     and sets `group.members[i].visible = animFrame > 0`, selecting frame
     `abs(clamp(animFrame - 1, 0, 5) - 5)` (`viz{i}0000` tallest .. `viz{i}0005`
     shortest). When no audio FFT analyzer is present (`analyzer == null`),
     default levels are all `0.0`, so `animFrame == 0` and all 7 bars remain
     `visible == false`.
6. **Shaders and stage color variants** (`shaders.rs`, `nene*.hxc`,
   `otis-speaker.hxc`, stage `.hxc` files):
   - Notice Oxide `Hsbc::new(brightness, hue, contrast, saturation)` parameter order.
   - `nene` & `nene-christmas`: `synchronizeShader()` copies the parent
     character's stage shader (`character_shader(stage_id, Girlfriend, id, masks)`)
     to `abot`, `abotViz`, `stereoBG`, `eyeWhites`, while leaving **`pupil`
     with `LayerShader::None`**.
     - On `phillyStreetsErect`: `nene` gets `AdjustColor(-20, -5, -25, -40)`.
     - On `phillyBlazin`: `abot` additionally receives color tint `0xFF888888`.
   - `otis-speaker` & `nene-tankmen`: parent keeps its stage shader (e.g.
     `DropShadow` on `tankmanBattlefieldErect`), while A-Bot components override
     shaders independently (never copying `DropShadow`):
     - `abot`, `stereoBG`, `pupil`: `AdjustColor(Hsbc::new(-40.0, -40.0, -25.0, -20.0))`
     - `eyeWhites`: `LayerShader::None`
     - `abotViz`: `AdjustColor(Hsbc::new(-12.0, -30.0, 0.0, -10.0))`
   - `nene-dark`:
     - `abot` uses `characters/abot/dark/abotSystem` (geometry/timelines identical
       to `abotSystem`; swapped `spritemap1.png`), `LayerShader::None`.
     - `stereoBG` tint `0xFF616785`, `eyeWhites` tint `0xFF6F96CE`,
       `pupil` `LayerShader::None`, `abotViz` `AdjustColor(Hsbc::new(-12.0, -26.0, 0.0, -45.0))`.
7. **Adobe Animate verbose JSON format (`oxide-atlas`):**
   - `characters/abot/abotSystem/Animation.json`, `dark/abotSystem/Animation.json`,
     and `systemEyes/Animation.json` use unminified Adobe Animate JSON (`metadata`,
     `ANIMATION.StageInstance`, `ANIMATION.TIMELINE.LAYERS`, `SYMBOL_DICTIONARY.Symbols`,
     `Frames[].{index, duration, name, elements}`, `SYMBOL_Instance`,
     `ATLAS_SPRITE_instance`, and `Matrix3D` `{ m00..m33 }` objects whose
     column-major 2D affine projection is `[m00, m01, m10, m11, m30, m31]`).
   - `oxide-atlas::animate_stage::parse_stage` currently accepts only the
     minified BTA schema (`MD`, `AN`, `SD`, `L`, `FR`, `E`, `SI`, `ASI`, `MX`, `M3D`).

### 18.2 Derived Requirements

1. `oxide-atlas::animate_stage` must parse both minified BTA JSON and verbose
   Adobe Animate JSON (`metadata` / `ANIMATION` / `SYMBOL_DICTIONARY` /
   `Matrix3D` object) into the same `StageData` representation without
   altering existing minified atlas behavior.
2. When `gf` (or any loaded character) is one of the five Animate-Atlas
   `ABotSpeaker` variants, `StageScene` must insert the five A-Bot child layers
   at their exact `zIndex` offsets below the character (`char_z - 20`, `-18`,
   `-15`, `-11`, `-10`) so stage props such as `tankBricks` (`z=101`) sort
   between `gf` (`100`) and foreground characters (`200`) exactly as native.
3. A-Bot root world position must derive from the Block-A character anchor plus
   `(data_offsets * scale)` plus the variant's signed offset (`[-95, 384]` or
   `[170, 455]`), without disturbing Block-A character anchors.
4. `abot` body timeline must restart from frame `1` on each speaker `dance()`
   and clamp at frame `15`; `pupil` timeline must start paused at frame `0`,
   respond to `FocusCamera` `target_char == 0` (play `17..=31` / stop at `30..=31`)
   and `target_char == 1` (play `0..=16` / pause at `16`), and (for `nene*`
   variants only) respond identically to `"weekend-1-lightcan"` and
   `"weekend-1-cockgun"` on `Hit`, `OpponentHit`, and `Miss`.
5. `abotViz` bars remain `visible = false` when no audio FFT energy/analyzer is
   present (matching native `ABotVis.hx:152` headless / muted behavior), emitting
   no fabricated visualizer bars in `render-diff`.
6. Variant shaders and solid/sprite color tints must be applied per component as
   observed in §18.1 item 6, using the existing GPU `AdjustColor` pipeline.
7. Explicit Block-B non-goals remain untouched: `otis-speaker` combat
   (`muzzleFlash`, `shoot1..4`), `nene` train-hair reaction (`HAIR_BLOWING` /
   `HAIR_FALLING`), `nene` `normalChar` lightning crossfade, `nene-tankmen`
   `comboHeart`, `nene-pixel`, passing cars/running tankmen (M8), Phase 6H
   HUD/note visuals, and camera-focus math refactors.

### 18.3 Engine Decisions

1. **Unified parser in `oxide-atlas::animate_stage`:** parameterize field lookup
   across minified and verbose Animate JSON keys inside `animate_stage.rs`
   (including `Matrix3D` object -> `[m00, m01, m10, m11, m30, m31]` projection
   and integer-valued float `metadata.framerate` validation), keeping the public
   `parse_stage` / `AnimateStageError` contract cleanly intact.
2. **Variant-table driven `ABotSpeaker` in `oxide-game::render`:** introduce a
   single compact `ABotSpec` / `ABotRig` keyed by `(character_id, stage_id)`
   handling asset paths (`abotSystem` vs `dark/abotSystem`), root offset,
   body/eye/stereo tints, note-kind trigger enablement, and per-part `LayerShader`.
   No trait hierarchy or duplicate per-character modules.
3. **`eyeWhites` backing texture & drawing:** create a deterministic 2×2 opaque
   white `#FFFFFFFF` RGBA texture page scaled to `(160, 60)` (`makeSolidColor`
   equivalent) and reuse existing `StageLayer` / `prop_world_matrix` rendering
   primitives (incl. color tint modulation on solid/static/atlas parts).
4. **Headless `abotViz` representation:** load the `aBotViz` Sparrow frames and
   store the 7 bar positions/shaders in `ABotRig` while gating bar visibility
   on live spectrum levels (`0.0` in deterministic `render-diff`), avoiding
   arbitrary static bars that contradict native `ABotVis.hx:152`.
5. **Pupil playback target-range rule:** model `pupil` as a `systemEyes`
   timeline at 24 fps initialized paused at frame `0`:
   - `movePupilsLeft()` sets active range `[0..=16]` from frame `0`, advancing at
     24 fps and pausing at frame `16`.
   - `movePupilsRight()` sets active range `[17..=30]` from frame `17`,
     advancing at 24 fps and pausing at frame `30` (visually identical to `31`),
     avoiding the contradictory `nene.hxc:342` `pupilState` stall while
     matching both `otis-speaker.hxc` and intended Weekend-1 left/right motion.

### 18.4 Validation Obligations

1. `oxide-atlas` unit tests covering verbose Animate JSON parsing (symbols,
   nested frames, `Matrix3D` object affine extraction, float framerate, fail-closed
   errors on non-integer framerate / missing keys), leaving minified BTA tests
   untouched.
2. `oxide-game` targeted unit tests covering:
   - Z-ordering of all 5 A-Bot layers relative to `gf` and `tankBricks` (`z=101`).
   - Exact root and child anchor coordinates for `nene` (`phillyStreetsErect`)
     and `otis-speaker` (`tankmanBattlefieldErect`) preserving Block-A character
     positions.
   - Body dance frame `1..=15` restart/clamp on beat and pupil range playback on
     `FocusCamera` `0`/`1` and `weekend-1-lightcan`/`cockgun` (`nene*` vs `otis-speaker`).
   - Variant shader & tint assignments (`nene`, `otis-speaker`, `nene-dark`,
     `nene-tankmen`, `nene-christmas`, `phillyBlazin`).
   - Headless `abotViz` zero-level invisibility.
3. Single post-integration `cargo xtask render-diff`, visual PNG inspection of
   newly rendered `philly_streets_erect` and `tankman_erect` frames, ADR-0020
   re-establishment of **only** affected family frame/run digests in
   `tests/fixtures/render-slice/_manifest.json` (`trace_sha256` & `geometry`
   unchanged) recorded in `docs/DECISIONS.md` and `docs/PROGRESS.md`, followed
   by one clean `cargo xtask verify` and `tools/workflow_lint.py` pass.

