# tools/oracle — instrumented Haxe conductor runner (M4)

Committed, standalone Haxe program that transcribes `funkin.Conductor` and dumps the
frozen JSONL timelines in `tests/fixtures/conductor/`. It never writes inside the
reference game tree and it does not compile or import the game, Flixel, or any
haxelib: the only inputs are the game's assets (charts, metadata, `Inst.ogg`) and the
ported conductor maths.

Contract: `docs/M4-CONTRACT.md` §0 (oracle facts, including the 2026-11-05
`Math.fround` amendment) and §1 (fixture format). The line-number citations are in
`src/Verbatim.hx` and in per-method comments in `src/OracleConductor.hx`;
`snapshot_validate.sh` hashes each cited range so drift is visible.

## Toolchain

Verified on this machine:

| tool | version | role |
|---|---|---|
| Haxe | 4.3.7 | compiler (`/usr/bin/haxe`) |
| Neko | 2.4.1 | canonical runtime (`/usr/bin/neko`) |
| ffprobe | 8.1.3 | `format=duration` probe of the default instrumental |

HashLink is not installed. Neko is the canonical target because it is installed
out of the box and its `Std.string` float formatting is stable; the Haxe
interpreter (`haxe --run Main ...`) also works but prints more digits, so its
bytes are **not** interchangeable with the committed fixtures. Regenerate with
Neko only.

No network access is used. The only external commands are `haxe`, `neko` and
`ffprobe` (override the probe binary with `FFPROBE=/path/to/ffprobe`).

## Build and run (canonical)

From the repository root:

```bash
cd tools/oracle
mkdir -p /tmp/opencode/oracle-build
haxe build.hxml
neko /tmp/opencode/oracle-build/oracle.n \
  --assets "$HOME/Descargas/fnf/Funkin/assets" \
  --out ../../tests/fixtures/conductor
```

`build.hxml` puts the generated module under `/tmp/opencode/oracle-build/`, never
in the repository. A direct interpreter invocation is also available for quick
checks (not for byte-identical regeneration):

```bash
cd tools/oracle
haxe -cp src --run Main --assets "$HOME/Descargas/fnf/Funkin/assets" --out /tmp/opencode/oracle-check
```

CLI:

```
oracle --assets <game-assets-dir> --out <fixture-dir>
       [--only <song-id|fixture-id>[,<id>...]] [--citations]
```

* `--only` filters by song id (`bopeebo`, all its variations) or fixture id
  (`bopeebo-erect`). It is for debugging only.
* `--citations` prints the audit table from `Verbatim.hx`.
* The tool always writes `_smoke.jsonl` next to the chart fixtures.
* The reference tree yields **61 chart fixtures** plus the smoke file. The tool
  prints a per-chart summary (`step= beat= measure= rows= length=<ms>ms`) and a
  final `charts: 61` line.

## Asset discovery

V-Slice stores the JSON data and the audio in different places:

* chart/metadata JSON: `assets/preload/data/songs/<id>/<id>-chart[-<variation>].json`
  and `<id>-metadata[-<variation>].json`;
* audio: `assets/songs/<id>/Inst.ogg` (default instrumental).

`--assets` is the game's `assets/` root. The runner first looks for
`preload/data/songs` and falls back to `songs` (the path written in the M4
contract), so a flattened test tree also works. Chart files are discovered by
name, sorted by fixture id, and each one gets exactly one fixture:

* `<id>-chart.json` → `tests/fixtures/conductor/<id>-default.jsonl`
* `<id>-chart-erect.json` → `tests/fixtures/conductor/<id>-erect.jsonl`
* `<id>-chart-pico.json` → `tests/fixtures/conductor/<id>-pico.jsonl`

The metadata file is selected exactly like
`SongRegistry.loadEntryMetadataFile` (`SongRegistry.hx:L464-477`): the default
variation uses `<id>-metadata.json`; a variation uses
`<id>-metadata-<variation>.json`. One timeline per chart **file**, not per
difficulty: the conductor depends only on that variation's `timeChanges` and
`offsets`.

