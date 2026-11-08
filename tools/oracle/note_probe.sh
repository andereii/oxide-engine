#!/usr/bin/env bash
# Native real-PlayState note-score probe (tools/oracle/NOTE-BEHAVIOR.md).
#
# Human approval 2026-11-06: build a read-only hxcpp probe harness that
# executes the REAL installed PlayState note-score path, Scoring (PBOT1) and
# GRhythmUtil over the exported bopeebo `note-stream-1` stream, to satisfy the
# M5 `native_note_reference` gate. The original Funkin' checkout is NEVER
# written; only byte-exact copies under target/ are compiled.
#
# Round 2 (PROBE_ROUND=2, driver `native-note-probe-2`, M6 contract §10.2):
# constructs the opponent notes/holds (chart lanes 4..7, chart array order,
# direction = data % 4) from the same bopeebo chart JSON and drives the REAL
# retained `processNotes` opponent loops. Player strumlines stay empty and the
# stream input events are not replayed, so the run emits only schema-2
# header-only `opponent_hit` rows. Multiple opponent hits per tick are allowed;
# any other dispatched event fails closed. The same stream files are consumed
# and re-hashed; round-1 artifacts stay untouched. Not native-game parity.
#
# CLI environment:
#   PROBE_STREAM_DIR   required: directory with *.stream.jsonl files
#   PROBE_OUT_DIR      required: directory for <difficulty>.jsonl rows
#   PROBE_ASSETS       optional: assets root (default $FUNKIN_DIR/assets)
#   PROBE_BUILD_DIR    optional: persistent build dir under target/ (reuse)
#   PROBE_ROUND        optional: 1 (default, player note-score path) or 2
#                      (opponent-side `opponent_hit` run)
#
# Evidence boundary: this is a REAL PlayState/Scoring/GRhythmUtil execution on
# a synthesized stream and simulated conductor time; NOT a native-game parity
# claim and NOT a human replay. Round 1 rows are emitted only for events
# dispatched by the retained path (hit/miss/ghost_press/hold_drop); engine-
# decision kinds hold_start/hold_tick/hold_end/game_over are not dispatched and
# have no row. Round 2 emits only the retained opponent HitNoteScriptEvent as
# `opponent_hit`; opponent hold/drop/sing effects have no trace row.
set -euo pipefail
export LC_ALL=C

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GAME="${FUNKIN_DIR:-$HOME/Descargas/fnf/Funkin}"
FLIXEL="$GAME/.haxelib/flixel/git"
HXCPP="$GAME/.haxelib/hxcpp/git"
PROBE_ASSETS="${PROBE_ASSETS:-$GAME/assets}"

: "${PROBE_STREAM_DIR:?PROBE_STREAM_DIR is required (directory with *.stream.jsonl)}"
: "${PROBE_OUT_DIR:?PROBE_OUT_DIR is required (row output directory)}"

fail() {
  echo "native-note-probe: FAIL: $*" >&2
  exit 2
}

PROBE_ROUND="${PROBE_ROUND:-1}"
case "$PROBE_ROUND" in
  1|2) ;;
  *) fail "PROBE_ROUND must be 1 or 2 (got '$PROBE_ROUND')" ;;
esac

[[ -f "$GAME/source/funkin/play/PlayState.hx" ]] || fail "installed PlayState.hx missing (set FUNKIN_DIR)"
[[ -f "$GAME/source/funkin/play/scoring/Scoring.hx" ]] || fail "installed Scoring.hx missing"
[[ -f "$GAME/source/funkin/util/GRhythmUtil.hx" ]] || fail "installed GRhythmUtil.hx missing"
[[ -f "$GAME/source/funkin/Conductor.hx" ]] || fail "installed Conductor.hx missing"
[[ -d "$FLIXEL" ]] || fail "installed Flixel git tree missing"
[[ -f "$HXCPP/hxcpp.n" ]] || fail "installed hxcpp.n missing"
[[ -d "$PROBE_STREAM_DIR" ]] || fail "PROBE_STREAM_DIR is not a directory: $PROBE_STREAM_DIR"
[[ -d "$PROBE_ASSETS" ]] || fail "PROBE_ASSETS is not a directory: $PROBE_ASSETS"
[[ -f "$PROBE_ASSETS/preload/data/songs/bopeebo/bopeebo-chart.json" ]] || fail "bopeebo chart missing under PROBE_ASSETS"
[[ -f "$PROBE_ASSETS/preload/data/songs/bopeebo/bopeebo-metadata.json" ]] || fail "bopeebo metadata missing under PROBE_ASSETS"

mkdir -p "$ROOT/target" "$PROBE_OUT_DIR"

