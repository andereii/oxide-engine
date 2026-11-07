#!/usr/bin/env bash
# Native real-utility evidence only. All build/config outputs stay under target/.
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GAME="${FUNKIN_DIR:-$HOME/Descargas/fnf/Funkin}"
FLIXEL="$GAME/.haxelib/flixel/git"
HXCPP="$GAME/.haxelib/hxcpp/git"
[[ -f "$FLIXEL/flixel/math/FlxMath.hx" && -f "$HXCPP/hxcpp.n" ]] || {
  echo "probe_rounding: installed Flixel/hxcpp missing (set FUNKIN_DIR)" >&2
  exit 1
}
mkdir -p "$ROOT/target"
WORK="$(mktemp -d "$ROOT/target/conductor-round-probe.XXXXXX")"
mkdir -p "$WORK/home"
echo "probe build: $WORK"
haxe --version
neko -version
sha256sum "$FLIXEL/flixel/math/FlxMath.hx" "$HXCPP/include/hxMath.h"
# The compiler loads the actual class from FLIXEL. Only unrelated fields are
# removed, before typing; two extern import shims have no executable behavior.
timeout 30s haxe -cp "$HERE/src" -cp "$FLIXEL" -cp "$HERE/probe-types" \
  --macro 'haxe.macro.Compiler.addGlobalMetadata("flixel.math.FlxMath", "@:build(ProbeSlice.keepRoundDecimal())", false, true, false)' \
  -main RealRoundProbe -cpp "$WORK/native" -D no-compilation -dce full -D analyzer-optimize
(
  cd "$WORK/native"
  timeout 120s env HOME="$WORK/home" HAXELIB_PATH="$GAME/.haxelib" HXCPP_COMPILE_THREADS=2 \
    neko "$HXCPP/hxcpp.n" Build.xml "-DHXCPP=$HXCPP" -DHXCPP_M64 -Dlinux
)
timeout 10s "$WORK/native/RealRoundProbe" | tee "$WORK/real-utility.txt"
sha256sum "$WORK/native/src/flixel/math/FlxMath.cpp" "$WORK/native/RealRoundProbe"
timeout 30s haxe -cp "$HERE/src" -main ModelRoundProbe -neko "$WORK/model.n"
timeout 10s neko "$WORK/model.n" | tee "$WORK/model.txt"
echo "probe_rounding: OK (real native utility + Neko model cases; not full-game conductor evidence)"
