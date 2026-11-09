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
4. **Phasing:** six phases below; at most three workers; one heavy queue job;
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
