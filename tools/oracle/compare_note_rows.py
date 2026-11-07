#!/usr/bin/env python3
"""Field-wise comparator for the native note probe rows.

Contract (tools/oracle/NOTE-BEHAVIOR.md section 6, docs/M5-CONTRACT.md section 2,
xtask/src/note_native_check.rs comparison rules):

- integers, booleans and strings are EXACT;
- f64 fields use an absolute tolerance of <= 1e-9 (the labelled gate tolerance);
- sims and event order are exact: row i of native is compared with row i of Rust;
- the JSON key order must match the normative serde field order for the row kind;
- a mismatch, a missing/extra file, an empty file, a malformed row or schema
  drift exits nonzero.

The native probe currently emits only the dispatchEvent-observable kinds
(hit, miss, ghost_press, hold_drop); the comparator itself is kind-agnostic and
will fail on engine-only Rust rows because they have no native counterpart.
That failure is an honest coverage finding, not hidden here.

Self-test: `python3 tools/oracle/compare_note_rows.py --selftest`.
"""
import argparse
import json
import math
import sys
import tempfile
from pathlib import Path

TOLERANCE = 1e-9
HEALTH_MAX = 2.0
KINDS = (
    "hit",
    "miss",
    "ghost_press",
    "hold_start",
    "hold_tick",
    "hold_drop",
    "hold_end",
    "game_over",
)
HEADER = [
    "schema", "kind", "sim", "input_ms", "input_time_us", "note_id",
    "note_direction", "judgement", "raw_note_diff", "note_diff_int",
]
SCHEMA = {
    "hit": HEADER + [
        "score_delta", "health_before", "health_after", "combo_before",
        "combo_after", "combo_max", "tallies",
    ],
    "miss": HEADER + [
        "score_delta", "health_before", "health_after", "combo_before",
        "combo_after", "combo_max", "tallies",
    ],
    "ghost_press": HEADER + ["score_delta", "health_change"],
    "hold_start": HEADER,
    "hold_tick": HEADER + [
        "health_delta", "score_delta", "hold_remaining_ms", "hold_elapsed_ms",
    ],
    "hold_drop": HEADER + [
        "score_penalty", "health_change", "combo_before", "combo_after",
        "is_combo_break",
    ],
    "hold_end": HEADER,
    "game_over": HEADER + ["death_count"],
}
TALLY_KEYS = ["sick", "good", "bad", "shit", "missed", "total_notes_hit"]


class CompareError(Exception):
    pass


def load_rows(path: Path):
    rows = []
    text = path.read_text(encoding="utf-8")
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            raise CompareError(f"{path}: line {number} is blank; fail-closed")
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise CompareError(f"{path}: line {number} is not valid JSON: {error}") from error
        if not isinstance(row, dict):
            raise CompareError(f"{path}: line {number} is not a JSON object")
        rows.append(row)
    if not rows:
        raise CompareError(f"{path} is empty; a reference trace cannot be empty")
    return rows


def check_schema(label: str, row: dict):
    kind = row.get("kind")
    if kind not in SCHEMA:
        raise CompareError(f"{label}: unknown row kind {kind!r}; schema drift")
    if row.get("schema") != "1":
        raise CompareError(f"{label}: schema is {row.get('schema')!r}, expected \"1\"")
    keys = list(row.keys())
    expected = SCHEMA[kind]
    if keys != expected:
        missing = [key for key in expected if key not in keys]
        extra = [key for key in keys if key not in expected]
        raise CompareError(
            f"{label}: {kind} field order/set differs from the normative schema; "
            f"missing {missing!r}, extra {extra!r}, got {keys!r}"
        )
    if kind in ("hit", "miss"):
        tallies = row.get("tallies")
        if not isinstance(tallies, dict) or list(tallies.keys()) != TALLY_KEYS:
            raise CompareError(
                f"{label}: tallies field order/set differs; expected {TALLY_KEYS!r}, "
                f"got {list(tallies.keys()) if isinstance(tallies, dict) else tallies!r}"
            )


