#!/usr/bin/env python3
"""Generate the small note-stream-1 toy fixtures used to develop/exercise the
native note probe before the real worker export exists.

Layout under target/note-probe-toy/:
  assets/             toy bopeebo chart+metadata for the positive stream
  assets-multi/       toy chart with two same-tick hits (multi-event negative)
  streams/            the positive easy.stream.jsonl
  negative/<case>/    damaged streams (same assets unless noted)
  negative/multi-event/assets-multi/  the multi-event assets

Every stream is emitted in the frozen NOTE-BEHAVIOR.md section 4 format:
header line + one tick per millisecond, music_ms == sim, events on their own
tick with input_ms == sim.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOY = ROOT / "target" / "note-probe-toy"

POSITIVE_NOTES = [
    {"t": 1000, "d": 0},
    {"t": 2000, "d": 1},
    {"t": 3000, "d": 2, "l": 400},
    {"t": 4000, "d": 0, "l": 320},
    {"t": 5000, "d": 1, "l": 320},
    {"t": 6000, "d": 3, "l": 200},
    {"t": 7000, "d": 2, "l": 100},
]
MULTI_NOTES = [
    {"t": 1000, "d": 0},
    {"t": 1000, "d": 1},
]

METADATA = {
    "version": "2.2.4",
    "songName": "Bopeebo",
    "playData": {"difficulties": ["easy", "normal", "hard"]},
    "timeChanges": [{"t": 0, "b": 0, "bpm": 100, "n": 4, "d": 4}],
}

# (sim, [(kind, direction), ...]) for the positive stream.
POSITIVE_EVENTS = {
    1000: [("press", 0)],
    1001: [("release", 0)],
    2161: [("press", 1)],
    2162: [("release", 1)],
    2700: [("press", 3)],
    2701: [("release", 3)],
    3000: [("press", 2)],
    3001: [("release", 2)],
    4000: [("press", 0)],
    # end-160: snapshot at the penalty tick is 161, so this penalizes.
    4160: [("release", 0)],
    5000: [("press", 1)],
    # end-159: snapshot at the penalty tick is exactly 160.0 (zero penalty).
    5161: [("release", 1)],
    6000: [("press", 3)],
    # Exactly end: zero-penalty drop at end+1, stale snapshot stays 1 ms.
    6200: [("release", 3)],
    7000: [("press", 2)],
    # end+1: natural decay already completed; hold_end at end+2, no drop row.
    7101: [("release", 2)],
}
POSITIVE_END = 7300


def write_assets(directory: Path, notes):
    songs = directory / "preload" / "data" / "songs" / "bopeebo"
    songs.mkdir(parents=True, exist_ok=True)
    chart = {
        "version": "2.0.0",
        "scrollSpeed": {"easy": 1.2, "normal": 1.3, "hard": 1.6},
        "events": [],
        "notes": {"easy": notes, "normal": notes, "hard": notes},
    }
    (songs / "bopeebo-chart.json").write_text(json.dumps(chart, separators=(",", ":")) + "\n", encoding="utf-8")
    (songs / "bopeebo-metadata.json").write_text(json.dumps(METADATA, separators=(",", ":")) + "\n", encoding="utf-8")


def stream_bytes(end: int, events_by_sim, difficulty="easy") -> str:
    header = {
        "schema": "note-stream-1",
        "difficulty": difficulty,
        "chart_id": "bopeebo-default",
        "tick_count": end + 1,
        "songpos_source": "toy fixture: slice step position",
    }
    lines = [json.dumps(header, separators=(",", ":"))]
    for sim in range(end + 1):
        events = [
            {"kind": kind, "direction": direction, "input_ms": sim}
            for kind, direction in events_by_sim.get(sim, [])
        ]
        lines.append(json.dumps({"sim": sim, "music_ms": float(sim), "events": events}, separators=(",", ":")))
    return "\n".join(lines) + "\n"


def write_stream(path: Path, text: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def main() -> int:
    write_assets(TOY / "assets", POSITIVE_NOTES)
    write_assets(TOY / "assets-multi", MULTI_NOTES)

    positive = stream_bytes(POSITIVE_END, POSITIVE_EVENTS)
    write_stream(TOY / "streams" / "easy.stream.jsonl", positive)

    negative = TOY / "negative"
    # 1. header schema drift
    write_stream(
        negative / "schema-drift" / "easy.stream.jsonl",
        positive.replace('"schema":"note-stream-1"', '"schema":"note-stream-2"', 1),
    )
    # 2. event input_ms below its tick sim
    below = positive.replace(
        '"events":[{"kind":"press","direction":0,"input_ms":1000}]',
        '"events":[{"kind":"press","direction":0,"input_ms":999}]',
        1,
    )
    assert below != positive, "positive stream press anchor missing"
    write_stream(negative / "event-below-sim" / "easy.stream.jsonl", below)
    # 3. short stream: drop the last tick line, header still declares the total
    short_lines = positive.splitlines(keepends=True)
    write_stream(negative / "short-ticks" / "easy.stream.jsonl", "".join(short_lines[:-1]))
    # 4. missing stream: drop a middle tick (sim sequence gap)
    missing_lines = positive.splitlines(keepends=True)
    del missing_lines[1 + 1000]
    write_stream(negative / "missing-ticks" / "easy.stream.jsonl", "".join(missing_lines))
    # 5. extra stream: append one more tick beyond the declared count
    extra = positive + json.dumps(
        {"sim": POSITIVE_END + 1, "music_ms": float(POSITIVE_END + 1), "events": []},
        separators=(",", ":"),
    ) + "\n"
    write_stream(negative / "extra-ticks" / "easy.stream.jsonl", extra)
    # 6. multi-event tick: two real hits dispatched on the same tick
    multi_events = {1000: [("press", 0), ("press", 1)], 1001: [("release", 0), ("release", 1)]}
    write_stream(negative / "multi-event" / "easy.stream.jsonl", stream_bytes(1200, multi_events))

    print(f"note_probe_toy: wrote {TOY}")
    for path in sorted(TOY.rglob("*.stream.jsonl")):
        print(f"note_probe_toy: stream {path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
