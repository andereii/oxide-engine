# Conductor validation: behavior specification and evidence boundary

Authority: human-approved ADR-0014 (2026-11-05). This corrects erroneous expectations;
no gate tolerance is increased. This specification separates measured utility behavior,
model assumptions, generated expectations, and Rust implementation. It is inspired by
behavioral-spec/cleanroom separation, **not** a claim of a legally clean-room process or
independent full-game proof: the implementation/model authors have read game sources.

## Observed: native rounding utility

`probe_rounding.sh` imports the **original installed** `flixel.math.FlxMath` with a
classpath into the read-only game tree. `ProbeSlice` retains `roundDecimal`'s original
compiler AST unchanged and removes unrelated class fields. Two explicitly external,
empty import types (`FlxSprite`, `Rectangle`) avoid pulling in graphics. Neither type
is used by the retained function. No `FlxMath` replacement/transcription is on the
probe classpath. Haxe emits that real function into native C++, linked to the installed
hxcpp runtime. Analyzer optimization/full DCE are enabled. The generated function calls
`Math_obj::fround`; source/generated-code hashes and runtime versions are recorded in
`provenance.json`. The original tree is never modified.

Both that native utility and a separate minimal native `Math.fround` scaling harness
were executed **before** correcting Rust/the timeline model. They returned:

| input | decimal places | result |
|---:|---:|---:|
| 0.5 | 0 | 1 |
| -0.5 | 0 | 0 |
| 2.5 | 0 | 3 |
| -1.5 | 0 | -1 |
| -1.6 | 0 | -2 |
| 1.23456789 | 6 | 1.234568 |
| -1.23456789 | 6 | -1.234568 |
| 1.23445 | 4 | 1.2345 |
| -1.23445 | 4 | -1.2344 |
| 2147483649.25 | 0 | 2147483649 |

Exact stdout is in `rounding-probe.txt`. This discriminates from binary32 conversion,
negative-half-away rounding, and an overflowing Int-valued implementation.
`FlxMath.hx:68-76` builds `mult` by repeated multiplication by 10 and returns
`Math.fround(value * mult) / mult`. The installed hxcpp `include/hxMath.h:28` implements
`fround(x)` as `std::floor(x + 0.5)`, returning a double, **not** JavaScript's similarly
named binary32 operation. Everything stays binary64. The Neko model uses
`Math.ffloor(value * mult + 0.5) / mult` and passes the same cases, including the
above-`i32::MAX` value. Do not substitute Neko's potentially Int-valued fround fallback.
These finite test cases are measured evidence, not exhaustive floating-point proof.

## Model-only: full conductor timeline

`OracleConductor` is a source-informed transcription; it does not execute the real game
Conductor. Native utility parity does **not** independently validate its time-change
mapping, dispatches, offsets, measures, or PlayState integration. Byte-identical
regeneration proves determinism, not correctness or oracle independence.

The model and Rust are compared over exactly the 61 chart IDs in `corpus.txt`, plus a
separate smoke case. Inputs are the variation's metadata time changes, default
instrumental offset, and `ffprobe` duration of the song's default `Inst.ogg`. Chart,
metadata, and instrumental source hashes are recorded in `provenance.json`.
Alternate instrumentals, audio jitter, real input, and human replay are not covered.

Simulation `countdown-1ms-playback-1ms-v1` has start timestamp zero, no save-file
calibration, desktop OGG format offset zero, and `applyOffsets=false` throughout:

1. Map changes (stable timestamp sort, clamp negative timestamps, recompute beat times
   using round-4). The map's initialization update does not emit fixture rows.
2. Initialize stopped pre-roll at `-5 * beatLengthMs`, `sim=0`.
3. While position is below combined offset, update with previous position + 1 ms,
   incrementing `sim`. This can overshoot zero by a fractional millisecond.
4. Switch to playing, increment `sim`, update with music time 0 + combined offset.
   Then repeat at each integer music time through `floor(lengthMs)` inclusive.
   No fractional final update is invented. Music-playing clamp is active in this phase.

