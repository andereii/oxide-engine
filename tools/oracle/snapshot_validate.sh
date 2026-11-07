#!/usr/bin/env bash
#
# M4 oracle snapshot validation.
#
#   1. prints sha256 of every cited reference source (the habitat of the
#      transcription) and of every cited line range in tools/oracle/src/
#      Verbatim.hx;
#   2. builds the Neko module twice from source into fresh /tmp dirs;
#   3. runs the oracle twice into two fresh fixture dirs and asserts the
#      outputs are byte-identical;
#   4. asserts both runs are byte-identical to the committed
#      tests/fixtures/conductor/ files;
#   5. asserts exactly 61 chart fixtures plus _smoke.jsonl;
#   6. prints the whole-set sha256 for the committed set and both runs.
#
# The reference game tree is only read. No network access is used.
# Override locations with FUNKIN_DIR / FUNKIN_ASSETS / FFPROBE.
set -euo pipefail
export LC_ALL=C

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/conductor"
GAME="${FUNKIN_DIR:-$HOME/Descargas/fnf/Funkin}"
ASSETS="${FUNKIN_ASSETS:-$GAME/assets}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/oxide-conductor-snapshot.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
BUILD_A="$WORK/build-a"
BUILD_B="$WORK/build-b"
RUN_A="$WORK/run-a"
RUN_B="$WORK/run-b"
DIFF_AB="$WORK/diff-ab.txt"

fail() {
  echo "snapshot_validate: $*" >&2
  exit 1
}

