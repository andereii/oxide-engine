# M4 contract — clock, input, conductor + Haxe oracle tooling

M4 contract, amended under human-approved ADR-0013/0014. The source-informed Haxe
model fixtures are one regression criterion, not independent full-game acceptance.
Native-game traces, input/replay/jitter and platform evidence remain required; see
`WORKFLOW.md` and `../tools/oracle/BEHAVIOR.md` for the evidence boundary. Units: milliseconds,
held as Haxe `Float` semantics — in Rust this means **f64 everywhere** (never f32;
Haxe `Float` is IEEE double and the roundDecimal(…,6)/(…,4) rounding only
reproduces in f64). No rendering, no gameplay/notes, no video, no runtime decode in
game code (decode lives behind test-only paths this milestone). No new native deps
without an ADR (evdev is pure Rust; audiopus_sys vendors C — needs its own ADR when
the runtime decoder work starts, M5+; this milestone uses symphonia behind
`#[cfg(test)]`-style paths only where allowed).

## 0. Oracle facts (source of truth, `~/Descargas/fnf/Funkin`)

`source/funkin/Conductor.hx` (901 lines, verbatim reference — cite line numbers):

- Lengths: `beatLengthMs = (60/bpm)*1000 * (4/tsDen)` (L176-182);
  `stepLengthMs = beatLengthMs / 4` (STEPS_PER_BEAT=4, L187-192);
  `measureLengthMs = beatLengthMs * tsNum` (L166-171).
- `combinedOffset = instrumentalOffset + formatOffset + globalOffset` (L294-299);
  `globalOffset = Preferences.globalOffset`; `audioVisualOffset` = save option
  (L282-292). Desktop build: `formatOffset = 0` (`EXT_SOUND = 'ogg'`; web-only
  `MP3_DELAY_MS = 528/44100*1000 ≈ 11.97`, PlayState L1159-1163, Constants L618/430).
- `instrumentalOffset = song metadata offsets.getInstrumentalOffset(currentInstrumental)`
  (PlayState L866-875; SongOffsets L275: per-`altInstrumentals` name, default the
  `instrumental` field; applied ON TOP of nothing else).
- `update(?songPos, applyOffsets=true, forceDispatch=false)` (L422-517):
  1. `songPos += applyOffsets ? combinedOffset : 0` (L433).
  2. If music `playing` (gameplay): `songPosition = Math.min(combinedOffset, 0)
     .clamp(songPos, currentLength)` — Haxe `Floats.clamp(value=this, min=songPos,
     max=currentLength)`, i.e. `songPosition = min(max(songPos, min(combinedOffset, 0)),
     currentLength)` — L440-444; else `songPosition = songPos` (no clamp).
  3. `currentTimeChange = timeChanges[0]` for `songPosition <= 0`; otherwise the last
     time change with `timeStamp <= songPosition` (L452-461).
  4. `currentStepTime = roundDecimal(currentTimeChange.beatTime*4 + (songPosition −
     currentTimeChange.timeStamp)/stepLengthMs, 6)` (L470-471);
     `beatTime = currentStepTime/4`; `measureTime = getTimeInMeasures(songPosition)`.
  5. Else branch (timeChanges empty): `stepTime = roundDecimal(songPosition /
     stepLengthMs, 4)`.
  6. `currentStep/Beat/Measure = floor(…Time)`; dispatch onStep/Beat/MeasureHit iff
     the integer CHANGED (L490-503).
- CORRECTED 2026-11-05 (ADR-0014, observed native utility execution):
  `FlxMath.roundDecimal` is `Math.fround(value * mult) / mult`. Native hxcpp
  `fround(x)` returns the floating-point integer `floor(x + 0.5)`, NOT binary32.
  Everything remains binary64. The real installed FlxMath method was compiled and
  executed for discriminating positive/negative/precision/large-number cases; see
  `../tools/oracle/rounding-probe.txt` and `provenance.json`. Negative halves round
  toward positive infinity. Utility parity does not prove full Conductor parity.
- `mapTimeChanges` (L549-600): sort by `timeStamp` ascending (SongDataUtils
  L267-275, `FlxSort.ASCENDING` of `a.timeStamp` values); negative `timeStamp`
  clamped to 0 with `beatTime = 0`; for `timeStamp > 0`:
  `beatTime = roundDecimal(prev.beatTime + (ts−prev.stamp)*prev.bpm/60000 *
  (prev.tsDen/4), 4)`. `STEPS_PER_BEAT = 4`, `SECS_PER_MIN = 60`, `MS_PER_SEC = 1000`
  (`funkin/util/Constants.hx`).