def is_bool(value):
    return isinstance(value, bool)


def compare_value(native, rust, path: str, stats: dict):
    if is_bool(rust):
        if not is_bool(native) or native != rust:
            raise CompareError(f"{path}: bool native={native!r} Rust={rust!r}; booleans are exact")
        return
    if isinstance(rust, str):
        if not isinstance(native, str) or native != rust:
            raise CompareError(f"{path}: string native={native!r} Rust={rust!r}; strings are exact")
        return
    if rust is None:
        if native is not None:
            raise CompareError(f"{path}: native {native!r} != Rust null")
        return
    if isinstance(rust, bool):
        raise CompareError(f"{path}: unexpected bool branch")
    if isinstance(rust, int):
        if is_bool(native) or not isinstance(native, int) or native != rust:
            raise CompareError(f"{path}: integer native={native!r} Rust={rust!r}; integers are exact")
        return
    if isinstance(rust, float):
        if is_bool(native) or not isinstance(native, (int, float)):
            raise CompareError(f"{path}: f64 field native={native!r} Rust={rust!r} is not numeric")
        native_value = float(native)
        if not math.isfinite(native_value):
            raise CompareError(f"{path}: native f64 {native!r} is not finite")
        delta = abs(native_value - rust)
        stats["f64_fields"] += 1
        previous = stats["max_delta"].get(path.rsplit(".", 1)[-1], 0.0)
        if delta > previous:
            stats["max_delta"][path.rsplit(".", 1)[-1]] = delta
        if delta > TOLERANCE:
            raise CompareError(
                f"{path}: native={native_value!r} Rust={rust!r} abs delta {delta:e} "
                f"exceeds the gate tolerance {TOLERANCE:e}"
            )
        return
    if isinstance(rust, dict):
        if not isinstance(native, dict):
            raise CompareError(f"{path}: native is not an object")
        if list(native.keys()) != list(rust.keys()):
            raise CompareError(
                f"{path}: field set/order differs; native {list(native.keys())!r} "
                f"Rust {list(rust.keys())!r}"
            )
        for key in rust:
            compare_value(native[key], rust[key], f"{path}.{key}", stats)
        return
    if isinstance(rust, list):
        if not isinstance(native, list) or len(native) != len(rust):
            raise CompareError(f"{path}: array mismatch")
        for index, (n, r) in enumerate(zip(native, rust)):
            compare_value(n, r, f"{path}[{index}]", stats)
        return
    raise CompareError(f"{path}: unsupported Rust value {rust!r}")


def hold_drop_score_matches(event_score, snapshot, tolerance=TOLERANCE):
    """Driver formula: penalizing HoldNoteScriptEvent.score == -125 * snapshot / 1000."""
    expected = -125.0 * float(snapshot) / 1000.0
    return abs(float(event_score) - expected) <= tolerance


def compare_rows(native_rows, rust_rows, label: str, stats: dict):
    if len(native_rows) != len(rust_rows):
        raise CompareError(
            f"{label}: native row count {len(native_rows)} != Rust {len(rust_rows)}"
        )
    clamping = False
    stats.setdefault("clamp_excluded_fields", 0)
    stats.setdefault("clamp_excluded_rows", 0)
    stats.setdefault("clamp_first_sim", None)
    stats.setdefault("clamp_last_sim", None)
    for index, (native, rust) in enumerate(zip(native_rows, rust_rows)):
        where = f"{label}: row {index + 1} (sim {rust.get('sim')}, kind {rust.get('kind')})"
        check_schema(where, native)
        check_schema(where, rust)
        if native.get("sim") != rust.get("sim"):
            raise CompareError(
                f"{where}: sim native={native.get('sim')!r} Rust={rust.get('sim')!r}; "
                "sims/event order are exact"
            )
        if native.get("kind") != rust.get("kind"):
            raise CompareError(
                f"{where}: kind native={native.get('kind')!r} Rust={rust.get('kind')!r}; "
                "event order is exact"
            )
        if clamping:
            compare_excluding_health(native, rust, where, stats)
            continue
        try:
            compare_value(native, rust, where, stats)
        except CompareError as error:
            if not is_clamp_crossing(native, rust):
                raise error
            clamping = True
            stats["clamp_first_sim"] = rust.get("sim")
            compare_excluding_health(native, rust, where, stats)