**Every update matters**, including those emitting no rows, the countdown's final
fractional overshoot, and the distinct playback time-zero update. Never replay once
per row (three rows can describe one update), nor skip all silent updates. The
`startingSong` BPM scan is in the model but not the Rust API; for this zero-start
corpus it selects the same first change. Empty changes use the previously documented
permissive 100 BPM, 4/4 fallback, rather than the game's stale-state warning branch.
This is an explicit model assumption, not observed native-game parity.

## Fixture / gate contract

JSONL row fields/order are unchanged. Each boundary row is the post-update state;
`t` is step/beat/measure, `i` its new index. Equal `sim` groups must share every state
field and contain changed dispatches once in step -> beat -> measure order. Simulation
times must increase between groups, and playback cannot revert to pre-roll.

`_manifest.json` schema 1 records the replay recipe/version and reference limitation,
the lexicographically ordered 61 `charts`, and a separate `smoke` entry. Each entry
contains `id`, `lengthMs`, raw `timeChanges` (`t,b,bpm,n,d`, copied before model mapping),
`iOff/fOff/avOff/gOff`, `steps/beats/measures`, and total `rows`. It is generated alongside
candidates, not inferred from boundary rows. This descriptor compactly records the
whole input sequence, including silent updates and both phase boundaries. It is tool
metadata, not a change to any game data format. It has no dependence on the original
asset tree during Rust verification.

