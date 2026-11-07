# tools/oracle — conductor model and native utility probes

This directory produces deterministic **model-only** conductor timelines. It is not
an independent execution of the real game Conductor. Native probes validate the
rounding utility only, to the extent specified in [BEHAVIOR.md](BEHAVIOR.md). M4 input,
replay and jitter work remain pending. ADR-0014 authorizes correction of the former
binary32 interpretation and reviewed replacement of erroneous expectations.

## Evidence and toolchain

Verified here: Haxe 4.3.7, Neko 2.4.1, installed hxcpp, Linux native C++ compiler,
ffprobe 8.1.3. Exact source/input/generated-code hashes and tool versions are in
`provenance.json`; exact utility stdout is in `rounding-probe.txt`.

Haxe `Math.fround` on hxcpp returns `floor(x + 0.5)` as a floating-point integer.
It is **not** a binary32 conversion. The model uses `Math.ffloor(x + 0.5)` to avoid
Neko's potentially Int-valued fround fallback. All timing arithmetic remains f64.
The real-class probe imports installed `FlxMath.roundDecimal` unchanged, removes only
unrelated fields, and supplies two non-executable graphics import shims. See the
behavior specification for the exact evidence/limitations and 10 discriminating cases.

No network access is used. The game tree is read-only. Outputs go to ignored `target/`
or fresh temporary directories; no fixed global `/tmp` directory is cleared.

## Reproduce the native real-utility probe

From the repository root (Haxe/native builds have bounded runtime):

```bash
bash tools/oracle/probe_rounding.sh
```

Set `FUNKIN_DIR` for a different installed game tree (default
`~/Descargas/fnf/Funkin`). The script prints the build directory under `target/`,
source/generated-code hashes, native utility results, and Neko model results.
`NativeRoundProbe.hx` is an additional, separate runtime scaling harness; it does
not import Flixel and must not be presented as real-class evidence on its own.

## Generate candidates — never automatically repair fixtures

```bash
# From tools/oracle; ../.. is the workspace root.
haxe build.hxml
neko ../../target/oracle.n \
  --assets /home/alex/Descargas/fnf/Funkin/assets \
  --out ../../target/conductor-candidates
```

CLI: `--assets <assets-root> --out <candidate-dir>`; optional `--only <song|fixture>`
(repeatable/comma-separated, debugging only), `--citations`, `--help`.
Generation writes `_smoke.jsonl` and `_manifest.json` too. A filtered output cannot
certify the full 61-chart Rust gate. Neko is the canonical snapshot runtime; interpreter
float formatting is not byte-interchangeable. Generation sorts chart IDs and has no
wall-clock component. Assets/ffprobe output must match the recorded provenance.

The reference tree has JSON under `assets/preload/data/songs/<id>/` (fallback:
`assets/songs/<id>/`) and audio under `assets/songs/<id>/Inst.ogg`. One fixture per
chart **file/variation**, not per difficulty. Metadata selection uses the same suffix:
`<id>-metadata[-<variation>].json`. The default instrumental is used even for variations.
Only `offsets.instrumental` is selected; alt instrumental/vocal maps are out of scope.
No save is loaded, so global/audio-visual offsets are zero; desktop OGG format offset
is zero. Combined offset excludes audio-visual offset.

## Review, then replace only with authorization

```bash
python3 tools/oracle/review_candidates.py \
  --before tests/fixtures/conductor --after target/conductor-candidates \
  --out target/conductor-candidate-review.json
```

This is a read-only comparison of both directories. For ADR-0014 the committed
`candidate-review.json` records the original erroneous and corrected hashes plus
semantic changes. Re-running against already corrected expectations naturally yields
a different (empty) review, not a recreation of the historical before-set.
Do not overwrite frozen expectations merely to make validation pass. The ADR-0014
replacement retained all counts/per-kind indices and did not relax tolerances.

## Verify frozen expectations

```bash
bash tools/oracle/snapshot_validate.sh
cargo xtask fixtures-conductor
cargo xtask fixtures-conductor --fixtures tests/fixtures/conductor
```

Snapshot verification builds Neko twice, runs it twice into fresh private temporary
directories, verifies deterministic binary/output bytes, then compares all JSONL files
**and the replay manifest** against frozen expectations. It hashes every cited source
and range, including `hxMath.h:28`, prints the set hash, and removes only its own fresh
temporary directory. Overrides: `FUNKIN_DIR`, `FUNKIN_ASSETS`, `FFPROBE`, `TMPDIR`.
Missing/empty frozen fixtures fail; verification never regenerates them in place.

The Rust gate requires the exact 61 named chart files in `corpus.txt` and valid replay
metadata. Smoke must pass separately, never count as a chart. It replays every update
(including silent ones), checks update groups and boundary order/indices/counts,
keeps <= 1 ms timing and 1e-6 relative beat/step-length tolerances, and reports its
model-only evidence boundary. No original assets, Haxe toolchain or ffprobe are needed
for the Rust check. See BEHAVIOR.md for row schema and compact input-sequence recipe.

Set hash (including replay inputs):

```bash
# Run inside tests/fixtures/conductor
LC_ALL=C sha256sum *.jsonl _manifest.json | sort | sha256sum
```

## Integrated Rust validation

The gate is wired into `xtask` and the explicit M4+ profiles. It embeds the approved
replay manifest at compile time: an alternate `--fixtures` directory cannot substitute
61 renamed smoke descriptors for the real corpus. Missing or changed inputs/counts
fail before replay. Numerical tolerances remain unchanged.

```bash
python3 tools/workflow.py run --timeout 600 -- cargo test --locked -p oxide-audio
python3 tools/workflow.py run --timeout 600 -- cargo test --locked -p xtask --test conductor_gate
python3 tools/workflow.py run --timeout 600 -- cargo xtask fixtures-conductor
```

Integration expectations and the active M4 contract were corrected under ADR-0014.
See `../../docs/WORKFLOW-REPORT.md` for actual validation commands and limitations.
A conductor/model pass alone must not mark M4 complete: the static acceptance catalogue
also requires independent native Conductor traces and the pending input/replay/jitter work.