def is_clamp_crossing(native, rust):
    """Native health is above HEALTH_MAX and Rust is in [HEALTH_MAX, native]."""
    if not isinstance(native, dict) or not isinstance(rust, dict):
        return False
    if list(native.keys()) != list(rust.keys()):
        return False
    saw = False
    for key, rust_value in rust.items():
        native_value = native.get(key)
        if key in ("health_before", "health_after"):
            if native_value == rust_value:
                continue
            if not isinstance(native_value, (int, float)) or isinstance(native_value, bool):
                return False
            if not isinstance(rust_value, (int, float)) or isinstance(rust_value, bool):
                return False
            native_number = float(native_value)
            rust_number = float(rust_value)
            if native_number <= HEALTH_MAX:
                return False
            if rust_number < HEALTH_MAX or rust_number > native_number + TOLERANCE:
                return False
            saw = True
            continue
        if native_value != rust_value and not (
            isinstance(native_value, (int, float))
            and isinstance(rust_value, (int, float))
            and not isinstance(native_value, bool)
            and not isinstance(rust_value, bool)
            and abs(float(native_value) - float(rust_value)) <= TOLERANCE
        ):
            return False
    return saw


def compare_excluding_health(native, rust, where, stats):
    if list(native.keys()) != list(rust.keys()):
        raise CompareError(f"{where}: field set/order differs after the clamp crossing")
    excluded = False
    for key, rust_value in rust.items():
        if key in ("health_before", "health_after"):
            excluded = True
            stats["clamp_excluded_fields"] += 1
            continue
        compare_value(native[key], rust_value, f"{where}.{key}", stats)
    if excluded:
        stats["clamp_excluded_rows"] += 1
        stats["clamp_last_sim"] = rust.get("sim")


