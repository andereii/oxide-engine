# M5 contract — bopeebo + mainStage + bf/dad/gf vertical slice (phase 1: behavior freeze)

Human-approved 2026-11-06 per the M5 kickoff (analysis + behavior freeze; no Rust
gameplay implementation in this phase). Compliance with `WORKFLOW.md` §2: sections
label each fact as **observed** (native source with line citations), **derived**
(invariant consequences) or decided (**M5 model/engine decision**; not observed).
Nothing in this contract claims full-game parity yet; the M4 native-probe boundary
(`tools/oracle/BEHAVIOR.md` + `tests/fixtures/conductor-native/`) still holds and
the acceptance evidence for every gate below remains pending until wired.

## 0. Observed corpus for the slice

- Chart files (read-only source assets, hashes tracked like the conductor probe):
  `assets/preload/data/songs/bopeebo/bopeebo-chart.json` with difficulties
  `easy` (88 notes), `normal` (102), `hard` (124) — 314 notes; holds exist in all
  three (easy 66, normal 58, hard 46 sustains). Universal note field set:
  `{t, d, l?, k?}` (`SongData.SongNoteData` fields: `t` L1052,
  `d` L1067, `l` L1074, `k` L1088).
  `bopeebo-chart-erect.json` (`erect` 532 / `nightmare` 558, 1090 notes) and
  `bopeebo-chart-pico.json` (`easy` 220 / `normal` 283 / `hard` 343, 846 notes)
  are OUT of the slice for this milestone (chart-id inventory from
  `tools/oracle/corpus.txt` maps `bopeebo-default/easy/nightmare`; nothing here
  removes them from M1/M4 gates).
  - Engine decision (recorded): the **slice acceptance manifest** covers all
    three default chart files' difficulties `easy`, `normal`, `hard` of the
    default chart (`bopeebo-chart.json`) — not the erect/pico variation charts.
  - Observed: `scrollSpeed` map `{easy: 1.2, normal: 1.3, hard: 1.6}` (chart L1
    top-level) — the same value applies to the player and opponent strumlines.
  - Observed: `bopeebo` metadata has a single time change (`t=0, bpm=100`),
    `offsets: {}` (empty map, still present as a key),
    `playData.characters` `{player: bf, girlfriend: gf, opponent: dad,
    altInstrumentals: [pico], opponentVocals: [dad], playerVocals: [bf]}`,
    `stage: mainStage`, `difficulties: [easy, normal, hard]`,
    `noteStyle: funkin`, `ratings: {easy: 1, normal: 1, hard: 2}`,
    `album: volume1`, and `songVariations: [erect, pico]`.
    - **Engine decision (recorded despite the present variations):** the
      phase-1 slice acceptance manifest excludes the `erect`/`pico` variation
      charts. Their exclusion is deliberate scope, NOT a data fact — the
      variation charts exist and the M1 gates already parse them.

- Stage file `assets/preload/data/stages/mainStage.json` (observed values):
  - props: `stageBack` `zIndex 10`, `position [-600, -200]`, `scale [1, 1]`,
    `scroll [0.9, 0.9]`, `assetPath stageback`, `animType sparrow`,
    `animations: []` (empty arrays in the JSON); `stageFront` `zIndex 20`,
    `position [-650, 600]`, `scale [1.1, 1.1]`, `scroll [0.9, 0.9]`,
    `assetPath stagefront`; `stageCurtains` `zIndex 30`,
    `position [-500, -300]`, `scale [0.9, 0.9]`, `scroll [1.3, 1.3]`,
    `assetPath stagecurtains`.
  - character slots: bf `zIndex 300`, `position [989.5, 885]`,
    `cameraOffsets [-100, -100]`; dad `zIndex 200`, `position [335, 885]`,
    `cameraOffsets [150, -100]`; gf `zIndex 100`, `position [751.5, 787]`,
    `cameraOffsets [0, 0]`.
  - `cameraZoom: 1.1` on the stage object.

- Character files (observed): `bf.json` is `multianimateatlas` with
  `assetPath: shared:characters/bf`, `flipX: true`, 16 animations (idle,
  singLEFT/DOWN/UP/RIGHT plus their `*miss` variants and the
  death/fakeout/scared categories; NO `*-hold` variants), `cameraOffsets:
  [-18, 36]`, `death` block. `dad.json` is `animateatlas` with `singTime:
  8.0`, 9 animations including `idle-hold` and the
  `singUP-hold`/`singLEFT-hold`/`singRIGHT-hold` variants (no
  `singDOWN-hold`). `gf.json` is `animateatlas`, `startingAnimation`
  present, dances on `danceLeft/danceRight`, 12 animations. No
  `usePlayerVocals` field exists on this checkout; the bf/dad vocal routing
  comes from the metadata (`playerVocals: [bf]` /
  `opponentVocals: [dad]`) plus the legacy voice handling.

- Corpus authorities (M1) still apply: parse-all inventories, stage/character
  data parsing registry contracts stay in force. Nothing here relaxes M1/M2 gates.

## 1. Observed gameplay mechanism (with citations)

### 1.1 The dispatch + hit window chain

- **Conductor** (M4 deliverable): verified against the frozen model AND the
  native hxcpp execution (62/62 timelines; 86131 rows). The gameplay loop consumes
  `Conductor.songPosition` (Conductor.hx L422-522) and
  `Conductor.getTimeWithDelta() = songPosition + songPositionDelta` (L523-526).