`conductor_check::run(&[String]) -> bool` accepts only `--fixtures <dir>` or no
arguments (workspace's `tests/fixtures/conductor`). It never invokes Haxe or repairs
missing expectations. Exact catalog/manifest identity, 61 non-underscore chart files,
nonempty rows, required numeric/bool fields, counts, indices, time-change indices,
signatures, state, per-update grouping and dispatch order must match. `_smoke` must
pass but is excluded from all 61-chart acceptance counts. A partial/micro corpus
cannot certify the gate. Private comparator tests use smoke without providing a
partial-corpus success CLI.

Position/input/simulation time errors must be <= 1 ms; fractional step/beat/measure
time errors are also checked after conversion to milliseconds. Beat/step/measure
lengths and BPM use at most 1e-6 relative error. Offset components must match recorded
metadata. Matching boundary counts/indices/order are never replaced by tolerances.
The output labels the reference as model-only and leaves M4 input/replay/jitter pending.

## Reviewed replacement

`candidate-review.json` records before/after hashes for all 62 timelines and a semantic
review aligned by each event kind/index sequence, not a misleading line-by-line zip.
41 snapshots change; 21 (including smoke) are byte-identical. All 86,131 rows and every
per-kind count/index sequence are retained. 127 matched rows shift by at most 1 ms.
`monster-default` has one cross-kind ordering change: measure 38 stays at 91,955 ms;
step 608 and beat 152 move from 91,955 to 91,956 ms. Correct decimal rounding removes
the old binary32 coalescing. This is a reviewed behavior correction, not permission
to reorder events inside an update. Only the 41 changed files were replaced.

Integrated validation now passes for Rust/model replay, native-utility rounding,
strict workspace tests and double-regenerated snapshots; see
`../../docs/WORKFLOW-REPORT.md` for commands and limitations. Alternate replay inputs
must equal the approved manifest embedded in the Rust gate, preventing renamed-smoke
substitution. Still needed: broader independent real-game traces and the unimplemented
M4 input/replay/jitter acceptance work. The passing snapshot script must never be
reported as completion of all M4 requirements.

## Native real-Conductor probe (2026-11-06, human-approved)

`native_conductor_probe.sh` compiles the **REAL installed** `funkin.Conductor` into a
native hxcpp binary with `ConductorSlice` (field whitelisting like `ProbeSlice`
for `FlxMath` — no field BODY is altered), and runs the frozen replay recipe
(`countdown-1ms-playback-1ms-v1`) from `tests/fixtures/conductor/_manifest.json`
against it. Row capture comes from the game's own `onStepHit`/`onBeatHit`/
`onMeasureHit` signals; the row schema is byte-compatible with the frozen model
fixtures. Comparison (`compare_native_rows.py`) is field-wise: counts, indices,
sims and dispatch order exact; lengths/BPM 1e-6 relative; offsets exact; ms
fields <= 1 ms. IMPORTANT units note (fixes the earlier wording): the rows'
`stepTime`/`beatTime`/`measureTime` hold STEP/BEAT/MEASURE counts, not
milliseconds, so the comparator converts them to ms through each row's own
step/beat/measure length BEFORE applying the 1 ms budget — the same mechanics
`fixtures-conductor` applies (`xtask/src/conductor_check.rs` compare_state).
Comparing those counts raw with a 1.0 absolute tolerance would hide up to a
full step of drift (checked by review 2026-11-06).

Observed result (first approved run): native == model on ALL 62 timelines
(86020 chart rows + 111 smoke) — `charts ok 62/62, worst delta 0.000 ms`; no
per-row drift. Row digests are frozen in
`tests/fixtures/conductor-native/_manifest_native.json` (input/source/instrument
sha256 chain). The xtask gate `cargo xtask conductor-native` replays:
native-vs-model field equality AND our Rust conductor vs the native rows (1 ms
budget), plus the digest fail-closed verification.

Stub/decision inventory (all separately labelled as measured, copied-verbatim, or
engine decisions; the game tree is never written):

1. `ConductorSlice.hx` retains only the timing path (update/mapTimeChanges/
   dispatch signals/timing getters/state); removed fields are unrelated
   gameplay/graphics surface; NO retained method body is modified.
2. Byte-exact copies of `Conductor.hx`, `FloatTools.hx` (clamp), `AnsiUtil.hx`
   (trace decoration) are compiled from a generated copy tree so the game's
   `import.hx` closure (funkin.Assets/Paths/Tracker/thx) is not dragged in;
   hashes recorded. `probe-shim/funkin/import.hx` (NOT an original file)
   declares only the using/import surface the retained modules need.
3. Classpath order (`game-copy`, installed `flixel`, `probe-types` last): the
compiler resolves a module name with the LAST classpath containing it
(empirically confirmed 2026-11-06: a shadowing stub in a later classpath wins).
So the stubs take precedence for their exact modules and everything else still
resolves to the real installed sources.

Shadowed stub modules: `SongData` (SongTimeChange shape copied verbatim with
   citations), `SongDataUtils.sortTimeChanges` (real body verbatim, real
   `FlxSort` constants/values), `Constants` (only the 6 constants used, values
   copied verbatim with line citations), `TimerUtil` (SongSequence signal
   shell), `flixel.util.FlxSort` (byValues comparator + constants verbatim),
   `flixel.util.FlxSignal` (add/remove/dispatch surface, insertion-order calls),
   `flixel.FlxG` + `flixel.sound.FlxSound` (simulated music state).
   Engine decisions: `FlxG.elapsed = 0` (songPositionDelta inert — frame deltas
   are not part of the frozen recipe), `PlayState.instance = null` (the bpm
   getter's startingSong scan never runs; the corpus's recipes select
   timeChanges[0] the same either way; startTimestamp 0), music stub drives
   `time/length/playing/pitch` exactly as the recipe does.
4. NOT covered: gameplay/notes/audio engine, save-file calibration (offsets
   recorded in the manifest), hardware latency, alternate instrumentals, and
   anything the recipe does not simulate. Model agreement below was observed on
   this machine and does not extrapolate to other inputs; the wider native-game
   parity question has its own acceptance obligations.

Clamp-semantics divergence (recorded 2026-11-06, contract-interpretation note):
the installed `FloatTools.clamp(value, min, max)` is
`Math.max(min, Math.min(max, value))`, so the real playback clamp
`Math.min(combinedOffset, 0).clamp(songPos, currentLength)` evaluates to
`max(songPos, min(length, min(combined, 0)))` — it does NOT cap `songPos`
above `music.length` when `songPos` is already above the length. The M4
contract §0 describes the model transcription reading
(`min(max(songPos, min(combined, 0)), length)`). For the frozen recipe corpus
both agree (combined == 0 and songPos <= length over every update), which is
why native == model at 0.000 ms is NOT clamp-identity proof. Any future claim
of clamp parity needs a discriminating case with `songPos > music.length`
against the real binary; the model remains the contract reading until then.