def compare_dirs(native_dir: Path, rust_dir: Path, report_path: Path) -> int:
    if not native_dir.is_dir():
        sys.stderr.write(f"compare_note_rows: native directory missing: {native_dir}\n")
        return 2
    if not rust_dir.is_dir():
        sys.stderr.write(f"compare_note_rows: Rust directory missing: {rust_dir}\n")
        return 2
    native_files = {p.name: p for p in sorted(native_dir.glob("*.jsonl"))}
    rust_files = {p.name: p for p in sorted(rust_dir.glob("*.jsonl"))}
    if set(native_files) != set(rust_files):
        missing = sorted(set(rust_files) - set(native_files))
        extra = sorted(set(native_files) - set(rust_files))
        message = (
            f"coverage mismatch; missing native files {missing!r}, "
            f"extra native files {extra!r}"
        )
        sys.stderr.write(f"compare_note_rows: {message}\n")
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(
            json.dumps({"status": "fail", "problems": [message]}, indent=1) + "\n",
            encoding="utf-8",
        )
        return 2

    stats = {"f64_fields": 0, "max_delta": {}}
    report = {
        "schema": 1,
        "comparison": "note-native field-wise",
        "gate_tolerance": {
            "f64_absolute": TOLERANCE,
            "integers_bools_strings": "exact",
            "row_order_and_sims": "exact",
        },
        "files": [],
        "total_rows": 0,
        "failures": 0,
        "problems": [],
        "honesty": (
            "synthesized stream; not native-game parity; health_before/health_after "
            "are excluded only after a HEALTH_MAX clamp crossing (update() clamp is not retained)"
        ),
    }
    print(
        f"compare_note_rows: tolerance f64 abs <= {TOLERANCE:e}; "
        "integers/bools/strings exact; row order/sims exact"
    )
    ok = True
    for name in sorted(native_files):
        try:
            native_rows = load_rows(native_files[name])
            rust_rows = load_rows(rust_files[name])
            file_stats = {"f64_fields": 0, "max_delta": {}}
            compare_rows(native_rows, rust_rows, name, file_stats)
            stats["f64_fields"] += file_stats["f64_fields"]
            for key, value in file_stats["max_delta"].items():
                stats["max_delta"][key] = max(stats["max_delta"].get(key, 0.0), value)
            entry = {
                "file": name,
                "rows": len(native_rows),
                "status": "ok",
                "clamp_excluded_fields": file_stats.get("clamp_excluded_fields", 0),
                "clamp_excluded_rows": file_stats.get("clamp_excluded_rows", 0),
                "clamp_first_sim": file_stats.get("clamp_first_sim"),
                "clamp_last_sim": file_stats.get("clamp_last_sim"),
            }
            report["files"].append(entry)
            report["total_rows"] += len(native_rows)
            if entry["clamp_excluded_fields"]:
                print(
                    f"compare_note_rows: {name}: OK rows={len(native_rows)}; "
                    f"clamp exclusion: {entry['clamp_excluded_fields']} health fields on "
                    f"{entry['clamp_excluded_rows']} rows; first sim {entry['clamp_first_sim']}, "
                    f"last sim {entry['clamp_last_sim']}"
                )
            else:
                print(f"compare_note_rows: {name}: OK rows={len(native_rows)}")
        except CompareError as error:
            ok = False
            report["failures"] += 1
            report["files"].append(
                {
                    "file": name,
                    "status": "fail",
                    "problem": str(error),
                }
            )
            report["problems"].append(str(error))
            print(f"compare_note_rows: {name}: FAIL: {error}")

    report["max_abs_delta_by_field"] = {
        key: value for key, value in sorted(stats["max_delta"].items())
    }
    report["f64_fields_compared"] = stats["f64_fields"]
    report["status"] = "ok" if ok else "fail"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    if ok:
        print(
            f"compare_note_rows: PASS rows={report['total_rows']} files={len(native_files)} "
            f"f64_fields={stats['f64_fields']} report={report_path}"
        )
        return 0
    print(f"compare_note_rows: FAIL failures={report['failures']} report={report_path}")
    return 1


def _row(kind: str, sim: int, **extra) -> dict:
    base = {
        "schema": "1",
        "kind": kind,
        "sim": sim,
        "input_ms": float(sim if kind in ("hit", "ghost_press") else 0),
        "input_time_us": 0.0,
        "note_id": 0,
        "note_direction": 0,
        "judgement": "none",
        "raw_note_diff": 0.0,
        "note_diff_int": 0,
    }
    base.update(extra)
    return base


def _hit(sim=1000) -> dict:
    return _row(
        "hit",
        sim,
        judgement="sick",
        note_id=0,
        note_direction=0,
        raw_note_diff=0.0,
        note_diff_int=0,
        score_delta=500,
        health_before=1.0,
        health_after=1.03,
        combo_before=0,
        combo_after=1,
        combo_max=1,
        tallies={"sick": 1, "good": 0, "bad": 0, "shit": 0, "missed": 0, "total_notes_hit": 1},
    )


def _ghost(sim=2000) -> dict:
    return _row(
        "ghost_press",
        sim,
        note_id=0,
        note_direction=3,
        judgement="none",
        score_delta=-10.0,
        health_change=-0.08,
    )


def _hold_drop(sim=3000) -> dict:
    return _row(
        "hold_drop",
        sim,
        input_ms=sim - 1.0,
        note_id=1,
        note_direction=2,
        judgement="none",
        score_penalty=-50.0,
        health_change=0.0,
        combo_before=3,
        combo_after=0,
        is_combo_break=True,
    )


