#!/usr/bin/env python3
"""Compare native Conductor timeline rows against the frozen model fixtures.

Native (real funkin.Conductor, hxcpp) rows and model rows share the frozen
fixture schema (M4-CONTRACT.md §1). The comparison is field-wise:
counts/index/sim/dispatch order EXACT; lengths/BPM at 1e-6 relative; offsets
exact; `input`/`songPosition` at <= 1 ms; and `stepTime`/`beatTime`/
`measureTime` — which are STEP/BEAT/MEASURE counts, not milliseconds —
converted to ms through each row's own step/beat/measure length BEFORE the
1 ms budget (the same mechanics the Rust gate `fixtures-conductor` applies).
Per-field max deltas are tracked and reported so a passing run proves what
was actually measured; a failing run prints the per-field deltas too.
"""
import argparse
import json
import math
import sys
from pathlib import Path

TOL_MS = 1.0
TOL_REL = 1e-6

# Integer-valued or boolean fields compared exactly (no tolerance).
EXACT_NUMBERS = {
    "step", "beat", "measure", "tc", "tsNum", "tsDen",
    "avOff", "gOff", "iOff", "fOff", "combined",
    "clampActive",
}
# Relative-tolerance lengths/BPM fields.
RELATIVE_FIELDS = {"beatLen", "stepLen", "measureLen", "bpm"}
# Step-count fields that require the row-length ms conversion before the
# 1 ms budget.
STEP_FIELDS = {"stepTime": "stepLen", "beatTime": "beatLen", "measureTime": "measureLen"}


def near_rel(before, after, tol=TOL_REL):
    if before == after:
        return True
    if after == 0:
        return math.isclose(before, after, abs_tol=tol)
    return abs(before / after - 1.0) <= tol or math.isclose(before, after, abs_tol=tol)


def field_delta(native_value, model_value, row, field, length_field_value=None):
    """Return the ms delta for a row/field pair under the frozen semantics."""
    if field in ("input", "songPosition"):
        return abs(float(native_value) - float(model_value))
    if field in STEP_FIELDS:
        return abs(float(native_value) * float(length_field_value)
                   - float(model_value) * float(length_field_value))
    if field in RELATIVE_FIELDS:
        before, after = float(native_value), float(model_value)
        if math.isclose(before, after, abs_tol=TOL_REL) or near_rel(before, after):
            return 0.0
        return float("inf")
    if field in EXACT_NUMBERS:
        return 0.0 if native_value == model_value else float("inf")
    return 0.0 if native_value == model_value else float("inf")


def compare(name, native_rows, model_rows):
    problems = []
    deltas = {
        "input": 0.0, "songPosition": 0.0,
        "stepTime": 0.0, "beatTime": 0.0, "measureTime": 0.0,
    }
    if len(native_rows) != len(model_rows):
        problems.append(f"row count {len(native_rows)} != model {len(model_rows)}")
        return problems, min(len(native_rows), len(model_rows)), deltas

    for index, (n, m) in enumerate(zip(native_rows, model_rows)):
        if n.get("t") != m.get("t"):
            problems.append(f"row {index}: kind native={n.get('t')} model={m.get('t')}")
            continue
        for field in ("i", "sim", "step", "beat", "measure", "tc", "tsNum", "tsDen"):
            if n.get(field) != m.get(field):
                problems.append(
                    f"row {index}: {field} native={n.get(field)!r} model={m.get(field)!r}"
                )
        for field in ("clampActive", "avOff", "gOff", "iOff", "fOff", "combined"):
            if n.get(field) != m.get(field):
                problems.append(
                    f"row {index}: {field} native={n.get(field)!r} model={m.get(field)!r}"
                )
        for field in RELATIVE_FIELDS:
            nv, mv = n.get(field), m.get(field)
            if nv is None or mv is None:
                problems.append(f"row {index}: missing {field}")
                continue
            if not near_rel(float(nv), float(mv)):
                problems.append(
                    f"row {index}: {field} native={nv} model={mv} outside 1e-6 relative"
                )
        for field, length_key in STEP_FIELDS.items():
            length = float(m.get(length_key, 0.0) or 0.0)
            if field not in n or field not in m:
                problems.append(f"row {index}: missing {field}")
                continue
            delta_ms = abs(float(n[field]) * length - float(m[field]) * length)
            if delta_ms > TOL_MS:
                problems.append(
                    f"row {index}: {field} (ms via {length_key}) native={n[field]} "
                    f"model={m[field]} delta {delta_ms:.6g} ms exceeds {TOL_MS}"
                )
            deltas[field] = max(deltas[field], delta_ms)
        for field in ("input", "songPosition"):
            if field not in n or field not in m:
                problems.append(f"row {index}: missing {field}")
                continue
            delta = abs(float(n[field]) - float(m[field]))
            if delta > TOL_MS:
                problems.append(
                    f"row {index}: {field} native={n[field]} model={m[field]} "
                    f"delta {delta:.6g} ms exceeds {TOL_MS}"
                )
            deltas[field] = max(deltas[field], delta)
    return problems, len(native_rows), deltas


def report_context(deltas_by_chart):
    worst = {}
    for name, deltas in deltas_by_chart:
        for field, value in deltas.items():
            worst[field] = max(worst.get(field, 0.0), value)
    return worst


def load_rows(path):
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="native output directory of JSONL files")
    parser.add_argument("--model", required=True, help="frozen tests/fixtures/conductor directory")
    parser.add_argument("--report", required=True, help="JSON comparison report path")
    args = parser.parse_args()

    native_dir = Path(args.input)
    model_dir = Path(args.model)
    native_ids = sorted(p.stem for p in native_dir.glob("*.jsonl"))
    model_ids = sorted(p.stem for p in model_dir.glob("*.jsonl"))
    if native_ids != model_ids:
        missing = [i for i in model_ids if i not in native_ids]
        extra = [i for i in native_ids if i not in model_ids]
        sys.stderr.write(f"coverage mismatch: missing {missing[:5]} extra {extra[:5]}\n")
        return 2

    report = {"charts": [], "total_rows": 0, "failures": 0}
    ok = True
    deltas_by_chart = []
    for chart_id in native_ids:
        native_rows = load_rows(native_dir / f"{chart_id}.jsonl")
        model_rows = load_rows(model_dir / f"{chart_id}.jsonl")
        problems, rows, deltas = compare(chart_id, native_rows, model_rows)
        deltas_by_chart.append((chart_id, deltas))
        status = "ok" if not problems else "fail"
        if problems:
            ok = False
        report["charts"].append({
            "id": chart_id,
            "rows": rows,
            "status": status,
            "problems": problems[:10],
            "problem_count": len(problems),
            "max_deltas_ms": {k: round(v, 9) for k, v in deltas.items()},
        })
        report["total_rows"] += rows
        report["failures"] += 0 if not problems else 1

    worst = report_context(deltas_by_chart)
    report["max_deltas_ms"] = {k: round(v, 9) for k, v in worst.items()}
    Path(args.report).write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    if ok:
        fields = ", ".join(f"{k}={v:.3f}" for k, v in sorted(worst.items()))
        print(f"compare_native_rows: OK rows={report['total_rows']} charts={len(native_ids)} max-deltas-ms [{fields}]")
        return 0
    print(f"compare_native_rows: FAIL charts_ok={len(native_ids) - report['failures']} failures={report['failures']}")
    for chart in report["charts"]:
        if chart["status"] != "ok":
            for problem in chart["problems"]:
                print(problem)
    return 1


if __name__ == "__main__":
    sys.exit(main())