- `Constants.HIT_WINDOW_MS = 160.0` (Constants.hx L361). The hit window for a
  note is `{start: note.strumTime - 160, end: note.strumTime + 160}`
  (`GRhythmUtil.getHitWindow` L54-61). **That window is the only accept gate**
  observed on this path (PlayState's `processInputQueue` only tests
  `note.mayHit`, no per-frame re-checking of judgement tiers).
- `GRhythmUtil.processWindow(note, isControlled)` (`util/GRhythmUtil.hx`
  L69-112) flips the note states:
  - `songPosition > windowEnd` → `note.hasMissed = true; note.mayHit = false;`
    and (if a hold note) `holdNoteSprite.missedNote = true`;
  - `isControlled == false && songPosition >= windowCenter` → botplay hit;
  - otherwise, `songPosition >= windowStart` → `note.mayHit = true`;
  - otherwise (`tooEarly`) `note.mayHit = false; note.hasMissed = false`.
- `Strumline.getNotesMayHit()` (`Strumline.hx` L360-366) = the notes currently
  `alive && !hasBeenHit && mayHit`.
- **Observed input-latency handling in the concrete note score path:**
  `PlayState.goodNoteHit` (PlayState.hx L3108-3165):
  1. `inputLatencyNs = PreciseInputManager.getCurrentTimestamp() - input.timestamp`
     cast to ms (`/ Constants.NS_PER_MS`);
  2. `noteDiff:Int = Std.int(Conductor.instance.songPosition - note.noteData.time 
     - inputLatencyMs)` (L3118) — the `Std.int` truncation is part of the
     observed judgement; the conductor fixtures use `inputLatencyMs = 0`
     through the M4 inputs replay recipe;
  3. `Scoring.scoreNote(noteDiff, PBOT1)` + `Scoring.judgeNote(noteDiff, PBOT1)`.
- **Engine decision (documented):** for the M5 deterministic sim, the
  `inputLatencyNs` term is 0 by construction (the synthesized stream has no
  device latency); the same `Std.int(...)` interpretation captured by the M4
  replay gate carries over unchanged.

### 1.2 PBOT1 scoring (the game's default system)

Observed (`funkin/play/scoring/Scoring.hx`; PBOT1 constants L96-160, functions
  `scoreNotePBOT1`/`judgeNotePBOT1` L162-203):

```
PBOT1_MAX_SCORE      = 500        PBOT1_MISS_SCORE      = -100
PBOT1_SCORING_OFFSET = 54.99      PBOT1_SCORING_SLOPE   = 0.080
PBOT1_MIN_SCORE      = 9.0        PBOT1_PERFECT_THRESHOLD = 5.0
PBOT1_MISS_THRESHOLD = 160.0      PBOT1_KILLER_THRESHOLD  = 12.5
PBOT1_SICK_THRESHOLD  = 45.0      PBOT1_SHIT_THRESHOLD    = 160.0
PBOT1_GOOD_THRESHOLD  = 90.0      PBOT1_BAD_THRESHOLD     = 135.0
```

- `scoreNotePBOT1(msTiming)`: `absTiming > 160` → `-100`; `< 5` → `500`;
  otherwise `Std.int(500 * (1 − 1/(1 + exp(−0.080 × (absTiming − 54.99)))) + 9.0)`.
- `judgeNotePBOT1(absTiming)`: `<=45 → 'sick'`; `<=90 → 'good'`;
  `<=135 → 'bad'`; `<=160 → 'shit'`; else `'miss'`.
  **Observed:** the `killer` branch inside `judgeNotePBOT1` (12.5 ms) is
  COMMENTED OUT (Scoring.hx L188-190) — the judge outcomes are
  `sick | good | bad | shit | miss` and `HEALTH_KILLER_BONUS` (Constants.hx
  L471) is dead code on this path. Preserve the observable: **no 'killer'
  judgement is produced from `judgeNotePBOT1`**.
- Fallback systems (`LEGACY`, `WEEK7`) exist but are NOT selected on this
  path (`Scoring` default parameter is `PBOT1` everywhere in PlayState).
  No configuration key changes that default in the shipped build.

### 1.3 Health, combo, and the tally bookkeeping

- `GoodNoteHit.switch (daRating)` (PlayState.hx L3125-3139) maps judgements to
  `Constants.HEALTH_*_BONUS` and `JUDGEMENT_*_COMBO_BREAK`. Observed values
  (Constants.hx L455-522 + L553-573; observed combo-break values:
  sick false, good false, bad TRUE, shit TRUE, killer false):
  `HEALTH_MAX = 2.0`, `HEALTH_STARTING = 1.0`, `HEALTH_MIN = 0.0`;
  bonuses/penalties: sick `+1.5%` (`1.5 / 100 * HEALTH_MAX`), good `+0.75%`,
  bad `+0.0%`, shit `-1.0%`, hold-hit `+6.0%/s`, miss `-4.0%`,
  ghost press `-4.0%`, hold-drop `-0.0%/s` (the shipped game has the
  hold-drop penalty constants at ZERO — no per-second penalty) and mines
  `-15%` (bopeebo has 0 mines; out-of-slice rules but recorded).