def _write_jsonl(path: Path, rows):
    path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")


def selftest() -> int:
    root = Path(tempfile.mkdtemp(prefix="note-compare-selftest-"))
    native = root / "native"
    rust = root / "rust"
    native.mkdir()
    rust.mkdir()
    formula_failures = []
    if not hold_drop_score_matches(-50.0, 400):
        formula_failures.append("snapshot 400 must match -50")
    if not hold_drop_score_matches(-20.125, 161):
        formula_failures.append("snapshot 161 must match -20.125")
    if hold_drop_score_matches(-49.875, 400):
        formula_failures.append("stale-off-by-one score must not match snapshot 400")
    if hold_drop_score_matches(-20.0, 161):
        formula_failures.append("a wrong event score must not match the formula")
    if formula_failures:
        for failure in formula_failures:
            print(f"compare_note_rows selftest: FAIL: {failure}")
        return 1

    failures = []

    def expect_ok(name):
        code = compare_dirs(native, rust, root / f"{name}-report.json")
        if code != 0:
            failures.append(f"{name}: expected pass, got exit {code}")

    def expect_fail(name, needle):
        code = compare_dirs(native, rust, root / f"{name}-report.json")
        report = json.loads((root / f"{name}-report.json").read_text())
        text = json.dumps(report)
        if code == 0:
            failures.append(f"{name}: expected failure, got exit 0")
        elif needle not in text:
            failures.append(f"{name}: expected {needle!r} in the report, got {text[:400]!r}")

    rows = [_hit(), _ghost(), _hold_drop()]
    _write_jsonl(native / "easy.jsonl", rows)
    _write_jsonl(rust / "easy.jsonl", rows)
    expect_ok("identical")

    bad = json.loads(json.dumps(rows))
    bad[0]["combo_after"] = 2
    _write_jsonl(native / "easy.jsonl", bad)
    expect_fail("integer-divergence", "integers are exact")

    bad = json.loads(json.dumps(rows))
    bad[0]["health_after"] = 1.03 + 2e-9
    _write_jsonl(native / "easy.jsonl", bad)
    expect_fail("float-divergence", "exceeds the gate tolerance")

    bad = json.loads(json.dumps(rows))
    bad[0]["health_after"] = 1.03 + 9e-10
    _write_jsonl(native / "easy.jsonl", bad)
    expect_ok("float-within-tolerance")

    bad = json.loads(json.dumps(rows))
    bad[1]["kind"] = "hold_tick"
    _write_jsonl(native / "easy.jsonl", bad)
    expect_fail("schema-drift", "field order/set differs")

    _write_jsonl(native / "easy.jsonl", rows)
    (rust / "easy.jsonl").unlink()
    expect_fail("missing-file", "coverage mismatch")
    _write_jsonl(rust / "easy.jsonl", rows)
    _write_jsonl(native / "normal.jsonl", rows)
    expect_fail("extra-file", "coverage mismatch")
    (native / "normal.jsonl").unlink()

    _write_jsonl(native / "easy.jsonl", rows[:-1])
    expect_fail("short-rows", "row count")
    _write_jsonl(native / "easy.jsonl", rows)

    if failures:
        for failure in failures:
            print(f"compare_note_rows selftest: FAIL: {failure}")
        print(f"compare_note_rows selftest: FAILED ({len(failures)} cases)")
        return 1
    print("compare_note_rows selftest: PASS (identical, int/float divergence, tolerance edge, schema drift, missing/extra file, short rows)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-dir")
    parser.add_argument("--rust-lines-dir")
    parser.add_argument("--report")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    if not args.native_dir or not args.rust_lines_dir or not args.report:
        parser.error("--native-dir, --rust-lines-dir and --report are required (or use --selftest)")
    return compare_dirs(Path(args.native_dir), Path(args.rust_lines_dir), Path(args.report))


if __name__ == "__main__":
    sys.exit(main())
