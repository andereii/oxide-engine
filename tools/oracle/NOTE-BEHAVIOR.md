# native_note_reference — behavior specification for the native hxcpp note probe

Human approval 2026-11-06: build and wire a read-only native hxcpp probe for
`native_note_reference`, following the ADR-0014/0017 non-invasive approach, without
modifying the original Funkin' checkout, to validate `oxide_game::slice` scoring,
judgement, window and hold semantics against the real Haxe classes. This file
specifies the probe; representative of the conductor-probe separation rules
(`BEHAVIOR.md`), NOT a claim of formal clean-room or full-game parity.

Units: all sim/step times are milliseconds on the 1 ms grid; conductor state stays the
M4 native probe semantics; `elapsed` for the hold bonus is 1/1000 s per tick (the
frozen M5 recipe). No unit is converted silently.

## 1. Observed vs derived vs decided

- **Observed (compiled from the real files, bodies untouched):** see §2 retained list.
  Each stub/decision is separately labelled below with citations.
- **Derived:** consequences frozen in `docs/M5-CONTRACT.md` §1/§2 (per-frame order,
  odd-but-true 161 ms path, zero hold-drop health constants, unconditional ghost
  penalties). The probe may falsify derived statements for the covered path; it cannot
  observe what is not compiled.
- **Engine decisions / model assumptions:** stream synthesis (§4), zero input latency
  term, `elapsed` = 1/1000 s, note/hold lifecycle construction (§5), fail-closed
  row emission (§6). A source-informed probe is not an independent full-game oracle;
  real-game integration traces remain a separate acceptance obligation.

## 2. Retained real code (compiled from the installed checkout, read-only)

Byte-exact copies under a generated `game-copy` tree (hashes recorded); macro build
metadata whitelists FIELDS, never retypes or edits a retained body:

- `funkin/play/scoring/Scoring.hx` — the `ScoringSystem` dispatch surface
  (`scoreNote`/`judgeNote`/`getMissScore`) and all PBOT1 constants/functions +
  whichever other static functions those bodies call (LEGACY/WEEK7 bodies may be
  retained or whitelisted away, but the default-parameter PBOT1 paths must be the real
  functions). The shipped default is PBOT1 everywhere on this path; no retained body
  may be modified. The killer branch stays commented out — preserving the observable
  that `judgeNotePBOT1` never yields `'killer'`.
- `funkin/util/GRhythmUtil.hx` — all static functions retained
  (`mirrorNoteDirection`/`getHitWindow`/`processWindow`/`getNoteY`).
- `funkin/play/PlayState.hx` — the note-score frame path: `processInputQueue`,
  `processNotes`, `goodNoteHit`, `onNoteMiss`, `ghostNoteMiss`, `applyScore`, plus
  every field those retained bodies read or write (`health`, `songScore`,
  `isPracticeMode`, `isBotPlayMode`, `isInCutscene`, `disableKeys`,
  `inputPressQueue`, `inputReleaseQueue`, `playerStrumline`, `opponentStrumline`,
  `vocals`, `playerVocalsVolume`, `opponentVocalsVolume`, `currentStage`, …). The
  transitive closure is the whitelister's job; the probe fails to compile if a
  retained body references a dropped field.
  NOT retained: `update()` and everything else — the runtime clamp/death ordering
  (PlayState.hx L1213-1215/L1255) is therefore NOT a probe observable (the sim
  ordering stays an engine decision; trace field `health_after` records the internal
  post-event value).

## 3. Stub and shadow inventory (each labelled: [V]erbatim copy with citations |
   [S]hell with documented contract | [D]ecision)

- `Conductor` — reuse the proven `ConductorSlice` pattern ([V] retained timing path).
  `Conductor.instance` is the probe conductor ([D]).
- `flake.flixel.FlxG`/`FlxSound`/sound stubs — existing probe stubs where importable
  ([S]; e.g. `FlxG.log` swallows, `FlxG.random` unused default paths, `FunkinSound`
  playOnce no-op).
- `funkin.Highscore` — real file or [V]-copied shape: `Tallies` class
  (sick/good/bad/shit/missed/combo/maxCombo/totalNotesHit) and `Highscore.tallies`
  ([V] with citation; only the state surface the retained bodies touch).
- `funkin/input/PreciseInputManager` — [S]: `getCurrentTimestamp()` returns the probe
   event timestamp so `inputLatencyNs` evaluates to exactly 0; `PreciseInputEvent`
   typedef kept [V] (noteDirection/timestamp/keyCode).
- Script events (`HitNoteScriptEvent`, `NoteScriptEvent`, `GhostMissNoteScriptEvent`,
  `HoldNoteScriptEvent`) — real data classes if their closure is small, else [V]
  field copies with citations. Do NOT invent event fields ([V]/[S]).
- `PlayState.dispatchEvent` (override at PlayState.hx L1511 or its base) — [S]
  instrumentation: records a pending row (§6) and returns the event NOT canceled (no
  mods on the probe). Only Sanscript defaults apply.
