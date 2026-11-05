# Decision records (ADRs)

One entry per non-trivial decision: context, options, decision, consequences,
and what would make us revisit it. Gate/test/threshold changes MUST have an ADR.

---

## ADR-0001 — Workspace structure and deferred heavy dependencies

- **Context / date:** 2026-11-04, M0 skeleton. Plan §3 lists the final stack (Bevy
  =0.20, kira, symphonia, libopus...).
- **Options:** (A) declare all final dependencies in M0 so the skeleton mirrors
  the final stack; (B) stub crates with zero dependencies and add each
  dependency when its crate gets real work.
- **Decision:** B. Each `oxide-*` crate is an empty stub in M0; `oxide-data`
  starts with `serde` + `serde_json` + `thiserror`. Bevy `=0.20` is added with
  an ADR when `oxide-game` (or the CI gate) first needs it.
- **Consequences:** fast M0 cycles; every dependency addition stays visible and
  costed. Risk: none — dependencies are additive.
- **Revisit when:** a milestone gate requires the dependency earlier than planned.

- `serde` 1.x (Apache/MIT): JSON data formats are the core of M1. Build cost:
  seconds.
- `serde_json` 1.x (Apache/MIT): parse + round-trip of the 432 asset JSONs.
  Build cost: seconds; pure Rust.
- `thiserror` 2.x (Apache/MIT): per-crate error enums, convention in AGENTS.md.
  Build cost: seconds.

## ADR-0002 — Strip a leading UTF-8 BOM before JSON parsing

- **Context / date:** 2026-11-04, M1. 37 shipped asset JSON files (Animate Atlas
  `spritemap1.json` under `images/...`) carry a UTF-8 BOM; `serde_json` fails on it.
- **Options:** (A) treat those files as invalid data; (B) strip one leading BOM in
  `oxide_data::parse_any` and in the gate's reparse.
- **Decision:** B. The BOM is a property of the shipped data. `parse_all` re-parses raw
  bytes with the same strip, so both sides see identical input.
- **Consequences:** parse-all counts those 37 files under `unknown` and they round-trip.
  No fixture or gate weakened.
- **Revisit when:** a parser misbehaves because a *secondary* BOM appears mid-stream.

## ADR-0003 — Fields that only exist as JSON shapes stay verbatim (`Value`)

- **Context / date:** 2026-11-04, M1 contract v2. Real asset files disagree with the
  Haxe types in three places: metadata `offsets` is freeform (`{"vocals":{"bf":0,"dad":-8}}`),
  level `capsule` and stage `characters` have unstructured stamps.
- **Decision:** keep those three as `Option<serde_json::Value>` verbatim in M1 instead of
  inventing field shapes. Everything else is typed. Key presence is preservation-enforced:
  an original `[]` must serialize (`stage props`), an absent key must stay absent
  (`songVariations`).
- **Consequences:** lossless round-trip over all 432 files is guaranteed by the gate;
  typed refinement of these three shapes is deferred to the milestone that consumes them
  (M5 gameplay for `offsets`/`characters`, M7 freeplay for `capsule`).
- **Revisit when:** gameplay code needs typed access to one of them, with the Haxe oracle
  as the field-name source.

## ADR-0004 — parse-all kind taxonomy is shape-based, not directory-based

- **Context / date:** 2026-11-04, M1. The plan (§1) claims "43 characters"; a shape-based
  classifier sorts 55 files into the character kind (includes `isPixel` text-anims and
  per-directory variants) and 3 dialogue boxes into unknown after the `text` guard.
- **Decision:** parse-all classifies by JSON shape (frozen rules in `crates/oxide-data/src/kind.rs`),
  counts are reported per kind, and the gate condition is "zero failed". The 432 file
  total and 61/61/21 subtotals match the plan; per-kind counts are documented in PROGRESS.md.
- **Consequences:** the gate measures parseability, not an editorial character census.
- **Revisit when:** M2/M5 needs an exact character registry count (registry, not classifier).
