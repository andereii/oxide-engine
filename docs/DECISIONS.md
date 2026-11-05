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

## ADR-0005 — Path-first classification with strict shape validation (supersedes 0004)

- **Context / date:** 2026-11-04, M1 evolution (human bug report). Shape-based
  `classify` could silently accept a malformed chart (a `"notes"` array) because
  nothing anchored a file to a kind. Measured guards: every real chart lives under
  `*/songs/`; all `*/data/characters/*.json` validate as characters.
- **Decision:** `oxide_data::parse_file(path, bytes)` classifies path-first:
  `songs/*-chart*` ⇢ Chart, `songs/*-metadata*` ⇢ Metadata, exact `data/<characters|
  stages|levels|notestyles|players>` parent pairs ⇢ Character/Stage/Level/NoteStyle/
  Player. Only files with no implied kind fall back to shape classification. An
  implied kind whose shape does not validate fails with `DataError::Schema` (gate:
  `tests/fixtures/broken` exits 1, wired as an xtask integration test). This
  *strengthens* the gate, no fixture or threshold weakened.
- **Consequences:** two-player files (`data/players/*`) become a new `Player` kind
  (schema grounded in Haxe `PlayerData.hx`; complex nested stamps stay verbatim
  `Value` same policy as ADR-0003). Real-assets table shifts: player 2, unknown 203,
  totals unchanged 432/432.
- **Revisit when:** a modder layout legitimately deviates from these dir names
  (mods define their own roots; resolution happens above this layer).

## ADR-0006 — Metadata without `timeChanges` (permissive skip)

- **Context / date:** 2026-11-04, M1 evolution (human bug report). Some metadata
  files may lack `timeChanges` entirely.
- **Options:** strict (require the key) vs permissive (default empty, omit on serialize).
- **Decision:** permissive for this one field, documented: absent key ⇢ empty
  `Vec` decode ⇢ key absent on serialize; a present-but-empty `[]` still serializes
  as `[]`. Rationale: charts always validate strongly through `timeChanges`-bearing
  metadata at gameplay time (M4 conductor gate), so early parse leniency here does
  not hide errors that matter.
- **Revisit when:** M4 finds real charts whose metadata lacks `timeChanges`
  (then strictness moves into the conductor preflight, not the parser).