- Note/strumline surface shadows (LAST classpath wins, per the documented empirical
  rule): `funkin.play.notes.NoteSprite` [V]-field data holder: `noteData`/`strumTime`
  get/set ([V] bodies verbatim), `direction` (set body [V] after dropping the
  `frames==null` guard is NOT allowed — keep the body; `frames` stays null in the
  probe so the getter short-circuits, an [D] documented), `hasBeenHit`/`hasMissed`/
  `tooEarly`/`mayHit`/`lowPriority`/`scoreable`/`isHoldNote` ([V]), `holdNoteSprite`.
  `funkin.play.notes.SustainTrail` [V]-field data holder: `alive`/`hitNote`/
  `missedNote`/`handledMiss`/`sustainLength`/`scoreable`/`noteData`.
  `funkin.play.notes.Strumline` shadow: `KEY_COUNT` [V]; `getNotesMayHit` body [V]
  (real body over `notes.members`), `mayGhostTap` [V or S per the real body],
  `pressKey`/`releaseKey`/`playPress`/`playConfirm`/`playStatic`/`hitNote`/
  `playNoteSplash`/`playNoteHoldCover` [S] shells each citing the real body's
  observable semantics they must preserve for the probe path: `hitNote` must at
  minimum set `note.hasBeenHit`, `note.alive=false`, and for a hold note
  `holdNoteSprite.hitNote=true`; others are animation surfaces with no slice
  observables ([D]: excluded from trace).
- `currentStage` [S]: `getBoyfriend`/`getOpponent`/`getDad` return shells with
  `playSingAnimation`/`isSinging`/`holdTimer`/`tempVocals`; opponent `notes`/`holdNotes`
  containers are empty for the probe ([D]: opponent side excluded).
- `controls` [S]: the NOTE_*_P getters used by `onNoteMiss`/`ghostNoteMiss` return
  false (playSound paths excluded from the slice; recorded as excluded).
- `Constants` — [V] the full referenced constant set copied with citations (extends the
  M4 6-constant set with the health/score constants used by the retained bodies).

## 4. Stream input (the probe consumes, never synthesizes)

The M5 gate exports the exact synthesized input stream it replays (bopeebo
easy/normal/hard, §2 recipe: presses at `ceil(t)`, releases at `ceil(t+l)`, scripted
negative phase incl. 110/160/161 presses, early sustains, one ghost press) into a
jsonl stream manifest consumed by BOTH the Rust comparator and the Haxe probe. The
manifest carries per-tick `sim_ms`, the input events for that tick, and the
`Conductor.update(musicTime)` sequence the Rust sim used, so the probe driver replays
the identical states instead of re-deriving the recipe. The frozen stream files live
with the frozen native rows after review; sha256 chain recorded. The probe must
fail-closed on absent/short/extra ticks ([D]: no recipe re-derivation, no
interpolation). Frozen interface (the ONLY supported format, both consumers pin it):

- Export location: `target/note-native-export/<difficulty>.stream.jsonl` (one file per
  difficulty; never committed directly — copied into `tests/fixtures/note-native/`
  only by the review-approved freeze step).
- Line 1 (header): `{"schema":"note-stream-1"}` then string fields `difficulty`
  (`easy|normal|hard`), `chart_id` (`bopeebo-default`), `tick_count` (u32, total tick
  objects), `songpos_source` (short provenance string, e.g. the recipe label).
  Any consumer receiving a header whose `schema` differs fails closed.
- Following lines, one per tick in ascending `sim` order:
  `{"sim":u64,"music_ms":f64,"events":[...]}` where `music_ms` is exactly the value
  applied to the conductor state the Rust sim consumed for this tick and `events` is
  `{"kind":"press|release","direction":i64,"input_ms":u64}` with `input_ms` equal to
  the `InputEvent` `sim_ms` (the ceil-quantized M4 conversion; the gate asserts
  consistency between `sim` and the event `input_ms` — an event input_ms below its
  tick's `sim` is an export bug and fails closed). 1:1 conversion InputEvent switches
  `kind` by the sim's Press/Release value.

## 5. Note/hold object lifecycle ([D], labelled)

Notes and hold trails are constructed by the probe driver from the real bopeebo chart
JSON (probe reads the asset file directly; read-only) using the real retained
properties. Note list order = chart array order for the difficulty, matching the Rust
sim's documented order; any divergence is a comparison failure, not a silently sorted
match. Opponent notes are not constructed in round 1 (round 2 extends this: §10).
Note `alive`/`n` bookkeeping outside the retained bodies is the driver's (engine
decision).

## 6. Row emission fail-closed rule

`dispatchEvent` instrumentation pends one row per event; the driver flushes a pending
row when (a) the next instrumentation point fires, or (b) the retained call returns.
Result unless one stateful dispatching event per tick: **fail-closed** — the probe
aborts with a diagnostic naming sim_ms and the events seen (a multi-event tick has no
defensible per-event health/combo decomposition from a post-call state). The comparator
labels excluded ticks in the report; none expected under the frozen recipe. Final
fields (`health_after`, `combo_after`, tallies) come from REAL state
(PlayState field / Highscore.tallies) observed at the flush.