- `applyScore` (PlayState.hx L3330-3360): tallies the judgement counter map
  (`sick/good/bad/shit/missed`), then `health += healthChange`; on
  `isComboBreak` the combo resets to 0 (a note display only when the previous combo is >= 10), else
  the combo increments and `maxCombo = max(maxCombo, combo)`; adds
  `songScore += score` per note.
- `onNoteMiss` (PlayState.hx L3171-3195): applies
  `Scoring.getMissScore()` (PBOT1 → `-100`), healthChange from the
  `MissNoteScriptEvent`, mutes the player vocal volume and plays a random
  miss-note sound (0.5..0.6 gain) when `playSound` is set.
- **Hold notes (sustains)** (PlayState.hx L2929-2998): the health/score
  branch runs ONLY when `holdNote.hitNote && !holdNote.missedNote &&
  holdNote.sustainLength > 0` AND `!isBotPlayMode && holdNote.scoreable`
  (botplay resets bf's hold timer only, L2939-2942 — no score/health).
  Per tick: `health += Constants.HEALTH_HOLD_BONUS_PER_SECOND * elapsed`
  (`+0.12` absolute units/s: 6.0/100 of HEALTH_MAX) and
  `songScore += Constants.SCORE_HOLD_BONUS_PER_SECOND * elapsed`
  (`250.0/s`; Constants.hx L538). Dropped holds
  (`holdNote.missedNote && !handledMiss` at L2961+) get the penalty path —
  observed constants: `HEALTH_HOLD_DROP_PENALTY_PER_SECOND = 0.0` (L512)
  and `HEALTH_HOLD_DROP_PENALTY_MAX = 0.0` (L517) — the HEALTH penalty is
  zero in the shipped build — while `SCORE_HOLD_DROP_PENALTY_PER_SECOND =
  -125.0` (L543) DOES apply (score penalty `remainingLengthSec * -125.0`),
  with `HOLD_DROP_PENALTY_THRESHOLD_MS = 160.0` (L548) gating
  when ends still trigger the penalty. The trace `hold_drop` event records
  the score penalty; the health change records 0.0 (the shipped constants).
  A completed hold re-press keeps the strum sound playing instantly.
- Ghost taps (anti-spam): with a direction press that has NO may-hit note
  (PlayState.hx L3055-3068 & `ghostNoteMiss` L3205-3237): after the
  `GhostMissNoteScriptEvent` is NOT canceled, `health += event.healthChange`
  (`HEALTH_GHOST_MISS_PENALTY = -0.08` absolute; `-4.0%` of `HEALTH_MAX`,
  Constants.hx L507) and `songScore += event.scoreChange`
  (`SCORE_GHOST_MISS_PENALTY = -10.0`, L532) apply UNCONDITIONALLY.
  `hasPossibleNotes` (the call argument `notesInRange.length > 0`) is event
  METADATA — some other direction may hold a may-hit note — and is NOT a
  penalty or sound gate. The miss-note sound (gain `random.float(0.1, 0.2)`)
  plays when `event.playSound` is true (default true on
  `GhostMissNoteScriptEvent`, ScriptEvent.hx L215-224; scripts may clear it);
  practice mode does NOT gate the sound in `ghostNoteMiss`.

- **Deletion/miss bookkeeping:** PlayState.hx L2999-3017 `handleSkippedNotes` —
  invoked from the resync paths (L2706-2709), NOT the ordinary frame loop:
  once `songPosition > note.strumTime + HIT_WINDOW_MS` and the note is
  not `hasBeenHit`, the note is flagged `handledMiss = true` (skipped without
  penalty). The note leaving the hit window DURING the frame loop is missed
  through the `processWindow` branch (PlayState.hx L2899-2919 process-notes
  loop: `if note.hasMissed && !note.handledMiss` → the `onNoteMiss` judge).
  No note gets judged twice; recycling happens when the missed note leaves
  the screen (Strumline L619-624, `handledMiss && isOffscreen`).
- **Same-frame ordering (observed):** the health clamp runs at the top of the
  PlayState update (PlayState.hx L1213-1215: cap `[HEALTH_MIN, HEALTH_MAX]`)
  and the death check runs at L1255 — both BEFORE the
  `processInputQueue`/`processNotes` calls (L1324-1332). Effect: events in one
  frame can push `health` past the bounds (or to/below zero), and only the
  NEXT update evaluates the clamp/death trigger; the trace therefore records
  `health_after` as the internal post-event value and the sim carries the
  same next-frame clamp/death semantics (engine decision: the sim applies the
  clamp + death check at the START of each `step` before consuming events,
  mirroring the observed order). Observed ORDER WITHIN a frame:
  `processInputQueue` (L1327) runs BEFORE `processNotes` (L1332) — the sim
  observes the identical processing order inside one `step`; the
  `getNotesMayHit()` set is computed from the PREVIOUS frame's
  `processWindow` state and the same-frame `processNotes` run applies the
  window/state changes afterwards. The 160/161 negative acceptance cases
  therefore run through this exact per-frame path in the acceptance trace
  (input-before-notes inside the step; clamp/death before both).

### 1.4 Game over

Observed (PlayState.hx L1251-1470): IN each update, after the health cap
(L1213-1215), the check at L1255 fires when
`health <= Constants.HEALTH_MIN && !isPracticeMode && !isPlayerDying` —
BEFORE `processInputQueue`/`processNotes` (L1327/L1332): the vocals pause
and `FlxG.sound.music.pause()`; `deathCounter += 1` happens BEFORE the
`GAME_OVER` script event dispatches (L1261 before L1266-1267);
`persistentUpdate = false`, `isPlayerDying = true` and the state opens
`GameOverSubState` (optionally after
`currentStage.getBoyfriend().getDeathPreTransitionDelay()`). The
health/score track (`songScore`) and `health = Constants.HEALTH_STARTING`
reset through `moveToGameOver` (PlayState L1438-1470: `songScore = 0.0`,
  `health = HEALTH_STARTING` and the icon/healthBar realignment before the
  substate opens). The R key
(`controls.RESET`) sets `health = Constants.HEALTH_MIN` before the collapse
check (L1241-1246, game-over check at L1255) and triggers a quick reset path.

- The game-OVER flow (substate, reset UI) is an M6/M7 UI deliverable; the M5
  slice requires the deterministic death transition up to the GameOver
  substate boundary recorded above (no pause UI requirements in the sim).

### 1.5 Note rendering geometry (upscroll default; the slice locks it)

Observed (`Strumline.hx`):
- `STRUMLINE_SIZE = 104` (L37), `NOTE_SPACING = STRUMLINE_SIZE + 8 = 112`
  (L42), `INITIAL_OFFSET = -0.275 * STRUMLINE_SIZE = -28.6` (L45),
  `BACKGROUND_PAD = 16` (L202) and
  `backgroundWidth = KEY_COUNT * NOTE_SPACING + BACKGROUND_PAD * 2 = 480`
  (L240: `4 * 112 + 32`), drawn via `makeSolidColor(Std.int(backgroundWidth))`
  (L247) and positioned with `background.x = -BACKGROUND_PAD = -16` (L251) —
  the painted span relative to the strumline origin runs `-16..464`.
- `get_renderDistanceMs() = FlxG.height / Constants.PIXELS_PER_MS / (scrollSpeed < 1 ? scrollSpeed : 1)`
  (Strumline.hx L64-72) where `Constants.PIXELS_PER_MS = 0.45` (Constants.hx
  L356). With `FlxG.height = 720` this gives e.g. 1600 ms at speed 1.0.
- `strumnote.y = this.y − INITIAL_OFFSET + GRhythmUtil.getNoteY(strumTime, scrollSpeed, isDownscroll=Preferences.isDownscroll, conductorInUse) + note.yOffset;`
  Strumline.hx L610-618.
- `GRhythmUtil.getNoteY(strumTime, scrollSpeed, isDownscroll, conductor) =
  0.45 * (conductor.getTimeWithDelta() − strumTime) * scrollSpeed *
  (downscroll ? 1 : −1)` (util/GRhythmUtil.hx L119-129).
- Notes outside the render window (before spawn) are not built; the build
  pass is `Strumline.updateNotes` (L570-610), walking notes up to
  `renderWindowStart = songPosition + renderDistanceMs` and stopping the
  spawn loop when `note.time > renderWindowStart`. Notes die past the screen
  via the L619-624 kill rules (`handledMiss && isOffscreen`; downscroll:
  `y > FlxG.height`; upscroll: `y < -note.height`).
- Hold notes render through a `SustainTrail` positioned by
  `holdNote.y = this.y − INITIAL_OFFSET ± ...` rules (Strumline.hx L630-750
  and the end-of-hold cleanup with `holdNote.missedNote &&
  songPosition >= renderWindowEnd` (`strumTime + fullSustainLength +
  HIT_WINDOW_MS + renderDistanceMs / 8`).

- **Engine decision (documented):** the M5 slice locks the config to the
  shipped-default preferences: `Preferences.isDownscroll = false` (upscroll),
  `noteSpacingScale = 1.0`, no custom render distance, no custom strumline
  scale, no practice mode, no botplay. These are engine-decision reductions to
  make the observed state traceable ONE configuration at a time; the
  underlying data never disallows different values (M7 save support).

### 1.6 Characters and singing/dancing surface (slice scope)

Observed (`bf.json`/`dad.json`/`gf.json`, CharacterDataParser):
- bf: `multianimateatlas` renderer, `shared:characters/bf`, `flipX: true`,
  16 animations (idle, singLEFT/DOWN/UP/RIGHT + the matching `*miss`
  variants plus death/fakeout/scared categories; NO `*-hold` variants),
  `cameraOffsets [-18, 36]`, `death` block present.
- dad: `animateatlas`, `shared:characters/dad`, 9 animations including
  `idle-hold` and `sing*`-hold variants, `singTime 8.0` (seconds), camera
  offsets `[11, 6]`.
- gf: `animateatlas`, `shared:characters/gf`, `startingAnimation` present,
  cycles `danceLeft`/`danceRight` as idle, 12 animations, camera offsets
  `[12, 8]`; `flipX` unset.
- Vocal routing comes from the metadata (`playerVocals: [bf]`,
  `opponentVocals: [dad]`) plus the legacy voice handling in `Song.hx` /
  `PlayState.hx` (no `usePlayerVocals` character field exists on this
  checkout).
- **Engine decision (documented):** the bf/dad/gf "sing/dance" animation
  bookkeeping is part of phase 2; phase 1 freezes only the OBSERVED data
  contract (character files + which renderType + the note pipeline). The
  render-diff gate for the visuals moves to phase 2 with its separate
  evidence step, and any animation sequence resolution needed by phase 2
  extends this file instead of hidden per-crate assume entries.
- **Open observation (owner: M5 phase 2):** the bf `multianimateatlas`
  machinery (per-frame deltas from the shipped module) has not been mapped
  yet; phase 2 must decide whether to read the module directly or translate
  it into the M2 atlas-type reader without changing the observable frame
  content.

### 1.7 Audio wiring

Observed in `PlayState`:
- `music.playing` drives the Conductor clamp semantics frozen at M4; the
  vocal object (`playerVocals: bf`, `opponentVocals: dad` from the metadata)
  mutes the player vocals on miss paths (`vocals.playerVolume = 0`, e.g.
  `onNoteMiss` L3179-3195 with the `tempVocals` bypass when bf holds its own
  voice) and restores on hits (`goodNoteHit` post-event `playerVolume =
  playerVocalsVolume`). The opponent vocal mute happens only inside the
  legacy voice path. The precise WAVEFORM verification of the V-Slice audio
  volume does not fit the M5 deterministic sim of this phase; phase 1 records
  the mute and unmute TICKS ONLY as a deferred (phase-2) acceptance
  obligation — no volume field belongs in the phase-1 trace schema — and the
  waveform measurement moves to the separate sequence with its own
  calibration inputs (M5/M6).

## 2. M5 acceptance gate definitions (defined in this phase; wired when
their implementations exist — they remain `pending` inside
`xtask/src/verify.rs` until then)

- `gameplay_sim_bopeebo` (NEW pending gate, milestone 5):
  the deterministic reimplementation maximally reproduces the observed
  mechanics with a DEFINED artifact. Required evidence for the M5 slice:
  - a state trace of the replay with the same exact 1 ms grid and offsets
    used by the conductor gate (fixture recipe `countdown-1ms-playback-1ms-v1`
    — the conductor timeline already proves the core time progression);
  - **event-driven trace records, each with a schema version string** — the
    phase-1 schema (this list IS the normative wire schema; serialization
    rules are under the synthesis bullet below) covers these events
    (player-path observables; booleans and keys snake_case):
    - `kind = hit|miss|ghost_press|hold_start|hold_tick|hold_drop|hold_end|
      game_over`.
    - **shared header on every event** (a serde JSON object in this exact
      key order): `schema` ("1"), `kind`, `sim` (u64, ms), `input_ms` (f64,
      0.0 for non-input events), `input_time_us` (f64; 0.0 in the canonical
      hashed trace — device kernel µs only travels in non-canonical
      evidence, see the digested-trace rule below), `note_id` (u32; 0 for
      `game_over`), `note_direction` (i64; 0 for `game_over`),
      `judgement` (`sick|good|bad|shit|miss|none`), `raw_note_diff` (f64,
      the signed `songPosition − note.t − inputLatencyMs` BEFORE `Std.int`;
      0.0 for non-note events) and `note_diff_int` (i32, the `Std.int`
      outcome; 0 for non-note events).
    - `hit|miss` add `score_delta` (i32: PBOT1 score or the -100 miss
      score), `health_before`/`health_after` (f64, absolute health units
      that MAY sit outside `[0, 2]` within the tick — the internal
      post-event value, clamped at the next sim tick per §1.4 ordering),
      `combo_before`/`combo_after`/`combo_max` (u32), and
      `tallies` with EXACTLY `{sick, good, bad, shit, missed,
      total_notes_hit}` (u32 keys in that order). `miss` events carry
      `judgement: "miss"`.
    - `hold_tick` carries `health_delta` (f64: `HEALTH_HOLD_BONUS_PER_SECOND`
      `0.12` absolute health units/s times the tick elapsed in seconds —
      the 1 ms recipe grid yields 0.00012 per tick) and `score_delta`
      (f64: SCORE_HOLD_BONUS_PER_SECOND `250.0`/s → `0.25` per tick),
      plus `hold_remaining_ms`/`hold_elapsed_ms` (f64);
    - `hold_drop` carries `score_penalty` (f64: the observed
      `remainingLengthSec * SCORE_HOLD_DROP_PENALTY_PER_SECOND (-125.0)`
      WITHOUT invented rounding), `health_change` (f64, 0.0 with the shipped
      zero constants), `combo_before`/`combo_after` (u32; the shipped
      `HoldNoteScriptEvent` hard-codes `isComboBreak = true`, PlayState
      L2961-2977, so `combo_after` records the reset to 0 or the current
      chain value), and `is_combo_break` (bool, shipped true);
    - `ghost_press` deltas live in the shared-header pattern with
      `score_delta`/`health_change` fields; `game_over` carries
      `death_count` (u32).
  - the input synthesis carries the M4 replay stream PROPERTIES on the 1 ms
    grid with the same chart-press basis and the same synthesis label
    (`input source: synthesized from the chart (no human replay yet)`),
    EXTENDED BY a scripted phase: 110 ms-late presses and 160/161 window-edge
    presses, sustain drops (release the key while the hold continues), plus
    one ghost press (a direction with no may-hit note in the window).  The
    M4 `StreamEvent` → `InputEvent` conversion is exact:
    `sim_ms: u64 = audio_time_ms` on the 1 ms grid (the M4 presses are
    ceil-quantized and the releases carry the SAME
    `audio_time_ms.ceil()` rule; a same-tick bind processes Press BEFORE
    Release, in the M4 order rank). Fractional sustain releases round UP to
    the next grid ms (an M4-grid property the M5 harness keeps deterministic
    rather than lossy — the conversion rule is frozen here).
    The M4 scoring stand-ins are NOT reused: M5 produces the real §1
    semantics.
  - the sha256-stable trace rule: a compact serde JSON in the §2 declared
    field order with `input_time_us = 0.0` in the canonical trace; the two
    runs per gate MUST be synthesized streams (a device stream is captured
    in separate, non-hashed evidence); `input_time_us` never enters the
    digest. The report prints the honesty labels (input source + the
    standard "not native-game parity" disclaimer).
  - **negative acceptance cases** (every case records the FULL-PATH trace
    under the frozen per-frame order: previous tick's `processWindow` state
    → `processInputQueue` → `processNotes`; the `judgeNotePBOT1` threshold
    list is the §1.2 citation — the trace checks event kinematics too):
    - judge thresholds (PBOT1, §1.2): a 110 ms-late press traces `bad`
      (NOT `shit`, NOT `sick`); 45 ms `sick`; 46 ms `good`; 90 ms `good`;
      91 ms `bad`; 135 ms `bad`; 136 ms `shit`;
    - **exact 160 ms press (full path)**: the previous tick's state still has
      `mayHit` (`processWindow` at 159 is not > windowEnd), so the
      `goodNoteHit` hit path runs: `kind=hit`, `judgement="shit"`,
      `note_diff_int=160`, `score_delta=9`
      (`Std.int(500 * (1 − 1/(1+exp(−0.080*(160−54.99)))) + 9.0)`),
      `health_after = health_before − 0.02` (the shipped
      `HEALTH_SHIT_BONUS`), `combo_break` (the shipped
      `JUDGEMENT_SHIT_COMBO_BREAK = true`), `combo_after=0`,
      `tallies.shit+1`;
    - **161 ms press (full path)**: the previous tick still leaves
      `mayHit=true` (the miss flag only sets when `songPosition >
      windowEnd`, and 160 is NOT `>` 160), so the press still runs the
      `goodNoteHit` HIT path where the `judgeNotePBOT1` outcome is
      `miss` — the `switch` over judgements (PlayState L3125-3139) has NO
      `miss` case, so `healthChange` stays 0.0 AND `isComboBreak` stays
      false while `applyScore('miss')` (L3330+) increments
      `tallies.missed` by 1 and INCREMENTS the combo (odd-but-true
      observed behavior; the trace must prove exactly this, not a
      cleaner `onNoteMiss` reshaping): `kind=hit`, `judgement="miss"`,
      `note_diff_int=161`, `score_delta=-100` (PBOT1 miss),
      `health_after == health_before`, `combo_after = combo_before + 1`
      (and `combo_max` updated if larger). The event REMOVES the note
      (`playerStrumline.hitNote(note, true)`) so no extra miss fires
      later;
    - a key press with NO may-hit note (ghost tap) applies the
      `HEALTH_GHOST_MISS_PENALTY=−4.0% of HEALTH_MAX (−0.08 absolute)` health
      change and the `SCORE_GHOST_MISS_PENALTY` = −10.0 score change
      UNCONDITIONALLY (up to the script event NOT canceling), regardless of
      `hasPossibleNotes` metadata (§1.3 rules) — one exact run proves it;
    - a hold released early with sustain REMAINING and
      `remaining_ms > 160` (strict `>` — PlayState L2956 — exactly 160 does
      NOT trigger the penalty) yields the observed hold-drop SCORE penalty
      `f64 = remainingLengthSec * −125.0`, and the shipped HEALTH hold-drop
      constants
      are ZERO (`HEALTH_HOLD_DROP_PENALTY_PER_SECOND = 0`,
      `HEALTH_HOLD_DROP_PENALTY_MAX = 0`), so the health after the drop is
      UNCHANGED by the drop (the base-hit penalty, if any, already applied at
      the corresponding note's judge). A sustain held to its full length
      yields `hold_end` with NO drop penalty (the `hold_tick` values already
      covered the score/health accrual).
- `native_note_reference` (NEW pending gate, milestone 5 — requires the same
  human approval workflow as the M4 conductor probe): a PlayState note-judge
  probe that compiles the REAL `PlayState`+`Strumline`+`Scoring` path
  (read-only, non-invasive slicing) against the bopeebo inputs. **Approval
  needed before implementation**: phase-1 merges without it, but M5
  acceptance stays blocked on `native_note_reference` alongside
  `render_diff`/`perf_budget` (the phase-2 visual gates). The M8
  `ruby_mods` gate is unrelated to M5 acceptance and only appears in the
  cumulative project gate list.
- `render_diff` (milestone 5, existing pending): phase-2 deliverable with a
  rendered slice on the bopeebo chart + mainStage; the fixture flow follows
  the same ADR as the conductor streaks (no silent regeneration).
- `perf_budget` (milestone 5, existing pending): the same deferred state.

## 3. Integration boundary for `crates/oxide-game`

`oxide-game` (Bevy integration) may only expose the following M5 phase-1
`pub` API surface (this is the freeze boundary; private representations are
free). Type OWNERSHIP: `InputEvent` and the trace/event records live in
`oxide-game`; `xtask` (or any harness) CONVERTS its own M4 `StreamEvent`
values into them — the M4 replay producer does not become a dependency of
the game crate, and `oxide-game` never owns the M4 CLI.

```pseudo-rust (non-normative shape; the NORMATIVE field list is §2. If a
        discrepancy is found between this sketch and §2, §2 wins, and the
        first compile must re-derive the trace types from §2 with a
        serde-serialized round trip that is sha256-stable across two runs)

// module oxide_game::slice — the M5 phase-1 gameplay sim (Bevy-free);
// no audio decode, no rendering, no save IO in this boundary.
/// One key-press/release transition consumed by the sim. xtask converts its
/// M4 StreamEvent{audio_time_ms, direction, difficulty, state} 1:1 per the
/// §2 conversion rule (ceil on the 1 ms grid; same-tick Press before Release).
#[derive(Debug, Clone)]
pub enum InputEvent {
    Press   { sim_ms: u64, direction: i64 },
    Release { sim_ms: u64, direction: i64 },
}
// §2 normative wire schema flattened to an event enum; NO `Eq` derive
// (f64 fields) — `PartialEq` for any in-sim deduping, derives `Serialize`
// with the §2 field order via `set_serializing_options`-style helper or
// field-order-preserving builder; the shared §2 header (schema/kind/sim/
// input_ms/input_time_us/note_id/note_direction/judgement/raw_note_diff/
// note_diff_int) is serialized FIRST on EVERY variant (no flagging).
pub enum TraceEvent { Hit { ... }, Miss { ... }, GhostPress { ... },
                      HoldStart { ... }, HoldTick { ... }, HoldDrop { ... },
                      HoldEnd { ... }, GameOver { ... } }
#[derive(Clone, Copy, serde::Serialize)]
pub struct TallyCounts { pub sick: u32, pub good: u32, pub bad: u32,
    pub shit: u32, pub missed: u32, pub total_notes_hit: u32 } // §2 exact list
pub struct GameplaySimState { /* private design */ }
#[derive(Debug, thiserror::Error)] pub enum SliceError { /* missing assets,
    unknown difficulty, parse errors, out-of-grid events */ }
impl GameplaySimState {
    /// Player-land bopeebo slice sim; the asset read path must reuse the
    /// M1/VFS data crates (oxide-data parse + oxide-vfs layer precedence) —
    /// raw path parsing outside the M1 crates risks parser divergence and
    /// layer-precision loss and is NOT frozen here.
    pub fn build(assets_root: &Path, difficulty: &str) -> Result<Self, SliceError>;
    /// One sim tick: clamp+death first (the §1.4 ordering), then the input
    /// events, then the window/process-notes updates. Returns only the
    /// trace events DISPATCHED during the tick (in §2 canonical order).
    pub fn step(&mut self, sim_ms: u64, events: &[InputEvent]) -> Vec<TraceEvent>;
    pub fn record(&self) -> Vec<TraceEvent>;
}

```

- `oxide-game` MUST NOT depend on `oxide-media`/`oxide-tools`/`xtask`
  (runtime crate: offline tooling stays split) and MUST NOT duplicate the
  conductor — `oxide-audio` is the single conductor provider (phase-2 Bevy
  integration consumes the same one). Bevy lands in phase 2 with its own
  representations; the sim above is meant to be the M6 render/anim source
  of truth for the note events the sprite layer consumes.
- Phase-2 extension rule (frozen): phase 2 may ADD `TraceEvent` variants or
  ADD fields in a NEW trace `schema` version; it may NOT change or remove
  the phase-1 variants/field semantics. The opponent side (dad auto-hit and
  sing schedules) is EXPLICITLY excluded from the phase-1 trace; phase 2
  derives it from the same chart notes without altering player event
  semantics.
- A trace `schema` value is embedded (`"schema": "1"`+); the two-run sha256
  equality requires the schema + serialization to match; a bump is a fresh
  evidence boundary not a silent change.

## 4. Contracts with the engine milestones

- M4 gates stay in their current states; bopeebo slice acceptance depends on
  them but does not insert so the M4 obligations stay required (the slice
  gameplay bumps the M4 acceptance along the same way the conductor gate
  reported its own coverage).
- The M5 phase-1 sim does not render, does not decode audio at runtime, and
  does not save any files. No scope creep: the sim covers ONLY the slice
  observables (no ads, no dialogs, no cutscenes, no menus, no save IO).

## 5. Frozen pieces not included in the M5 phase

- The Story Mode / level runner, freeplay, save files, dialogues, cutscenes,
  video subtitles, censorship variants, the whole M6 story-mode runner, pause
  menus and any UI beyond the health-bar semantics above (all documented for
  their milestones).
- The precise per-frame visual rendering of the slice (renderDiff) — the
  phase-2 deliverable with its own gate; this contract freezes the
  STRUCTURAL semantics the renderer must obey.

## 6. Honesty boundary summary
This contract does NOT certify:
- that the M5 rule implementation matches the original beyond the sections
  frozen here (`native_note_reference` pending; the `render_diff`/`perf_budget`
  acceptance of the vertical-slice visuals/latency pending);
- hardware latency behavior, real device precision (M4 input-jitter
  labels), audio clock tie-ins;
- parity of ALL game modes (any mode outside the frozen slice is unverified
  unless a separate findable/citable gate wire proves it).

## 7. Corrigendum (2026-11-07, native-probe evidence; recorded via ADR-0019)

Native-probe evidence (`tools/oracle/NOTE-BEHAVIOR.md` §6.1, frozen rows in
`tests/fixtures/note-native/`) corrects two over-generalizations in the §2
trace wording; the wire schema and the phase-2 add-only rule are unchanged:

1. **hold_drop combo semantics.** §1.3/§2 describe the dropped-hold event with
   "the shipped HoldNoteScriptEvent hard-codes isComboBreak = true". The real
   event is constructed ONLY inside the `remaining > HOLD_DROP_PENALTY_MS`
   branch (PlayState.hx L2956-2977): a sub-threshold drop dispatches nothing,
   applies no score/health change, and breaks NO combo. Trace rows still
   record a `hold_drop` row for every released sustain; its fields follow the
   branch that ran (`is_combo_break=false`, `combo_after == combo_before`,
   zero penalty for sub-threshold drops). The penalizing-branch wording
   ("shipped true") remains true for that branch only, not for all drops.
2. **total_notes_hit on misses.** The real `applyScore('miss')` path never
   touches `total_notes_hit`; the increment lives in `goodNoteHit` only
   (PlayState.hx L3152-3153). The `tallies` field list in §2 is unchanged, but
   the sim no longer bumps `total_notes_hit` on a windowed miss; the 161 ms
   odd-but-true HIT-path judgement-miss keeps its bump (goodNoteHit runs).

Also recorded (evidence, not schema change): `hold_drop.input_ms` records
0.0 (non-input event per §2's own input_ms rule), and the sim no longer
bumps `total_notes_hit` on a windowed miss while the 161 ms odd-but-true
HIT-path judgement-miss keeps its bump.

3. **Hold mirror ordering (round-3 correction; supersedes the corrigendum's
   earlier "deferred one tick" wording).** The real frame order is:
   `PlayState.update` calls `super.update` FIRST (PlayState.hx L1035), so the
   `Strumline` hold block (Strumline.hx L627-760) runs each frame with the
   PREVIOUS frame's conductor position; the natural chain order inside it is
   drop detect (L632-642) -> completion (L653-684) -> keep-clipped
   (L685-686) -> recompute (L721-727, `sustainLength = (strumTime +
   fullSustainLength) - previousSongPosition`). The penalty branch
   (PlayState.hx L2956-2977) reads the STALE snapshot at the same tick the
   drop flag fires: for a release at R, the stale value is `end - (R-1)`, so
   the trace row `hold_drop` (input_ms 0.0) is emitted at release+1 with the
   stale-derived penalty. Consequences for the §2 wording:
   - the §2 sentence "a hold released early ... exactly 160 does NOT trigger
     the penalty" described the PROJECTED remaining; the real branch tests
     `stale > 160` where `stale = (end - R) + 1` on the recipe grid, so the
     recipe's `ceil(t+l-160)` release (projected remaining exactly 160.0)
     PENALIZES (stale 161, score `-20.125`, combo break) and the zero-penalty
     boundary case is the release at `ceil(t+l-159)` (stale EXACTLY 160.0,
     bitwise; zero penalty, combo unchanged). Both cases are now mandatory
     fail-closed checks.
   - the §2 sentence "a sustain held to its full length yields hold_end with
     NO drop penalty" is likewise a projected reading: the recipe releases
     every sustain at `ceil(t+l) = end`, and the real chain flags that drop at
     the next mirror (stale = 1 ms, "Hold note too short, not penalizing..."),
     so the trace records a zero-penalty `hold_drop` at end+1, and `hold_end`
     rows exist ONLY when the key stays held past the sustain end
     (completion at the first integer tick >= end+2; exercised by unit/toy
     coverage, not by the recipe streams).
   - the accrual snapshot the real branch observes is the frame-START mirror
     value (two consecutive full snapshots at the hit tick and the first
     recompute; the last accrual row at ceil(end); never a negative
     remaining; no fractional partial row).
4. The `game_over` kind is unreachable in the native probe (death/clamp
   ordering lives in the unretained `update()`), so its frozen rows never
   exercise it; the §1.4 ordering stays an engine decision.

## 8. Phase 2b (approved 2026-11-07): visual slice semantics pointer + corrigendum

- Render/animation scene semantics are frozen in `tools/oracle/RENDER-BEHAVIOR.md`
  (OBSERVED/DERIVED labels) and ADR-0020; `render_diff`/`perf_budget` definitions live
  in RENDER-BEHAVIOR §7. This section adds no new trace schema variants — scene events
  are consumed from the §2 trace; the §3 add-only rule extends per schema-bump rule.
- **Corrigendum (observed units):** §0's "dad `singTime: 8.0`" recorded the data value
  with an "(seconds)" gloss taken from the earlier read; the real consumption is
  `singTimeSteps * conductor.stepLengthMs / 1000` (BaseCharacter.onUpdate L389-471) —
  the field is treated as STEPS, i.e. dad `8.0` = 8 steps = 1.2 s at BPM 100. At
  bopeebo's single time change this divergence never changes any animation boundary in
  the frozen frames, but the unit is now pinned by RENDER-BEHAVIOR §5.
- Phase-2b EXCLUSIONS (scope decisions, ADR-0020 addendum 3): chart song events
  (`FocusCamera`/`PlayAnimation`) and hold covers/note splashes; dropped player
  hold trails are hidden per the observed alpha-0 path; bf/gf `singTime` default is
  the observed 8.0 steps. Visual-gate honesty text carries each exclusion.