`_smoke.jsonl` is hand-crafted in `src/OracleFixture.hx`: 7/8, 120 BPM with two
BPM changes (150 BPM at 1000 ms, 90 BPM at 2500 ms), 4000 ms instrumental. It is
not a chart fixture. Its name begins with `_`, and the future
`cargo xtask fixtures-conductor` gate must skip `_`-prefixed files when it counts
the 61 charts.

## Offsets

`combinedOffset = instrumentalOffset + formatOffset + globalOffset`
(`Conductor.hx:L294-299`). The fixture records each component plus `audioVisualOffset`:

* `iOff` = `offsets.instrumental` from that variation's metadata. The game calls
  `getInstrumentalOffset(currentInstrumental)`; the default `currentInstrumental`
  is `''`, and `SongOffsets.getInstrumentalOffset` returns the plain
  `instrumental` field for `null`/`''` (`SongData.hx:L235-282`). Alternate
  instrumentals are out of scope in M4, so `altInstrumentals` never changes the
  fixture. All 61 reference metadata files currently have `instrumental = 0`
  (one file has only `altInstrumentals`/`altVocals`, which are ignored here).
* `fOff` = 0: desktop build, `EXT_SOUND = 'ogg'` (`Constants.hx:L618`); the
  MP3 delay is web-only (`Constants.hx:L430`, `PlayState.hx:L1159-1163`).
* `avOff` = 0: no save file is loaded by the oracle
  (`Conductor.hx:L282-292`).
* `gOff` = 0: `Preferences.globalOffset` defaults to 0 without a save
  (`Preferences.hx:L317-322`). It is an `Int` upstream.
* `combined` is the sum above, **not** including `avOff`, exactly as
  `Conductor.getCombinedOffset` does.

## Simulation contract

One continuous 1 ms simulation per chart, in two phases:

1. **Pre-roll** (`music.playing = false`). After `mapTimeChanges`, take
   `pre = beatLengthMs * -5 + startTimestamp` (`PlayState.hx:L878`) and call
   `update(pre)`. The M4 fixture contract freezes `applyOffsets = false` for this
   phase; §0 notes the L883 call omits the argument (default `true`). With this corpus
   `combinedOffset = 0`, so the two are numerically identical; `input` always
   records the argument before any offset addition.
   Then, per simulated millisecond:
   `update(conductor.songPosition + 1.0, false)` until
   `songPosition >= startTimestamp + combinedOffset`
   (`PlayState.hx:L1147-1158`, `startTimestamp = 0`).
2. **Playback** (`music.playing = true`, `music.length` = probed duration).
   `music.time` advances from 0 to `length` in 1 ms steps; the input is
   `music.time + combinedOffset` (the perfect-audio form of the lerp at
   `PlayState.hx:L1172-1180`); `update(input, false)`. The L440-444 clamp is
   active and `clampActive` is `true` on every playback row. The 1 ms grid stops
   at `floor(length)`; no fractional endpoint is synthesized.

`mapTimeChanges` (`Conductor.hx:L549-600`) sorts by timestamp, clamps negative
timestamps to 0 with `beatTime = 0`, and recomputes later `beatTime`s with
`roundDecimal(prev.beatTime + (ts - prev.ts) / 60000 * prev.bpm * (prev.tsDen / 4), 4)`.
The supplied `b` values are ignored, as in the source.

### Rounding (amended 2026-11-05)

The installed HaxeFlixel implements
`FlxMath.roundDecimal(value, precision)` as:

```haxe
var mult:Float = 1;
for (i in 0...precision) mult *= 10;
return Math.fround(value * mult) / mult;
```

([`.haxelib/flixel/git/flixel/math/FlxMath.hx:L68-76`]). `Math.fround` is a
single-precision (binary32) escape; everything else in the conductor is f64.
The oracle mirrors that formula. It deliberately does **not** call Haxe's
`Math.fround` directly: on Neko 2.4.1 and `--interp`, the standard library's
`math_fround` symbol is missing and Haxe silently falls back to `Math.round`
(an Int). Instead `OracleConductor.fround` performs an IEEE-754 binary32
round-trip through `haxe.io.Bytes.setFloat`/`getFloat`, which is the same
round-to-nearest-even conversion `Math.fround` performs on the game's hxcpp
runtime. All other arithmetic remains f64.