- `getTimeInMeasures(ms)` (L607-647): iterates time-change SEGMENTS and adds
  `(seg.stamp − prev.stamp)/prevStepLength/prevStepsPerMeasure`; final fractional part.
- PlayState call shapes (the phases the runner must reproduce):
  - Pre-roll (music stopped): `update(-5*beatLengthMs + startTimestamp, <default
    true on L883>)` then per-frame `update(songPosition + elapsed*1000, false)`
    (L1147) during countdown; `startTimestamp = 0` for standard playback (L766).
  - Gameplay: `update(lerp(previous, music.time + combinedOffset, ease), false)`
    (L1179); the runner simulates PERFECT audio: `music.time` in 1 ms steps,
    `music.playing = true`, `music.length` = decoded instrumental duration (probed
    with ffprobe). The Rust replay must reach identical junctions.

Corpus: 61 chart files under `assets/preload/data/songs/<id>/<id>-chart[-<variation>].json`
(siblings `<id>-metadata[-<variation>].json`); audio is under `assets/songs/<id>/`. Chart files carry per-difficulty notes for ONE
variation; the conductor timeline depends only on the metadata `timeChanges` of that
variation + `offsets` (verbatim JSON shapes, consumed through `oxide-data` on the
Rust side; ADR-0003 keeps `offsets` untyped).

## 1. `tools/oracle/` — instrumented Haxe runner (committed; assets tree unchanged)

Deliverables in this repo (NOT in the game tree — the game sources stay untouched):

```
tools/oracle/
  README.md                 # how to build + run; snapshot verifiability notes
  snapshot_validate.sh      # prints sha256 of the cited Haxe sources used as habitat
  build.hxml                # haxe build script (target hl or interp)
  src/Main.hx               # CLI: --assets <dir> --out <dir> [--only <songId>...]
  src/OracleConductor.hx    # the instrumented runner: per-chart timeline dump
  src/OracleStubs/...       # minimal stand-ins ONLY if transcription needs them
  src/Verbatim.hx           # per-line citation header enforcing the audit trail
```

Rules of the oracle:

- `OracleConductor` is a source-informed transcription with citations, not direct
  execution of the game's Conductor. Keep observed utility behavior, derived math
  and model assumptions separate; native-game traces must validate the broader model.
- It must NOT modify the game repo (never write inside `~/Descargas/fnf/Funkin`).
- CORRECTED 2026-11-05: the Neko model explicitly uses binary64
  `Math.ffloor(value * mult + 0.5) / mult`, matching the observed native utility.
  Do not use an Int-overflowing fallback or binary32 conversion.
- Fixture format (JSON Lines, one file per chart file):
  `tests/fixtures/conductor/<chart-id>.jsonl`; `<chart-id>` = song dir + chart file
  suffix, e.g. `bopeebo-default-easy.jsonl`... determinism: EXACTLY one timeline per
  chart FILE (61 files), each covering: the pre-roll phase (from `pre = 5 beats
  before 0`, countdown shape: `update(pos, false)`) AND the playback phase
  (`music.time` from 0 → `length` in 1 ms steps with `music.playing = true`,
  `songPos_input = music.time + combinedOffset`, `update(songPos_input, false)`).
  Boundary rows appear exactly when an int step/beat/measure changed, one line per
  changed int, ordered by simulation time and then by step<beat<measure dispatch
  order. Pre-roll rows and playback rows are the SAME stream (continuous sim with the
  two phase switches the runner reproduces PlayState's call shapes, including the
  clamp-active playback branch). Each line:
  ```
  {"t":"step|beat|measure","i":<int can be negative>,"sim":<simulation time ms>,
   "input":<songPos passed to update>,"songPosition":<result>,
   "stepTime":<rounded>,"beatTime":<rounded>,"measureTime":<rounded>,
   "step":<int>,"beat":<int>,"measure":<int>,
   "beatLen":<f>,"stepLen":<f>,"measureLen":<f>,
   "tc":<index int>,"tsNum":<int>,"tsDen":<int>,"bpm":<f>,
   "iOff":<f>,"fOff":<f>,"avOff":<f>,"gOff":<f>,"combined":<f>,
   "clampActive":<bool>}
  ```
  Geometry: `music.length` per song = ffprobe of `assets/songs/<id>/Inst.ogg`
  *1000 (the default instrumental; alternate instrumentals are out of scope
  this milestone — variations use their own metadata timeChanges but the DEFAULT
  audio path only). Determinism: the runner sorts charts lexicographically, and has
  NO wall-clock component (the fixture is human replay-free; the runner's 1 ms grid
  is part of the frozen snapshot).

The dumped citations (line numbers) are audited by `@oxide-reviewer` against the
real `Conductor.hx` at review time.

## 2. `crates/oxide-audio` — Kira clock + conductor semantics