if [[ -n "${PROBE_BUILD_DIR:-}" ]]; then
  WORK="$PROBE_BUILD_DIR"
  case "$WORK" in
    "$ROOT"/target/*) ;;
    *) fail "PROBE_BUILD_DIR must live under $ROOT/target (got $WORK)" ;;
  esac
  mkdir -p "$WORK"
  KEEP_BUILD=1
else
  WORK="$(mktemp -d "$ROOT/target/note-native-probe.XXXXXX")"
  KEEP_BUILD=0
  trap 'rm -rf "$WORK"' EXIT
fi
rm -rf "$WORK/native" "$WORK/game-copy" "$WORK/home"
mkdir -p "$WORK/home"
echo "native-note-probe: round $PROBE_ROUND; build dir $WORK"
if [[ "$PROBE_ROUND" == "1" ]]; then
  echo "native-note-probe honesty: synthesized stream; not native-game parity; driver hold rows follow NOTE-BEHAVIOR.md section 6.1"
else
  echo "native-note-probe honesty: round 2 opponent-side run; synthesized stream; not native-game parity; only schema-2 opponent_hit rows; opponent hold lifecycle excluded"
fi

# ---------------------------------------------------------------------------
# Byte-exact copies of the REAL retained modules (game tree stays read-only)
# ---------------------------------------------------------------------------
GAMECOPY="$WORK/game-copy"
mkdir -p "$GAMECOPY/funkin/play/scoring" "$GAMECOPY/funkin/util/tools"
cp "$GAME/source/funkin/play/PlayState.hx"          "$GAMECOPY/funkin/play/PlayState.hx"
cp "$GAME/source/funkin/play/scoring/Scoring.hx"    "$GAMECOPY/funkin/play/scoring/Scoring.hx"
cp "$GAME/source/funkin/util/GRhythmUtil.hx"        "$GAMECOPY/funkin/util/GRhythmUtil.hx"
cp "$GAME/source/funkin/Conductor.hx"               "$GAMECOPY/funkin/Conductor.hx"
cp "$GAME/source/funkin/util/tools/FloatTools.hx"   "$GAMECOPY/funkin/util/tools/FloatTools.hx"
cp "$GAME/source/funkin/util/tools/Int64Tools.hx"   "$GAMECOPY/funkin/util/tools/Int64Tools.hx"
cp "$GAME/source/funkin/util/AnsiUtil.hx"           "$GAMECOPY/funkin/util/AnsiUtil.hx"
cp "$HERE/note-probe/shim/funkin/import.hx"         "$GAMECOPY/funkin/import.hx"

PROVENANCE="$WORK/provenance.txt"
: > "$PROVENANCE"
record() { echo "$1" >> "$PROVENANCE"; }
record "driver: native-note-probe-${PROBE_ROUND}"
record "round: ${PROBE_ROUND}$([[ "$PROBE_ROUND" == "2" ]] && echo " (opponent notes/holds, lanes 4..7, chart array order; compacted opponent note_id; only opponent_hit rows)")"
record "probe-script-sha256: $(sha256sum "$HERE/note_probe.sh" | awk '{print $1}')"
record "probe-src-sha256: $(sha256sum "$HERE/note-probe/src/NativeNoteProbe.hx" | awk '{print $1}')"
record "sliver-playstate: $(sha256sum "$HERE/note-probe/src/NotePlayStateSlice.hx" | awk '{print $1}')"
record "sliver-scoring: $(sha256sum "$HERE/note-probe/src/NoteScoringSlice.hx" | awk '{print $1}')"
record "sliver-conductor: $(sha256sum "$HERE/note-probe/src/NoteConductorSlice.hx" | awk '{print $1}')"
record "runtime-sha256: $(sha256sum "$HERE/note-probe/src/NoteProbeRuntime.hx" | awk '{print $1}')"
record "defines: NO_FEATURE_GHOST_TAPPING (shipped release hxml; mobile/chart-editor/newgrounds/discord feature defines are not enabled)"
record "classpath-order: probe-src, game-copy, installed flixel, probe-modules (last wins for shadowed modules)"

for pair in \
  "funkin/play/PlayState.hx:$GAME/source/funkin/play/PlayState.hx" \
  "funkin/play/scoring/Scoring.hx:$GAME/source/funkin/play/scoring/Scoring.hx" \
  "funkin/util/GRhythmUtil.hx:$GAME/source/funkin/util/GRhythmUtil.hx" \
  "funkin/Conductor.hx:$GAME/source/funkin/Conductor.hx" \
  "funkin/util/tools/FloatTools.hx:$GAME/source/funkin/util/tools/FloatTools.hx" \
  "funkin/util/tools/Int64Tools.hx:$GAME/source/funkin/util/tools/Int64Tools.hx" \
  "funkin/util/AnsiUtil.hx:$GAME/source/funkin/util/AnsiUtil.hx"; do
  rel="${pair%%:*}"
  src="${pair#*:}"
  record "copied-sha256 $rel: $(sha256sum "$GAMECOPY/$rel" | awk '{print $1}')"
  record "original-sha256 $rel: $(sha256sum "$src" | awk '{print $1}')"
done
record "shim-import-sha256: $(sha256sum "$HERE/note-probe/shim/funkin/import.hx" | awk '{print $1}')"

# Shadow-module inventory (verbatim|shell|decision labels live in the files).
while IFS= read -r module; do
  rel="${module#$HERE/note-probe/modules/}"
  record "shadow-sha256 $rel: $(sha256sum "$module" | awk '{print $1}')"
done < <(find "$HERE/note-probe/modules" -type f -name '*.hx' | sort)

record "assets-root: $PROBE_ASSETS"
record "stream-dir: $PROBE_STREAM_DIR"
record "chart-path: $PROBE_ASSETS/preload/data/songs/bopeebo/bopeebo-chart.json"
record "chart-sha256: $(sha256sum "$PROBE_ASSETS/preload/data/songs/bopeebo/bopeebo-chart.json" | awk '{print $1}')"
record "metadata-path: $PROBE_ASSETS/preload/data/songs/bopeebo/bopeebo-metadata.json"
record "metadata-sha256: $(sha256sum "$PROBE_ASSETS/preload/data/songs/bopeebo/bopeebo-metadata.json" | awk '{print $1}')"
record "flxmath-source-sha256: $(sha256sum "$FLIXEL/flixel/math/FlxMath.hx" | awk '{print $1}')"
record "flxsort-source-sha256: $(sha256sum "$FLIXEL/flixel/util/FlxSort.hx" | awk '{print $1}')"

shopt -s nullglob
STREAMS=("$PROBE_STREAM_DIR"/*.stream.jsonl)
shopt -u nullglob
(( ${#STREAMS[@]} > 0 )) || fail "no *.stream.jsonl files under $PROBE_STREAM_DIR"
for stream in "${STREAMS[@]}"; do
  record "consumed-stream $(basename "$stream"): $(sha256sum "$stream" | awk '{print $1}') path=$stream"
done

# ---------------------------------------------------------------------------
# Compile once (macro whitelists filter FIELDS; retained bodies untouched)
# ---------------------------------------------------------------------------
echo "native-note-probe: haxe $(haxe --version)"
timeout 180s haxe \
  -cp "$HERE/note-probe/src" \
  -cp "$GAMECOPY" \
  -cp "$FLIXEL" \
  -cp "$HERE/note-probe/modules" \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("flixel.math.FlxMath", "@:build(NoteFlxMathSlice.keepRoundDecimal())", false, true, false)' \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("funkin.play.PlayState", "@:build(NotePlayStateSlice.keepNotePath())", false, true, false)' \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("funkin.play.scoring.Scoring", "@:build(NoteScoringSlice.keepPbot1())", false, true, false)' \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("funkin.Conductor", "@:build(NoteConductorSlice.keepTimingPath())", false, true, false)' \
  -D NO_FEATURE_GHOST_TAPPING \
  -main NativeNoteProbe -cpp "$WORK/native" -D no-compilation -dce full

echo "native-note-probe: hxcpp build"
(
  cd "$WORK/native"
  timeout 300s env HOME="$WORK/home" HAXELIB_PATH="$GAME/.haxelib" HXCPP_COMPILE_THREADS=2 \
    neko "$HXCPP/hxcpp.n" Build.xml "-DHXCPP=$HXCPP" -DHXCPP_M64 -Dlinux > "$WORK/hxcpp-build.log" 2>&1
) || {
  tail -n 40 "$WORK/hxcpp-build.log" >&2 || true
  fail "hxcpp Build.xml failed (log: $WORK/hxcpp-build.log)"
}

if [[ -f "$WORK/native/PlayState.cpp" ]]; then
  record "generated-playstate-cpp-sha256: $(sha256sum "$WORK/native/PlayState.cpp" | awk '{print $1}')"
fi
if [[ -f "$WORK/native/NativeNoteProbe" ]]; then
  record "binary-sha256: $(sha256sum "$WORK/native/NativeNoteProbe" | awk '{print $1}')"
fi

# ---------------------------------------------------------------------------
# Run once per stream. The binary derives the difficulty from the header and
# writes <PROBE_OUT_DIR>/<difficulty>.jsonl.
# ---------------------------------------------------------------------------
for stream in "${STREAMS[@]}"; do
  echo "native-note-probe: run $(basename "$stream")"
  PROBE_STREAM_JSON="$stream" \
  PROBE_OUT_DIR="$PROBE_OUT_DIR" \
  PROBE_ASSETS="$PROBE_ASSETS" \
  PROBE_ROUND="$PROBE_ROUND" \
    timeout 900s "$WORK/native/NativeNoteProbe"
done

cp -f "$PROVENANCE" "$PROBE_OUT_DIR/provenance.txt"
echo "native-note-probe: rows in $PROBE_OUT_DIR; provenance $PROBE_OUT_DIR/provenance.txt"
if [[ "$PROBE_ROUND" == "1" ]]; then
  echo "native-note-probe: honesty: synthesized stream; not native-game parity; input_time_us = 0.0 canonical; input latency term stubbed to 0"
else
  echo "native-note-probe: honesty: round 2 opponent-side run; synthesized stream; not native-game parity; only schema-2 opponent_hit rows; input_time_us = 0.0 canonical; input latency term stubbed to 0"
fi
if [[ "$KEEP_BUILD" == "1" ]]; then
  echo "native-note-probe: kept build dir $WORK"
fi
