#!/usr/bin/env python3
"""Record read-only input/runtime hashes for an authorized candidate generation."""
import argparse
import hashlib
import json
import platform
import shutil
import subprocess
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def set_sha(entries):
    # Same C-locale sorted sha256sum output used by snapshot_validate.sh.
    lines = sorted(f"{digest}  {name}\n" for name, digest in entries.items())
    return hashlib.sha256("".join(lines).encode()).hexdigest()


def tool(argv):
    result = subprocess.run(argv, capture_output=True, text=True, timeout=10, check=True)
    binary = Path(shutil.which(argv[0])).resolve()
    return {"version": (result.stdout + result.stderr).strip().splitlines()[0],
            "binary": str(binary), "sha256": sha(binary)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game", required=True, type=Path)
    parser.add_argument("--candidates", required=True, type=Path)
    parser.add_argument("--utility-build", required=True, type=Path)
    parser.add_argument("--native-build", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    oracle = root / "tools/oracle"
    review = json.loads((oracle / "candidate-review.json").read_text())
    manifest = json.loads((args.candidates / "_manifest.json").read_text())
    reference_files = [
        "source/funkin/Conductor.hx", "source/funkin/data/song/SongData.hx",
        "source/funkin/data/song/SongDataUtils.hx", "source/funkin/util/Constants.hx",
        "source/funkin/play/PlayState.hx", "source/funkin/Preferences.hx",
        ".haxelib/flixel/git/flixel/math/FlxMath.hx",
        ".haxelib/hxcpp/git/include/hxMath.h", ".haxelib/hxcpp/git/src/Math.cpp",
        ".haxelib/hxcpp/git/hxcpp.n", ".haxelib/hxcpp/git/haxelib.json",
    ]
    data_root = args.game / "assets/preload/data/songs"
    if not data_root.is_dir():
        data_root = args.game / "assets/songs"
    assets = {}
    for entry in manifest["charts"]:
        song, variation = entry["id"].rsplit("-", 1)
        suffix = "" if variation == "default" else "-" + variation
        paths = [data_root / song / f"{song}-chart{suffix}.json",
                 data_root / song / f"{song}-metadata{suffix}.json",
                 args.game / "assets/songs" / song / "Inst.ogg"]
        for path in paths:
            key = str(path.relative_to(args.game))
            if key not in assets:
                assets[key] = sha(path)
    timeline_hashes = {p.name: sha(p) for p in args.candidates.glob("*.jsonl")}
    if len(timeline_hashes) != 62 or len(manifest["charts"]) != 61:
        raise ValueError("provenance requires all 61 charts plus smoke")
    for entry in review["files"]:
        if timeline_hashes[entry["name"]] != entry["after_sha256"]:
            raise ValueError(f"candidate changed after review: {entry['name']}")
    manifest_hash = sha(args.candidates / "_manifest.json")
    sources = sorted(oracle.glob("src/*.hx")) + sorted(oracle.glob("probe-types/**/*.hx"))
    sources += [oracle / name for name in (
        "build.hxml", "probe_rounding.sh", "snapshot_validate.sh", "review_candidates.py",
        "record_provenance.py", "BEHAVIOR.md", "corpus.txt")]
    native_outputs = {
        "real_utility_generated_cpp": args.utility_build / "src/flixel/math/FlxMath.cpp",
        "real_utility_generated_probe_cpp": args.utility_build / "src/RealRoundProbe.cpp",
        "real_utility_binary": args.utility_build / "RealRoundProbe",
        "runtime_harness_generated_cpp": args.native_build / "src/NativeRoundProbe.cpp",
        "runtime_harness_binary": args.native_build / "NativeRoundProbe",
    }
    provenance = {
        "authority": "ADR-0014; human-approved correction, 2026-11-05",
        "evidence_boundary": "real native FlxMath.roundDecimal on 10 finite cases; full conductor timelines remain model-only",
        "not_claimed": ["independent full-game conductor oracle", "exhaustive floating-point parity", "M4 input/replay/jitter completion", "legal clean-room independence"],
        "platform": platform.platform(),
        "tools": {name: tool(argv) for name, argv in {
            "haxe": ["haxe", "--version"], "neko": ["neko", "-version"],
            "ffprobe": ["ffprobe", "-version"], "g++": ["g++", "-dumpfullversion"],
        }.items()},
        "hxcpp_declared_version": json.loads((args.game / ".haxelib/hxcpp/git/haxelib.json").read_text())["version"],
        "native_compilation": {
            "haxe": "-cpp <repo target output> -D no-compilation -dce full -D analyzer-optimize; original FlxMath classpath; ProbeSlice + type-only import shims",
            "hxcpp": "neko <installed hxcpp.n> Build.xml -DHXCPP=<installed hxcpp> -DHXCPP_M64 -Dlinux; HXCPP_COMPILE_THREADS=2",
            "original_tree": "read-only; all outputs/configuration under repository target/ or /tmp",
        },
        "reference_source_sha256": {name: sha(args.game / name) for name in reference_files},
        "input_asset_sha256": dict(sorted(assets.items())),
        "model_and_probe_source_sha256": {str(p.relative_to(root)): sha(p) for p in sources},
        "native_output_sha256": {name: sha(path) for name, path in native_outputs.items()},
        "probe_stdout_sha256": sha(oracle / "rounding-probe.txt"),
        "candidate_review_sha256": sha(oracle / "candidate-review.json"),
        "simulation": manifest["simulation"],
        "chart_count": 61, "smoke_count": 1, "manifest_sha256": manifest_hash,
        "timeline_sha256": dict(sorted(timeline_hashes.items())),
        "old_timeline_set_sha256": set_sha({entry["name"]: entry["before_sha256"] for entry in review["files"]}),
        "corrected_timeline_set_sha256": set_sha(timeline_hashes),
        "corrected_set_with_manifest_sha256": set_sha({**timeline_hashes, "_manifest.json": manifest_hash}),
        "review_summary": {key: value for key, value in review.items() if key != "files"},
    }
    args.out.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n")
    print("provenance:", args.out)
    print("corrected set with manifest:", provenance["corrected_set_with_manifest_sha256"])
