#!/usr/bin/env python3
"""Review model candidates without writing either input directory (ADR-0014)."""
import argparse
import collections
import hashlib
import json
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def review(before, after):
    names = sorted(p.name for p in before.glob("*.jsonl"))
    if names != sorted(p.name for p in after.glob("*.jsonl")) or len(names) != 62:
        raise ValueError("expected matching 61-chart + smoke inventories")
    report = {"authority": "ADR-0014; native utility parity, model-only full timelines",
              "files": [], "changed_files": 0, "old_rows": 0, "new_rows": 0,
              "changed_fields_by_matching_event": collections.Counter(),
              "max_event_input_shift_ms": 0, "global_event_order_changed": []}
    for name in names:
        old = [json.loads(line) for line in (before / name).read_text().splitlines()]
        new = [json.loads(line) for line in (after / name).read_text().splitlines()]
        if not old or not new:
            raise ValueError(f"empty timeline: {name}")
        report["old_rows"] += len(old)
        report["new_rows"] += len(new)
        for kind in ("step", "beat", "measure"):
            a = [r["i"] for r in old if r["t"] == kind]
            b = [r["i"] for r in new if r["t"] == kind]
            if a != b:
                raise ValueError(f"per-kind count/index sequence changed: {name} {kind}")
        if [(r["t"], r["i"]) for r in old] != [(r["t"], r["i"]) for r in new]:
            report["global_event_order_changed"].append(name)
        changes = collections.Counter()
        shift = 0
        for kind in ("step", "beat", "measure"):
            for a, b in zip([r for r in old if r["t"] == kind], [r for r in new if r["t"] == kind]):
                for key in a:
                    if a[key] != b[key]:
                        changes[key] += 1
                shift = max(shift, abs(a["input"] - b["input"]))
        if shift > 1:
            raise ValueError(f"event input shift exceeds 1 ms: {name}: {shift}")
        changed = digest(before / name) != digest(after / name)
        report["changed_files"] += int(changed)
        report["max_event_input_shift_ms"] = max(report["max_event_input_shift_ms"], shift)
        report["changed_fields_by_matching_event"].update(changes)
        report["files"].append({"name": name, "before_sha256": digest(before / name),
                                "after_sha256": digest(after / name), "changed": changed,
                                "rows": len(new), "max_event_input_shift_ms": shift,
                                "changed_fields_by_matching_event": dict(changes)})
    report["changed_fields_by_matching_event"] = dict(report["changed_fields_by_matching_event"])
    report["per_kind_counts_and_index_sequences"] = "unchanged in all 62 files"
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    result = review(args.before, args.after)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "files"}, indent=2, sort_keys=True))
