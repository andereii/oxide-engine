#!/usr/bin/env bash
# Native real-Conductor execution against the frozen replay recipe.
#
# Human approval 2026-11-06: build a read-only hxcpp probe harness (ProbeSlice-
# style non-invasive slicing; the original Funkin' checkout is never modified)
# that executes the real funkin.Conductor on the frozen 61-chart + smoke
# recipe, to satisfy the M4 `conductor_native_reference` gate.
#
# Evidence boundary: this is a REAL Conductor execution on the same simulated
# inputs as the model (no hardware, no audio engine, simulated music time) and
# NOT a real-game full-play trace. See tools/oracle/BEHAVIOR.md.
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/conductor"
NATIVE_FIXTURES="$ROOT/tests/fixtures/conductor-native"
GAME="${FUNKIN_DIR:-$HOME/Descargas/fnf/Funkin}"
FLIXEL="$GAME/.haxelib/flixel/git"
HXCPP="$GAME/.haxelib/hxcpp/git"
[[ -f "$GAME/source/funkin/Conductor.hx" && -d "$FLIXEL" && -f "$HXCPP/hxcpp.n" ]] || {
  echo "native_conductor_probe: installed game/Flixel/hxcpp missing (set FUNKIN_DIR)" >&2
  exit 1
}
[[ -f "$FIXTURES/_manifest.json" ]] || {
  echo "native_conductor_probe: model manifest missing" >&2
  exit 1
}
mkdir -p "$ROOT/target"
WORK="$(mktemp -d "$ROOT/target/conductor-native-probe.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
echo "probe build: $WORK"

# Module scope: byte-exact copies of the REAL retained modules (hashes logged)
# so no classpath brings in the real game import.hx (which would pull the
# graphics/gameplay registry). The game tree is never written to.
GAMECOPY="$WORK/game-copy"
mkdir -p "$GAMECOPY/funkin/util/tools"
SHAS="$WORK/copied-sha256.txt"
cp "$GAME/source/funkin/Conductor.hx"             "$GAMECOPY/funkin/Conductor.hx"
cp "$GAME/source/funkin/util/tools/FloatTools.hx" "$GAMECOPY/funkin/util/tools/FloatTools.hx"
cp "$GAME/source/funkin/util/AnsiUtil.hx"         "$GAMECOPY/funkin/util/AnsiUtil.hx"
cp "$HERE/probe-shim/funkin/import.hx"            "$GAMECOPY/funkin/import.hx"
sha256sum "$GAMECOPY/funkin/Conductor.hx" "$GAMECOPY/funkin/util/tools/FloatTools.hx" \
  "$GAMECOPY/funkin/util/AnsiUtil.hx" \
  > "$SHAS"

echo "probe compile:"
haxe --version
sha256sum "$FIXTURES/_manifest.json" | awk '{print "input-manifest-sha256: "$1}' > "$WORK/provenance.txt"
awk 'NR==1 {print "copied-conductor-sha256: "$1}' "$SHAS" >> "$WORK/provenance.txt"
awk 'NR==2 {print "copied-floattools-sha256: "$1}' "$SHAS" >> "$WORK/provenance.txt"
awk 'NR==3 {print "copied-ansiutil-sha256: "$1}' "$SHAS" >> "$WORK/provenance.txt"
sha256sum "$GAME/source/funkin/util/tools/FloatTools.hx" | awk '{print "floattools-source-sha256: "$1}' >> "$WORK/provenance.txt"
sha256sum "$GAME/source/funkin/util/AnsiUtil.hx" | awk '{print "ansiutil-source-sha256: "$1}' >> "$WORK/provenance.txt"
sha256sum "$GAME/source/funkin/Conductor.hx" | awk '{print "conductor-source-sha256: "$1}' >> "$WORK/provenance.txt"
sha256sum "$FLIXEL/flixel/math/FlxMath.hx" | awk '{print "flxmath-source-sha256: "$1}' >> "$WORK/provenance.txt"
sha256sum "$FLIXEL/flixel/util/FlxSort.hx" | awk '{print "flxsort-source-sha256: "$1}' >> "$WORK/provenance.txt"
sha256sum "$HERE/probe-shim/funkin/import.hx" | awk '{print "shim: "$1}' >> "$WORK/provenance.txt"

# Haxe macro: retain only the real Conductor fields the probe path needs; the
# retained bodies stay byte-identical (macex only filters fields like the
# FlxMath probe does).
timeout 120s haxe -cp "$HERE/src" -cp "$GAMECOPY" -cp "$FLIXEL" -cp "$HERE/probe-types" \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("flixel.math.FlxMath", "@:build(ProbeSlice.keepRoundDecimal())", false, true, false)' \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("funkin.Conductor", "@:build(ConductorSlice.keepTimingPath())", false, true, false)' \
  -main NativeConductorProbe -cpp "$WORK/native" -D no-compilation -dce full
(
  cd "$WORK/native"
  timeout 240s env HOME="$WORK/home" HAXELIB_PATH="$GAME/.haxelib" HXCPP_COMPILE_THREADS=2 \
    neko "$HXCPP/hxcpp.n" Build.xml "-DHXCPP=$HXCPP" -DHXCPP_M64 -Dlinux > /dev/null 2>&1
)
echo "probe run:"
PROBE_INPUT_JSON="$FIXTURES/_manifest.json" \
PROBE_OUT_DIR="$WORK/native-timelines" \
timeout 300s "$WORK/native/NativeConductorProbe" 2> "$WORK/probe-stderr.txt" > /dev/null
sha256sum "$WORK/native/Conductor.cpp" 2> /dev/null | awk '{print "generated-cpp-sha256: "$1}' >> "$WORK/provenance.txt" || true

# Native vs frozen model rows: field-wise comparison at the gate tolerances.
if ! python3 "$ROOT/tools/oracle/compare_native_rows.py" \
    --input "$WORK/native-timelines" --model "$FIXTURES" \
    --report "$WORK/compare-report.json"; then
  cp -f "$WORK/compare-report.json" "$ROOT/target/conductor-native-compare-fail.json" 2>/dev/null || true
  echo "native_conductor_probe: comparison FAILED (report: target/conductor-native-compare-fail.json)" >&2
  exit 1
fi

# Row digest set: per-file sha256 for the freeze manifest.
( cd "$WORK/native-timelines" && sha256sum ./*.jsonl ) > "$WORK/row-digests.txt"

# Publish artifacts into target/conductor-native (not fixtures) for review.
mkdir -p "$ROOT/target/conductor-native"
cp -a "$WORK/native-timelines/." "$ROOT/target/conductor-native/"
cp -f "$WORK/compare-report.json" "$ROOT/target/conductor-native-compare-report.json"
cp -f "$WORK/row-digests.txt" "$ROOT/target/conductor-native-row-digests.txt"
cp -f "$WORK/provenance.txt" "$ROOT/target/conductor-native-provenance.txt"
echo "native_conductor_probe: compare report at target/conductor-native-compare-report.json (not proving per-row byte equality; see BEHAVIOR.md)"