[[ -d "$GAME" ]] || fail "reference game tree not found: $GAME (set FUNKIN_DIR)"
[[ -d "$ASSETS" ]] || fail "reference assets not found: $ASSETS (set FUNKIN_ASSETS)"
[[ -d "$FIXTURES" ]] || fail "committed fixture directory not found: $FIXTURES"
[[ -s "$FIXTURES/_manifest.json" ]] || fail "missing/empty committed replay manifest"
for fixture in "$FIXTURES"/*.jsonl; do
  [[ -s "$fixture" ]] || fail "missing/empty committed fixture: $fixture (never auto-generated)"
done

command -v haxe >/dev/null 2>&1 || fail "haxe is not on PATH"
command -v neko >/dev/null 2>&1 || fail "neko is not on PATH"
command -v ffprobe >/dev/null 2>&1 || fail "ffprobe is not on PATH"

echo "== citation habitat: full-file sha256 =="
CITED_FILES=(
  "source/funkin/Conductor.hx"
  "source/funkin/data/song/SongData.hx"
  "source/funkin/data/song/SongDataUtils.hx"
  "source/funkin/util/Constants.hx"
  "source/funkin/play/PlayState.hx"
  "source/funkin/Preferences.hx"
  ".haxelib/flixel/git/flixel/math/FlxMath.hx"
  ".haxelib/hxcpp/git/include/hxMath.h"
)
for file in "${CITED_FILES[@]}"; do
  [[ -f "$GAME/$file" ]] || fail "cited source missing: $GAME/$file"
  printf '%s  %s\n' "$(sha256sum "$GAME/$file" | cut -d' ' -f1)" "$file"
done

echo
echo "== citation habitat: cited line-range sha256 (Verbatim.hx table) =="
citation() {
  local file="$1"
  local range="$2"
  local sed_range="${range#L}"
  sed_range="${sed_range/-/,}"
  local lines
  lines="$(sed -n "${sed_range}p" "$GAME/$file" | wc -l)"
  [[ "$lines" -gt 0 ]] || fail "cited range is empty: $file:$range"
  printf '%s  %s:%s\n' \
    "$(sed -n "${sed_range}p" "$GAME/$file" | sha256sum | cut -d' ' -f1)" \
    "$file" "$range"
}

# Keep this list in sync with tools/oracle/src/Verbatim.hx.
CITATIONS=(
  "source/funkin/Conductor.hx:L21-46"
  "source/funkin/Conductor.hx:L96-101"
  "source/funkin/Conductor.hx:L123-140"
  "source/funkin/Conductor.hx:L166-171"
  "source/funkin/Conductor.hx:L176-182"
  "source/funkin/Conductor.hx:L187-192"
  "source/funkin/Conductor.hx:L197-216"
  "source/funkin/Conductor.hx:L218-246"
  "source/funkin/Conductor.hx:L251"
  "source/funkin/Conductor.hx:L268-299"
  "source/funkin/Conductor.hx:L301-319"
  "source/funkin/Conductor.hx:L422-517"
  "source/funkin/Conductor.hx:L549-600"
  "source/funkin/Conductor.hx:L607-647"
  "source/funkin/data/song/SongData.hx:L157-203"
  "source/funkin/data/song/SongData.hx:L235-282"
  "source/funkin/data/song/SongDataUtils.hx:L267-275"
  "source/funkin/util/Constants.hx:L261"
  "source/funkin/util/Constants.hx:L326-331"
  "source/funkin/util/Constants.hx:L368"
  "source/funkin/util/Constants.hx:L373"
  "source/funkin/util/Constants.hx:L424"
  "source/funkin/util/Constants.hx:L430"
  "source/funkin/util/Constants.hx:L618"
  "source/funkin/play/PlayState.hx:L860-883"
  "source/funkin/play/PlayState.hx:L1142-1158"
  "source/funkin/play/PlayState.hx:L1172-1180"
  "source/funkin/play/PlayState.hx:L2634-2665"
  ".haxelib/flixel/git/flixel/math/FlxMath.hx:L68-76"
  ".haxelib/hxcpp/git/include/hxMath.h:L28"
  "source/funkin/Preferences.hx:L317-322"
)
for spec in "${CITATIONS[@]}"; do
  file="${spec%:*}"
  range="${spec##*:}"
  citation "$file" "$range"
done

echo
echo "== build (twice, fresh /tmp dirs) =="
prepare_build_dir() {
  local dir="$1"
  mkdir -p "$dir"
}
prepare_build_dir "$BUILD_A"
prepare_build_dir "$BUILD_B"
(cd "$HERE" && timeout 30s haxe -cp src -main Main -neko "$BUILD_A/oracle.n")
(cd "$HERE" && timeout 30s haxe -cp src -main Main -neko "$BUILD_B/oracle.n")
(cd "$HERE" && timeout 30s haxe -cp src -main ModelRoundProbe -neko "$BUILD_A/round.n")
timeout 10s neko "$BUILD_A/round.n"
build_hash_a="$(sha256sum "$BUILD_A/oracle.n" | cut -d' ' -f1)"
build_hash_b="$(sha256sum "$BUILD_B/oracle.n" | cut -d' ' -f1)"
echo "build A: $build_hash_a"
echo "build B: $build_hash_b"
[[ "$build_hash_a" == "$build_hash_b" ]] || fail "the two Haxe builds are not byte-identical"

prepare_run_dir() {
  local dir="$1"
  mkdir -p "$dir"
}
prepare_run_dir "$RUN_A"
prepare_run_dir "$RUN_B"

echo
echo "== regenerate twice =="
timeout 120s neko "$BUILD_A/oracle.n" --assets "$ASSETS" --out "$RUN_A" > "$RUN_A.log" 2>&1
timeout 120s neko "$BUILD_B/oracle.n" --assets "$ASSETS" --out "$RUN_B" > "$RUN_B.log" 2>&1

echo "-- run A summary (first 3 and last 2 lines) --"
sed -n '1,3p' "$RUN_A.log"
echo "..."
tail -2 "$RUN_A.log"

chart_count_a="$(find "$RUN_A" -maxdepth 1 -type f -name '*.jsonl' ! -name '_*' | wc -l)"
chart_count_b="$(find "$RUN_B" -maxdepth 1 -type f -name '*.jsonl' ! -name '_*' | wc -l)"
chart_count_committed="$(find "$FIXTURES" -maxdepth 1 -type f -name '*.jsonl' ! -name '_*' | wc -l)"
[[ "$chart_count_a" -eq 61 ]] || fail "run A produced $chart_count_a chart fixtures, expected 61"
[[ "$chart_count_b" -eq 61 ]] || fail "run B produced $chart_count_b chart fixtures, expected 61"
[[ "$chart_count_committed" -eq 61 ]] || fail "committed set has $chart_count_committed chart fixtures, expected 61"
[[ -f "$RUN_A/_smoke.jsonl" ]] || fail "run A did not produce _smoke.jsonl"
[[ -f "$RUN_B/_smoke.jsonl" ]] || fail "run B did not produce _smoke.jsonl"
[[ -f "$FIXTURES/_smoke.jsonl" ]] || fail "committed set has no _smoke.jsonl"

echo
echo "== determinism: run A vs run B =="
if diff -rq "$RUN_A" "$RUN_B" > "$DIFF_AB"; then
  echo "run A and run B are byte-identical (61 charts + _smoke.jsonl + replay manifest)"
else
  cat "$DIFF_AB" >&2
  fail "run A and run B differ"
fi

echo
echo "== committed fixtures vs regenerated run A =="
committed_names="$(cd "$FIXTURES" && printf '%s\n' *.jsonl | sort)"
run_a_names="$(cd "$RUN_A" && printf '%s\n' *.jsonl | sort)"
if [[ "$committed_names" != "$run_a_names" ]]; then
  diff <(printf '%s\n' "$committed_names") <(printf '%s\n' "$run_a_names") >&2 || true
  fail "committed fixture names differ from regenerated names"
fi
for path in "$RUN_A"/*.jsonl; do
  name="$(basename "$path")"
  cmp -s "$path" "$FIXTURES/$name" || fail "committed fixture differs: $name"
done
cmp -s "$RUN_A/_manifest.json" "$FIXTURES/_manifest.json" || fail "committed replay manifest differs"
echo "committed fixtures and replay manifest are byte-identical to run A"

set_hash() {
  (cd "$1" && sha256sum *.jsonl _manifest.json | sort | sha256sum | cut -d' ' -f1)
}

committed_hash="$(set_hash "$FIXTURES")"
run_a_hash="$(set_hash "$RUN_A")"
run_b_hash="$(set_hash "$RUN_B")"

echo
echo "== whole-set sha256 (LC_ALL=C sha256sum *.jsonl _manifest.json | sort | sha256sum) =="
echo "committed: $committed_hash"
echo "run A:     $run_a_hash"
echo "run B:     $run_b_hash"

[[ "$committed_hash" == "$run_a_hash" ]] || fail "committed set hash differs from run A"
[[ "$run_a_hash" == "$run_b_hash" ]] || fail "run A and run B set hashes differ"

echo
echo "snapshot_validate: OK (61 chart fixtures + _smoke.jsonl + replay manifest, byte-identical; model-only full timelines)"
