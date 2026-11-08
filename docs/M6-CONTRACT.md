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
  names, `sserafimShow/Sing` array-valued payloads) exists in this checkout but
  is NOT in `corpus.txt` — it is outside the measured acceptance inventory
  (observed). `oxide-data` already records unknown event names (`events.rs:
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
- One heavy queue job; `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0` per the
  recorded bounded-RAM policy.
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