### Boundary rows and `sim`

A JSONL row is emitted only when an integer step, beat, or measure **changes**
in an update. Within one update the dispatch order is step → beat → measure.
The row is the post-update conductor state. `i` is the new integer and may be
negative during pre-roll.

`sim` is the runner's simulation clock in milliseconds: `0` for the initial
pre-roll update, then `+1` per countdown/playback millisecond. It is not the
audio position; `input` and `songPosition` carry that. `sim` is part of the
frozen snapshot.

Field order (exactly this, one compact object per line, no spaces):

```
t,i,sim,input,songPosition,stepTime,beatTime,measureTime,
step,beat,measure,beatLen,stepLen,measureLen,
tc,tsNum,tsDen,bpm,iOff,fOff,avOff,gOff,combined,clampActive
```

* `t` is `"step" | "beat" | "measure"`.
* `tc` is the index into `timeChanges` (stable after mapping), or `-1` when the
  timeline is empty.
* Floats use Haxe `Std.string` semantics (no forced trailing zeros); integer
  fields print without a decimal point. Neko prints floats with 15 significant
  digits, which is part of this snapshot.
* `stepTime` is round-6 in the positive/current-time-change branch and round-4
  in the constant-BPM branch; `beatTime = stepTime / 4`; `measureTime` is
  `getTimeInMeasures` in the positive branch and `stepTime / stepsPerMeasure` in
  the constant branch (the source does not round `measureTime`).

### Empty `timeChanges` (ADR-0006)

If a metadata file has no `timeChanges` (or an empty array), the runner still
writes a fixture using the permissive defaults: `tc = -1`, default `bpm = 100`,
`tsNum = tsDen = 4`, and the constant-BPM branch of `Conductor.hx:L478-487`.
This is the §0.5 branch, matching `docs/DECISIONS.md` ADR-0006. The real source
also has a warning branch when `timeChanges` is empty and a music object exists
(`Conductor.hx:L463-466`); that branch leaves the timing fields stale. The M4
contract freezes the permissive timing branch instead. None of the 61 reference
metadata files takes this path.

## Fixture snapshot and validation

The fixtures are committed ground truth. Regeneration must be byte-identical:

```bash
bash tools/oracle/snapshot_validate.sh
```

The script:

1. prints the sha256 of every cited Haxe source file and of every cited line
   range (the `Verbatim.hx` table, including `Conductor.hx` and `FlxMath.hx`);
2. builds the Neko module into `/tmp/opencode/oracle-build/`;
3. runs the tool twice into two fresh `/tmp/opencode/` directories;
4. asserts the two runs are byte-identical, and that they match the committed
   `tests/fixtures/conductor/` files byte-for-byte;
5. asserts exactly 61 chart fixtures plus `_smoke.jsonl`;
6. prints the whole-set sha256 for the committed set and both regenerated sets.

Set hash command used by the script (run inside the fixture directory):

```bash
LC_ALL=C sha256sum *.jsonl | sort | sha256sum
```

Fixtures depend on the audio files and ffprobe output: the length enters the
clamp. Regenerate from the same reference assets, or the snapshot will (correctly)
contribute a different hash.

## Files

```
tools/oracle/
  README.md                 this file
  build.hxml                Neko build script (output under /tmp/opencode/)
  snapshot_validate.sh      citation hashes + rebuild determinism + fixture equality
  src/Main.hx               CLI, discovery orchestration, summaries, smoke write
  src/OracleAssets.hx       read-only asset discovery, metadata parse, ffprobe
  src/OracleConductor.hx    Conductor transcription, fround roundDecimal, citations
  src/OracleTimeChange.hx   SongTimeChange transcription
  src/OracleFixture.hx      pre-roll/playback simulation + JSONL format + smoke chart
  src/Verbatim.hx           citation/audit table
tests/fixtures/conductor/   61 generated chart snapshots + _smoke.jsonl
```

The game tree is read-only for this tool and nothing outside
`tools/oracle/` and `tests/fixtures/conductor/` is modified.