New dependencies (each an ADR line in the implementation brief):
- `kira` 0.12.6 with default features disabled (ADR-0012); opt-in real audio remains separate.
- `symphonia` 0.6/0.7 (pure Rust decoders: vorbis now, opus NOT available — runtime
  decode gate stays behind a test-only path this milestone; the CLOCK does not need a
  decoder: songs are simulated clocks).

```rust
// module oxide_audio::conductor — pure, no audio backend, no game deps
pub const STEPS_PER_BEAT: u32 = 4;
pub const SECS_PER_MIN: f64 = 60.0;
pub const MS_PER_SEC: f64 = 1000.0;

#[derive(Debug, Clone)]
pub struct TimeChange {              // mirrors SongData.SongTimeChange (alias names in oxide-data)
    pub time_stamp: f64,             // ms
    pub beat_time: f64,              // 0 for ts <= 0 (mapTimeChanges normalizes)
    pub bpm: f64,
    pub time_signature_num: i32,     // default 4
    pub time_signature_den: i32,     // default 4
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConductorOffsets {
    pub instrumental: f64,
    pub format: f64,
    pub audio_visual: f64,           // save option; separate from global
    pub global: f64,                 // user calibration (ms, int-typed upstream)
}
impl ConductorOffsets {
    pub fn combined(&self) -> f64 { self.instrumental + self.format + self.global }
}

#[derive(Debug, Clone)]
pub enum Dispatch { Step(i32), Beat(i32), Measure(i32) }   // one per integer change, in this order per update

#[derive(Debug)]
pub struct Conductor {
    pub offsets: ConductorOffsets,
    time_changes: Vec<TimeChange>,   // normalized: sorted; ts<0 -> 0 with beat_time = 0; beat_time recomputed
    current_change_index: usize,     // CURRENT_TIMESEN index in the vec (usize::MAX = none)
    song_position: f64,
    current_step_time: f64, current_beat_time: f64, current_measure_time: f64,
    current_step: i32, current_beat: i32, current_measure: i32,
}

impl Conductor {
    pub fn new() -> Self;
    /// funkin.Conductor.mapTimeChanges: sort + normalize + recompute beat_time (round 4).
    pub fn map_time_changes(&mut self, changes: &[TimeChange]);
    /// funkin.Conductor.update: f64 math, round 6/4 where Haxe rounds.
    /// `music_len_ms` = Some(len) when the simulated music "plays" (enables the clamp
    /// branch); None reproduces the non-playing (countdown) semantics.
    /// Returns the dispatches fired by THIS update call (in step→beat→measure order).
    pub fn update(&mut self, song_position: Option<f64>,
                  apply_offsets: bool, music_playing: bool,
                  music_len_ms: f64) -> Vec<Dispatch>;
    // getters: song_position_ms(), bpm(), starting_bpm(), beat_length_ms(),
    // step_length_ms(), measure_length_ms(), current_step()/beat()/measure() (i32),
    // current_step_time()/beat_time()/measure_time() (f64), current_time_change()
    pub fn combined_offset(&self) -> f64;
    // conversions (used by M5 scheduling):
    pub fn get_time_in_steps(&self, ms: f64) -> f64;
    pub fn get_step_time_in_ms(&self, step_time: f64) -> f64;
    pub fn get_beat_time_in_ms(&self, beat_time: f64) -> f64;
    pub fn get_measure_time_in_ms(&self, measure_time: f64) -> f64;
    pub fn get_time_in_measures(&self, ms: f64) -> f64;
    pub fn get_time_change_at_ms(&self, ms: f64) -> Option<&TimeChange>;
}

// module oxide_audio::clock — the Kira-backed truth (thin)
pub struct AudioClock { manager: kira::manager::AudioManager, clock: ClockHandle, /* sounds later in M5 */ }
impl AudioClock {
    pub fn new() -> Result<Self, AudioError>;
    /// Never frame-delta derived: `song_position_ms` reads the kira clock ticks.
    pub fn song_position_ms(&self) -> Result<f64, AudioError>;
    pub fn clock_hz(&self) -> f64;                  // documented kira tick rate
    pub fn set_start_offset_ms(&self, ms: f64) -> Result<(), AudioError>; // pause/seek = destroy + recreate
    pub fn set_rate(&mut self, rate: f64) -> Result<(), AudioError>;
    /// Same clock for Inst and Voices (M5 will attach sounds to this clock).
    pub fn handle(&self) -> ClockHandle;
}
#[derive(Debug, Error)] pub enum AudioError { Backend(String), Clock(String), Decode(String) }
```

Honesty checks (unit tests):
- `map_time_changes` normalization: negative stamps, provided-`b` vs recomputed b,
  the round-4 digit behaviour.