Row schema = `docs/M5-CONTRACT.md` §2 normative wire fields, byte-order identical to
the Rust serializer (`input_time_us` = 0.0 in the canonical trace). Honesty labels
(synthesized stream; not native-game parity; excluded event kinds) print with every run
and live in the gate report. Round 2 amends the multi-event rule only for
`opponent_hit` rows (§10.2/§10 below); the player-path rule here is unchanged.

### 6.1 REVISED shared hold-row regime (2026-11-07 round-3 freeze)

The first §6.1 freeze asserted a frame-END ordering ("Strumline holds run after
processNotes"), citing `PlayState.update`'s custom logic before `super.update`.
That reading is FALSE on this checkout and was caught in review: `PlayState.update`
calls `super.update(elapsed)` FIRST (PlayState.hx L1035); the strumline member
update (and `Strumline.updateNotes`'s hold block, Strumline.hx L627-760) runs with
the PREVIOUS frame's conductor position; the playing-phase conductor update sits
later in the same frame (PlayState.hx L1142-1189); `processInputQueue` (L1327)
then `processNotes` (L1332) close the frame. Everything below re-freezes the
regime on the real ordering; the first freeze's rows must be re-derived from it
(see items 8-9).

Grid terms: T is the current tick's sim ms, P = T-1 is the mirror's songPosition
(the conductor value from the previous frame's update), `end = strumTime +
fullSustainLength`, snapshot value = the hold's `sustainLength` field as last set.

1. Mirror (super-equivalent, BEFORE the conductor update and input/notes of this
   tick), per alive holdNote, in the real chain order (Strumline.hx L632-745):
     a. drop detect (L632-642): `P > strumTime && hitNote && !missedNote &&
        !isKeyHeld(dir)` -> missedNote = true; the snapshot value is NOT touched
        (the L685 keep branch preserves it).
     b. completion (L653-684): `hitNote && snapshot <= 0` where the snapshot the
        L653 branch sees at frame T is the value set at frame T-1, i.e.
        `end - (T-2)`; the hold is killed (no recompute this frame). THE
        `hold_end` ROW IS THE DRIVER'S ONLY ROW HERE and its sim is T.
     c. keep-clipped (L685-686): dropped-keep branch; no state change.
     d. recompute (L721-727): else-if `P > strumTime && hitNote` (not dropped,
        not completed) -> `sustainLength := (strumTime + fullSustainLength) - P`.
2. Conductor update to songPos T (retained ConductorSlice update call).
3. processInputQueue (retained): releases/presses update `held` used by the NEXT
   mirror's L632 check (a same-tick press-before-release nets held=false for the
   next mirror).
4. processNotes(1/1000) (retained):
   - accrual branch (PlayState.hx L2931-2935) for holds with
     `hitNote && !missedNote && snapshot > 0` where the snapshot is THIS tick's
     mirror value (1.d; before the first recompute it is the hitNote capture
     `min(full, end - songPosition_at_hit)` with songPosition = strumTime, so
     snapshot(T0) = full). Row `hold_tick` per branch entry: `health_delta` =
     0.12/1000 fixed, `score_delta` = 250/1000 fixed (NO fractional partial);
     `hold_remaining_ms` = the observed snapshot at the branch (NOT a projected
     `end - T` formula); `hold_elapsed_ms` = T - T0.
   - the penalty branch (PlayState.hx L2956-2977) runs on a hold flagged at THIS
     tick's mirror: penalizing iff snapshot > HOLD_DROP_PENALTY_THRESHOLD_MS
     (160.0 strict `>`); the real `HoldNoteScriptEvent` DISPATCH carries
     score/healthChange/isComboBreak and the row fields are read FROM the
     dispatch instrument (fail-closed if they differ from the driver formula
     `-125.0 * snapshot / 1000`; a hard-coded wrong score must fail the probe).
     Sub-threshold drops record the zero-penalty row (is_combo_break=false,
     combo unchanged, no event) as before.
5. `hold_start` at the base-note hit tick. Row/list sequencing: dispatch rows of
   processInputQueue, then (within processNotes) the player-note rows, then the
   hold rows (lane order); the <=1 stateful dispatch event per tick rule and the
   flush rule (§6) are unchanged.
6. Schema unchanged. ADR-0018's mandatory boundary case keeps its SAME release
   formula `ceil(t + l - 160)`: under this regime its snapshot is
   `end - (R-1) = 161` ms and the real branch PENALIZES — the prior expected
   zero-penalty outcome was a model misreading of the frame order (ADR-0019
   round-2 note). The NEW no-penalty boundary case is release
   `ceil(t + l - 159)`, whose snapshot is EXACTLY 160.0 and must record
   `score_penalty = 0.0`, `is_combo_break = false`, combo unchanged. Stream
   exports change accordingly (the negative scripted phase gains the second
   boundary case) and all frozen artifacts are re-derived once, after
   integration, by the orchestrator.
7. Formula notes: snapshots per frame: T0 = full (capture); T0+1 =
   `end - T0` = full as well (recipe holds hit exactly at strumTime: two
   consecutive full values then a decay by 1 per tick); the LAST accrual row
   sits at T = ceil(end) with snapshot `end - (ceil(end)-1) > 0`.
8. `hold_end` row at the completion tick = the first integer T >= end + 2
   (i.e. ceil(end) + 2 for integer ends, ceil(end + 2) for fractional ends), per
   items 1.b/4 (the completion branch sees the PREVIOUS mirror's value and
   follows the frame after the last accrual despite the accrual branch seeing a
   > 0 value at the LAST recompute). This shifts the hold_end row two ticks
   later than the first freeze's model; the counts of hold_tick rows are
   UNCHANGED (T0..ceil(end) inclusive). The negative-hold_remaining oddity of
   the first freeze (a last `hold_remaining_ms` of -0.5 at a fractional end)
   never existed in the real branch — the stale value 0.5 is what the real code
   observes.
9. Release semantics (PRECISION FIX round-3; the L632 flag check runs FIRST in
   every mirror, before the completion check): a release at R whose R <=
   ceil(end) with the snapshot still > 0 flags the drop at the mirror of R+1
   (P = R > strumTime) and produces the drop row AT R+1 with the stale
   snapshot `end - (R-1)` (zero penalty when stale <= 160: the retained
   "Hold note too short, not penalizing..." branch; the completion L653 check
   does NOT take precedence there because its condition snapshot <= 0 is
   false for stale > 0). For a FRACTIONAL end the recipe's release
   `ceil(end)` sits between end and end+1: the snapshot at that mirror is
   still > 0 (e.g. 0.5), the flag fires at ceil(end)+1, and the zero-penalty
   row appears there. Only a release at R >= end+1 INT (integer: the
   natural decay drove the recompute value to <= 0 by the mirror of end+1)
   reaches the completion path: the mirror at the first integer T >= end+2
   completes (L653 sees the <= 0 value; the processNotes hold loop skips the
   dead hold) and the release records NOTHING. Net: the recipe (releases at
   ceil(t+l) = ceil(end)) yields ONLY drop rows; `hold_end` rows appear only
   for holds whose key stays held past the sustain end (toy case), at the
   first integer tick >= end+2.
10. The opponent side stays excluded (the opponent strumline lists are empty in
    the probe so their loops are inert).

## 7. Acceptance chain

1. Probe run (orchestrator-owned freeze step, reviews first): produces
   `target/note-native-rows/{easy,normal,hard}.jsonl` + provenance manifest; after
   review, frozen into `tests/fixtures/note-native/` with sha256 digests. No automatic
   regeneration, ever.
2. `cargo xtask note-native --assets DIR` gate: fail-closed — re-exports the stream
   from the real corpus and compares its sha256 with the frozen stream; verifies row
   digests; re-runs the Rust `oxide_game::slice` sim over the exported stream; compares
   native rows field-wise with the Rust trace (row digests and stale/missing
   inventories fail the gate).
3. `verify.rs` moves `native_note_reference` pending→executable (milestone 5, assets
   required, fail-closed).

## 8. Explicit not-covered list

game_over/clamp ordering (update()-internal), note spawn/render geometry
(Strumline.updateNotes/SustainTrail render — phase 2b/render_diff), opponent-side
bookkeeping (excluded by design), botplay/practice branches, volcal/sound outputs,
mod/script event bodies (probe asserts no-canceled), input device timestamping
(the latency term is deliberately 0), recycling/kills past the slice path, and
everything outside bopeebo easy/normal/hard default charts. Field-wise equality on the
probe boundary does not prove full native-game parity; that obligation remains.

## 9. Implementation notes / open observations

Recorded 2026-11-07 while building the probe. These are implementation
decisions and observed divergences, not silent changes to the retained bodies.

1. **Row inventory is dispatch-only.** `PlayState.dispatchEvent` is the only
   instrumentation point (§6). The retained path dispatches `hit`, window
   `miss`, `ghost_press`, and a penalizing `hold_drop`. It does not dispatch
   `hold_start`, `hold_tick`, `hold_end`, or `game_over`. Those kinds were not
   invented. A diagnostic compare against `target/gameplay-sim/trace-*.jsonl`
   (the Rust sim trace; `target/note-native-rows/<difficulty>.jsonl` was not
   present) failed on row count: native easy/normal/hard 46/53/64 versus Rust
   19004/17352/14350. The extra Rust rows are almost all `hold_tick`.

2. **Exact-160 boundary release does not dispatch.** Retained `processNotes`
   (PlayState.hx L2956) emits `HoldNoteScriptEvent` only when
   `sustainLength > HOLD_DROP_PENALTY_THRESHOLD_MS`. The shipped run prints
   `Hold note too short, not penalizing...` and emits no row. The Rust sim
   still emits `hold_drop` with `score_penalty=0.0` and `is_combo_break=true`
   (easy sim 8015 note 3; normal/hard sim 3965 note 2). That extra row resets
   the Rust combo and shifts later `combo_before`/`combo_after`. The shared
   penalizing drop matches field-wise (easy/normal/hard sim 3001, penalties
   -140.5/-65.5/-65.5, `health_change=0.0`). Not patched on either side.

3. **Hold bookkeeping is driver-side and one tick off the real frame order.**
   `Strumline.update` is not retained, so the driver sets `missedNote`, clips
   `sustainLength`, and kills a completed hold before `processNotes` (§5). The
   clip uses the current tick after `Conductor.update`. In the real
   `PlayState.update`, `super.update` (and therefore `Strumline.update`) runs
   before the conductor update, so the real clip sees the previous frame.
   First observed health delta on a matched hit is one hold tick: easy
   hit@7200 `health_before` native 1.06012 versus Rust 1.06 (delta 1.2e-4 =
   `HEALTH_HOLD_BONUS_PER_SECOND * 0.001`).

4. **`update()` clamp is not a probe observable** (§2). Native `health_after`
   is the unclamped post-event value and grows past 2 (easy hit@75600
   `health_after` 4.256... versus Rust 2.03, which clamps at the next step).
   This is expected, not a scoring-formula bug.

5. **`NoteSprite.direction` setter short-circuit.** The verbatim setter
   (NoteSprite.hx L96-104) returns without storing when `frames == null`. The
   driver sets a temporary non-null sentinel only for the chart-direction
   assignment, then clears `frames`, so later sets still short-circuit (§3 [D]).

6. **Constructor dropped; four initializers added by the whitelist.** The real
   `PlayState.new` is not retained. `@:nullSafety` then rejects kept
   non-nullable fields that the constructor used to assign
   (`playerStrumline`, `opponentStrumline`, `currentSong`, `comboPopUps`).
   The build macro adds an initializer expression on those fields only. No
   retained method body is rewritten. The driver overwrites the strumlines
   before any retained call.

7. **LEGACY/WEEK7 scoring functions are retained.** The untouched
   `scoreNote`/`judgeNote`/`getMissScore` switches call them. Dropping the
   callees does not compile. Their bodies are the real functions; the PBOT1
   path is the one the probe executes (`Scoring.scoreNote(noteDiff, PBOT1)`).

8. **`NO_FEATURE_GHOST_TAPPING`** matches the shipped release hxml, so the
   retained ghost branch is the unconditional one. The latency term is 0:
   `PreciseInputManager.getCurrentTimestamp` and the queued event timestamp
   are both 0. Nonzero device latency is refused by construction; it never
   enters the probe.

9. **Window misses do not increment `totalNotesHit`.** Retained `goodNoteHit`
   increments it; retained `onNoteMiss`/`applyScore('miss')` do not. A toy
   window miss at sim 5161 left `total_notes_hit` unchanged. The Rust slice
   increments `total_notes_hit` for window misses. Not patched.

10. **Matched hit identity, before the health/combo gaps.** On the real
    bopeebo streams, every native `hit` matches the Rust `hit` on
    `note_id`, `sim`, `judgement`, `note_diff_int`, and `score_delta`
    (easy 44, normal 51, hard 62). The ghost press matches
    (sim 77275, direction 0, score -10.0, health -0.08) on all three.

11. **§6.1 hold regime, implemented 2026-11-07 (supersedes items 1-3 for
    hold rows).** The per-tick clip that ran before `processNotes` was
    removed. `hitNote` still captures
    `sustainLength = min(full, (strumTime + full) - songPosition)`
    (Strumline.hx L972-973) at the hit tick. After `processNotes` returns,
    the driver mirrors the state effects of `Strumline.updateNotes`:
    - L632-641 drop detection (`songPosition > strumTime && hitNote &&
      !missedNote && isPlayer && !isKeyHeld`) sets `missedNote` and does
      not recompute;
    - a release with `(end - sim) <= 0` completes instead (the L653
      `sustainLength <= 0` branch would kill it; §6.1 item 4);
    - L653-658 completion when `hitNote && sustainLength <= 0`;
    - L685-686 dropped hold stays clipped (no recompute);
    - L721-727 else recompute
      `sustainLength = (strumTime + fullSustainLength) - songPosition`,
      and `<= 0` emits `hold_end` on that tick;
    - L647-651 render-window kill is excluded (no render distance; no
      score effect).
    Driver rows follow dispatch rows. `hold_tick` is emitted only for holds
    that entered the retained accrual branch (PlayState.hx L2931-2935);
    deltas are `HEALTH_HOLD_BONUS_PER_SECOND/1000` and
    `SCORE_HOLD_BONUS_PER_SECOND/1000`. `hold_drop` is emitted on the tick
    after the drop-flag tick with `input_ms = 0.0`. Before that tick's
    `processNotes`, `sustainLength` is presented as `(end - flagTick)` so
    the retained `>` 160 test (PlayState.hx L2956) matches §6.1 item 4;
    the stale previous recompute is 1 ms longer and would penalize the
    exact-160 boundary. Sub-threshold rows have `score_penalty = 0`,
    `is_combo_break = false`, combo unchanged. The real
    `Hold note too short, not penalizing...` trace still prints.

12. **Official compare after the §6.1 Rust regeneration.**
    `target/note-native-rows/{easy,normal,hard}.jsonl` versus
    `target/note-native-rows/native/`. Row counts match: easy 19039,
    normal 17383, hard 14375. Every field matches within 1e-9 until the
    first hit whose `health_before` sits above 2 because the probe does
    not emulate the `update()` clamp and the Rust sim clamps state at the
    next step (§6.1). First divergent rows, verbatim:
    - easy row 5932 sim 31200 kind hit: `health_before` native
      2.1270399999994 Rust 2.0; `health_after` native 2.1570399999994
      Rust 2.03. Probe stderr: health exceeded HEALTH_MAX=2 starting at
      sim 27000.
    - normal row 5519 sim 31200 kind hit: `health_before` native
      2.02735999999944 Rust 2.0; `health_after` native 2.05735999999944
      Rust 2.03. Exceeded at sim 27598.
    - hard row 5824 sim 35700 kind hit: `health_before` native
      2.13335999999941 Rust 2.0; `health_after` native 2.16335999999941
      Rust 2.03. Exceeded at sim 31814.
    No other field diverged. Not patched; clamp was not emulated.

13. (Orchestrator note, 2026-11-07 — §6/§6.1 supersede any stale wording in
    items above: the sub-threshold drop now records a zero-penalty `hold_drop`
    row with `is_combo_break=false` and unchanged combo (the real path
    dispatches no event there); the rows normalize counts with the corrected
    Rust slices. Only §6/§6.1 and the frozen
    `tests/fixtures/note-native/_manifest_native.json` remain the operative
    probe contract for the `native_note_reference` gate.)

14. **Round-3 mirror order (supersedes item 11's frame-end ordering).**
    `presentDropRemaining` is gone. Each tick is now: (1) strumline mirror
    with P = previous songPosition (tick 0 uses P = -1), chain order
    L632-642 drop detect, L653-684 completion, L685-686 keep-clipped,
    L721-727 recompute `sustainLength = end - P`; (2) retained
    `Conductor.update` to T; (3) `processInputQueue`; (4) `processNotes`.
    A release at/after the sustain end (P >= end) does not set `missedNote`,
    so L653 can complete on the following mirror (§6.1 item 9). `hold_end`
    is therefore at ceil(end)+2 (toy full hold end 6200, row sim 6202).
    `hold_tick.hold_remaining_ms` is the snapshot the accrual branch
    observed. Two consecutive full snapshots: toy note l=400, ticks 3000
    and 3001 both remaining 400. Penalizing `hold_drop` fields are read
    from the real `HoldNoteScriptEvent`; the probe aborts if `score` differs
    from `-125.0 * snapshot / 1000` by more than 1e-9, or if `healthChange`
    is not 0, or if `isComboBreak` is not true. There is no injection hook
    into the retained event, so that abort is code-path evidence; the same
    formula is covered by `compare_note_rows.py --selftest`
    (`hold_drop_score_matches`). Toy: early release 3001 penalizes at sim
    3002 with snapshot 400, `score_penalty=-50` (stderr `Penalizing score
    by -50`); end-160 release penalizes at sim 4161 with snapshot 161,
    `score_penalty=-20.125`; end-159 release is sim 5162, snapshot 160.0,
    `score_penalty=0.0`, `is_combo_break=false`, combo 1→1. Real run on
    the current export (`a6c82a2e…` / `659e558b…` / `e7442de0…`) produced
    19039/17383/14375 rows and stderr penalties `-140.625`/`-65.625` plus
    `-20.125` (no "too short" trace). Official compare against the Rust
    rows then present failed at row 5: native `hold_remaining_ms` 1125/525
    vs Rust 1124/524. Those Rust files did not change during a 25-minute
    poll (mtime stayed 1794033939/3940/3940). Comparison against the
    round-3 Rust rows is pending. Clamp crossing still starts at easy/normal
    sim 27000/27598 and hard sim 31814; not emulated.

15. **Round-3 export compare stopped, not patched (2026-11-07).** Probe over
    the new exports (easy `9737e8a0…`, normal `b3c02ffa…`, hard `a1cd8b4c…`)
    wrote 18880/17224/14216 rows. `hold_tick` counts match the Rust rows
    (18769/17114/14107). The official comparator exited 1. First reported
    mismatch, verbatim: `easy.jsonl: row 423 (sim 7614, kind hold_drop): sim
    native=7615 Rust=7614`. The rows at that index are native `hold_end`
    sim 7615 note 2 versus Rust `hold_drop` sim 7614 note 2
    (`score_penalty=0.0`, `is_combo_break=false`, combo 1→1). Same shape on
    normal/hard at row 792 (note 3). Native kind totals include `hold_end`
    30/26/20 and `hold_drop` 3; Rust has `hold_end` 0 and `hold_drop`
    33/29/23. The boundary rows that both sides emit do match the revised
    recipe: penalizing `-20.125` (easy sim 8016 note 3; normal/hard sim 3966
    note 2) and a zero-penalty drop (easy sim 8767 note 4; normal/hard sim
    8017 note 4). Not adjusted on either side.

16. **Precision-fix compare is green except labeled clamp exclusions
    (2026-11-07).** The driver no longer skips the L632 flag when `P >= end`.
    A release at `R <= end` flags at the mirror of `R+1` and the drop row is
    that same tick, using the stale snapshot `end - (R-1)`. Completion
    (L653) clears that flag only when the pre-recompute snapshot is already
    `<= 0`, so a release at `R >= end+1` records nothing and `hold_end` is
    only the key-held-past-end case. Toy (ticks 7301, rows 645, exit 0):
    early release penalizes at sim 3002 with `-50`; `t+l-160` penalizes at
    sim 4161 with `-20.125`; `t+l-159` is a zero-penalty drop at sim 5162
    (combo 1→1); release at exactly end 6200 is a zero-penalty `hold_drop`
    at sim 6201 (combo 2→2, last accrual remaining 1.0); release at end+1
    is `hold_end` at sim 7102 and no drop row. Stderr: `Penalizing score by
    -50`, `Penalizing score by -20.125`, and two `Hold note too short, not
    penalizing...` lines. Negatives all exit 2 (schema drift, input_ms
    below sim, short/missing/extra ticks, multi-event). Official probe over
    the new exports (easy `9737e8a0…`, normal `b3c02ffa…`, hard `a1cd8b4c…`;
    chart `2d1fd06f…`, metadata `f891cd53…`) wrote 18880/17224/14216 rows.
    Kind totals match the Rust rows: `hold_tick` 18769/17114/14107,
    `hold_drop` 33/29/23 (of which 31/27/21 are zero-penalty and 2 penalize),
    `hold_end` 0. The item-15 split is gone: easy row 423 and normal/hard
    row 792 are now identical zero-penalty `hold_drop` rows at sim 7614.
    `python3 tools/oracle/compare_note_rows.py --native-dir
    target/note-native-rows/native --rust-lines-dir target/note-native-rows
    --report target/note-native-rows/native/compare-report.json` exited 0
    (50320 rows, 351220 f64 fields, failures 0). Clamp was not emulated.
    Stderr health exceeded `HEALTH_MAX=2` at easy sim 27000, normal 27757,
    hard 31973. Comparator exclusions (health fields only, after the first
    crossing row): easy 58 fields on 29 rows, first sim 31200, last sim
    75600; normal 66 fields on 33 rows, first sim 31200, last sim 75600;
    hard 78 fields on 39 rows, first sim 35700, last sim 75600. The report's
    `max_abs_delta_by_field.health_before` of about 0.114 is the crossing-row
    delta recorded before that exclusion, not a strict-compared mismatch.

17. (Orchestrator note, 2026-11-07 — operative-rules consolidation after the
    round-2 review.) The rules in force are ONLY: §6 + §6.1 REVISED (with the
    item-9 precision fix) + the frozen
    `tests/fixtures/note-native/_manifest_native.json`/`provenance.txt`.
    Historical §9 items 2, 3, 9, 11, 12 and 15 record superseded interim
    rounds; do not implement from them. Erratum on the code side (kept as a
    doc note so the frozen source hashes stay intact):
    `tools/oracle/note-probe/src/NativeNoteProbe.hx` header comment still says
    native rows exist only for dispatched events — the implemented
    `emitHoldRows` also emits the driver-derived `hold_start`/`hold_tick`/
    `hold_drop`/`hold_end` rows per §6.1, which is the governing rule. The
    compiler/mirror code itself is the reviewed implementation; only the
    stale comment text remains.

## 10. Round 2 — opponent-side probe (`native-note-probe-2`, M6 §10.2)

Frozen 2026-11-08. New evidence directory `tests/fixtures/note-native-opponent/`.
Round-1 sources, rows, streams and provenance stay byte-frozen; round 2 reads the
same round-1 stream files and re-freezes their sha256 in its own manifest. This is
still a synthesized-stream reference, **not** native-game parity and not a human
replay.

### 10.1 What is retained and driven

- **[V] Real opponent loops.** `funkin/play/PlayState.hx` is still the byte-exact
  game-copy file and `processNotes` is still the real body (L2801-2921, copied hash
  recorded in `provenance.txt`). Round 2 populates the opponent strumlines so the
  real opponent note loop (L2806-2837) and opponent hold loop (L2840-2866) execute
  unchanged. The player loops remain present in the same body but iterate empty
  lists. `GRhythmUtil.processWindow(note, false)` is the real retained function and
  supplies the `botplayHit` branch.
- **[S] Strumline surface.** The existing shadow `hitNote` preserves the observable
  state (`hasBeenHit`, `alive=false`, hold `hitNote`/`missedNote`/sustain capture);
  `playNoteHoldCover` is a no-op animation surface. Neither invents a trace row.
- **[S] Stage/character shells.** Round 2 constructs `currentStage = new Stage()`
  with non-null `dad`/`opponent` `BaseCharacter` shells. The shells expose
  `isSinging()`/`holdTimer`/`playSingAnimation()`; `singing` is a probe switch, not
  animation state. The driver sets the dad shell singing and presets `holdTimer=1.0`
  before every `processNotes`, so the real L2848-2851 assignment to `0` is observable
  and a missing reset fails the run. The opponent shell is non-null for the L2863
  miss-animation branch; that branch is not reached on the bopeebo corpus.
- **[S] Legacy-voice surface.** Round 2 gives `vocals` a `VoicesGroup` shell with
  `legacyVoiceSystem=true`, `legacyVoiceUsesPlayer=false`, and a distinct
  `opponentVocalsVolume=0.75`; after the run the probe asserts the real L2824
  `opponentVolume` assignment ran. No audio exists.
- **[D] Driver construction (pinned).** Opponent notes/holds are constructed from the
  same `bopeebo-chart.json`, only lanes `4..7`, in chart array order, with
  `note.direction = data % 4` and `hold.noteDirection = data % 4`. `note_id` is the
  zero-based **compacted index over the difficulty's opponent notes only** (chart
  array order). Player-note ids and opponent-note ids are independent id spaces; an
  opponent id does not count intervening player notes and vice versa.
- **[D] Player path kept inert.** Player strumlines are empty and the round-1
  stream's input events are validated but not replayed; `processInputQueue` is not
  called. The retained opponent loops read no input queue or player state, so this
  cannot change their observables and keeps the round-2 rows opponent-only. The
  conductor still replays the same per-tick `music_ms`; `processNotes(1/1000)` runs
  once per tick at the exported song position.

### 10.2 Rows emitted

Only one kind exists in round 2, emitted from the real L2812
`HitNoteScriptEvent(note, 0.0, 0, 'perfect', false, 0)` dispatch. Every row is the
schema-2 shared header **and nothing else** (serde field order):

```json
{"schema":"2","kind":"opponent_hit","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":2,"judgement":"perfect","raw_note_diff":0.0,"note_diff_int":0}
```

- `sim` is the tick whose `processNotes` dispatch fired; `input_ms` and
  `input_time_us` are `0.0`; `judgement` is the observed literal `perfect`;
  `raw_note_diff`/`note_diff_int` are `0.0`/`0` from the retained constructor.
- There are no score/health/combo/tallies fields. Multiple opponent hits on one tick
  are allowed because these rows carry no post-event state; the probe emits them in
  dispatch (chart array) order. The round-1 one-stateful-event rule is unchanged for
  player-path rows.
- The probe fails closed if a tick dispatches anything other than an opponent
  `HitNoteScriptEvent` (diagnostic names the tick and event), if an opponent note is
  hit twice/at a tick other than `ceil(strumTime)`, if any opponent note is never hit,
  if the row count differs from the constructed opponent-note count, if an active
  opponent hold does not reset `holdTimer`, if the round-2 legacy-voice shell is
  null/unconfigured or the retained `opponentVolume` assignment did not run, or if
  health/score/combo/tallies change. Player-path fail-closed behavior is untouched.
- Frozen counts: easy 44, normal 51, hard 62 rows; 78,276 ticks each. Row sha256:
  easy `c4756c80734591f085239eafd137ba0f84201ad5a6a50854124183739e5b0f8e`,
  normal `a3620ee69ba411cc5ed34354371a13615d0109f4ba2243fb9f238f3a6e09096d`,
  hard `191f757f891b4c517a1a97ce859c77c15680d1f031f93b75697c8d1343cf9a08`.
  A second run produced identical hashes (two-run determinism).
- Round-1 integrity after this freeze (unchanged, re-checked):
  rows easy `923ce1cb…`, normal `ae138208…`, hard `3c1acd54…`; streams
  `9737e8a0…`/`b3c02ffa…`/`a1cd8b4c…`. The round-1 row counts remain
  18,880/17,224/14,216 and re-running the extended probe with `PROBE_ROUND=1`
  reproduced those hashes byte-identically.

### 10.3 Exclusions (round 2)

- **Opponent hold lifecycle is not mirrored.** `Strumline.updateNotes` is still not
  retained, so opponent `sustainLength` is never recomputed/clipped/completed. The
  retained hold loop therefore evaluates its `hitNote && !missedNote &&
  sustainLength > 0` branch with the hit-tick capture; the `holdTimer` reset is
  exercised, but completion timing and the drop-miss `playSingAnimation` branch are
  not. Neither produces a trace row by contract (§10.2 [D]).
- **No opponent miss rows.** The real opponent note loop has no miss-event dispatch
  (only the hold loop reacts to `missedNote`). On this contiguous-grid corpus every
  opponent note is auto-hit at `ceil(strumTime)`; the probe turns that expectation
  into a fail-closed all-hit check rather than silently dropping a note.
- **No hold_start/hold_tick/hold_drop/hold_end/game_over rows** from the opponent
  side; those are not dispatched by the retained opponent bodies. No score, health,
  combo, tally, accuracy, sing-schedule or render observation is made. Botplay/practice
  branches, real input devices, audio, scripts, stage/character rendering, update()
  clamp/death ordering and full-game parity remain outside this probe.
- The fixture is a round-2 **observation** of the retained opponent loops; it is not
  an independent oracle, not a full-chart trace, and not a substitute for native-game
  integration evidence.