- Single-time-change vs multiple; 7/8 and 15/16 signatures produce the boundary sets
  from the class doc comment (L21-46) — compute 120 BPM 7/8 step length independently.
- update() phases against synthetic charts (no audio backend needed): clamp only when
  playing; applyOffsets only when flag set; force step 0 fire at songPosition >= 0
  crossing (not before); dispatch ordering step→beat→measure within one update.

## 3. `crates/oxide-input` — timestamped capture (Linux evdev; uinput for tests)

New dependency: `evdev` (pure Rust; ADR). Timestamp source: the kernel event
timestamp for each evdev event plus a `saturating delta to the audio clock taken as
close to the syscall return as possible`; never derive from frame loop.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState { Down, Up }
#[derive(Debug, Clone)]
pub struct KeyEvent { pub audio_time_ms: f64, pub kernel_time_us: u64, pub code: u16, pub state: KeyState }
pub struct InputStream { device: Option<VirtualDeviceDev> /* boxed opaque */ }
impl InputStream {
    /// Real device (path /dev/input/eventN) — documented fallback when group
    /// `input` access is missing (this machine: only /dev/uinput is ACL'd).
    pub fn open(path: &str) -> Result<Self, InputError>;
    /// Test-only virtual device (uinput, ACL pre-granted) driving the jitter probe.
    pub fn open_virtual() -> Result<Self, InputError>;
    pub fn poll(&mut self, now_audio_ms: f64) -> impl Iterator<Item = KeyEvent>;
}
pub const SECONDARY_CODES: [u16; 7] = [/* the 7 gameplay keys, u16 Linux codes */];
pub struct JitterReport { pub p50_ms: f64, pub p99_ms: f64, pub samples: usize }
```

`.poll(now_audio_ms)` timestamps each event with `now_audio_ms` (the value the
CALLER handed in, having been captured just after the poll syscall — the raw delta
between kernel timestamp and audio-time read is the jitter measurement).

## 4. Gates (xtask-owned once crates exist)

- `cargo xtask fixtures-conductor`:
  Never invokes the generator or repairs missing expectations. Requires the committed
  manifest, exact 61-chart catalog and separate smoke case; replays every simulation
  update (including silent updates) rather than only the emitted rows. For each
  `tests/fixtures/conductor/*.jsonl`, replays the recorded model simulation through
  OUR `oxide_audio::conductor::Conductor` using the fixture's inputs (per update: the
  recorded `input`, `music_playing`, `music_len_ms`), and checks:
  - identical step/beat/measure COUNTS per type and identical boundary index lists;
  - for every matched row: `|our.songPosition − oracle.songPosition| <= 1 ms` (the
    printed `worst delta`), plus `tsNum/tsDen` equality and `beatLen`/`stepLen`
    within 1e-6 relative;
  - exit 0 iff ALL 61 charts and the separately counted smoke case pass. Output must
    explicitly state model-only parity and pending native-game/input/replay/jitter work.
    The core chart summary retains:
    ```
    charts: 61
    ok: 61  failed: 0
    worst chart: <id>
    worst delta ms: <x.xxx>
    boundary ok: <n>/<n>   (step/beat/measure row counts)
    ```
- `cargo xtask replay --fixture <chart>`:
  Deterministic hits + score from a recorded input stream. The stream is
  SYNTHESIZED from the chart (label in the report: `input source: synthesised from
  chart (no human replay yet)`) with the audio-clock timestamps from the oracle
  fixture positions; the scoring delta windows replicate Haxe `PlayState` hit
  windows measured at M5 — this gate only requires DETERMINISM (same stream → same
  report across two runs, `--seed`-free). The ordering proof: two runs print identical
  sha256. Prints: `replay: <chart> hits <n> misses <n> score <n> epicas...` (exact
  fields deferred to the M5 port of `PlayState` scoring; the M4 REQUIRED fields are
  the sha256 determinism + hit/miss counts).
- Input jitter report: printed by `cargo xtask input-jitter` (NEW command): p50/p99
  on this machine from the uinput loop (target ≤ 2 ms; below we proceed; above, the
  documented mitigation is the kernel-timestamp→audio-clock interpolation rather
  than "ignore the input").

## 5. Sequencing + deferrals

- Phase 1 deliverables (this turn): tools/oracle (task O) and oxide-audio core
  (task A) in PARALLEL (disjoint crates/files). Follow-up tasks wire
  `xtask fixtures-conductor` + the input crate + the replay gate with their own
  briefs (max 2 in parallel, sequential if both touch xtask).
- Deferred: notes/gameplay (M5), runtime decoding (M5), save-file plumbing for the
  calibration offsets (M7), pause(video handling) (M6).
