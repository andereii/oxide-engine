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

## ADR-0007 — Bitmap fonts come in two formats; Sparrow is the primary one

- **Context / date:** 2026-11-04, M2 contract. The plan assumed the three bitmap fonts
  (`preload/images/fonts/{bold,default,freeplay-clear}.xml`) carry BMFont-style
  `xoffset/yoffset/xadvance` and `<info face size>`; inspection shows they are Sparrow
  `TextureAtlas` files whose glyph frame names are `<char><4 digits>`
  (`A0000`, `#0000`, `&0000`, `-dash-0000`). The only angel-code font in the assets is
  `fonts/vcr-bmp.fnt` (`<info face size>`, `<common lineHeight>`, `<pages>/<page id file>`,
  `<chars>/<char id x y width height xoffset yoffset xadvance page>`), loaded via
  `FlxBitmapFont.fromAngelCode` in `PlayState.hx:812`.
- **Decision:** `oxide-font` supports both, as separate types: `SparrowFont` (metrics derived
  exactly like `source/funkin/ui/AtlasText.hx`: space=40, newline→`maxHeight` row advance,
  glyph advance = first frame width, `caseAllowed` via `^[A-Z]\d+$`/`^[a-z]\d+$`) and
  `AngelFont` (full angel-code attribute set incl. optional `<kernings>`; none exists in
  shipped data). Neither format unifies into one struct — no invented normalization.
- **Consequences:** gate assertions split per format; AtlasText's quirky glyph-prefix table
  (e.g. `&` → `-andpersand-`) is replicated verbatim since gameplay text depends on it.
- **Revisit when:** a third font format appears in modded data (mount through VFS by
  extension, not by guesswork).

## ADR-0008 — VFS mod-layer precedence follows Polymod: LAST mod wins

- **Context / date:** 2026-11-04, M2 contract. Layer order for `oxide-vfs` was ambiguous
  ("first mod wins" vs "last mod wins").
- **Decision:** the shipped polymod git dependency states in
  `backends/PolymodAssetLibrary.hx`: *"order matters -- mod files will load from first to
  last, with last taking precedence"*. `oxide-vfs` resolves last-added-mod-first, then the
  base assets directory. Mod id: the FUTURE game loader derives it from the mod root
  directory name (zips: file stem, ADR-0008; the `oxide-vfs` API takes ids as caller-
  supplied parameters, frozen contract §3). Duplicate ids are an error.
- **Consequences:** overrides in examples behave identically in Rust. A first-wins
  interpretation would silently break every multi-mod setup with conflicting files.
- **Revisit when:** polymod changes its documented precedence upstream.

## ADR-0010 — M3 media pipeline runs ffmpeg as an external binary; sha256 via `sha2`

- **Context / date:** 2026-11-05, M3 contract. Audio/image/video conversion
  (plan §7.4) needs real encoders; the promise "no DLLs on Windows" forbids linking.
- **Decision:** ffmpeg + ffprobe are invoked as EXTERNAL COMMANDS from `oxide pack` /
  `xtask media-check` only (build-time tooling). They are NOT crate dependencies
  (`oxide-media` never links against libav*), and game/audio code paths are statically
  forbidden from spawning them. `--check-tools` versions-guards them. For the pak
  digest: `sha2` 0.10 (Apache/MIT, pure Rust, seconds of build cost; alternative:
  hand-rolled sha256 — rejected, pointless risk). Deterministic pak assembly uses the
  existing `zip` 8.x (ADR-0009 features) in write mode.
- **Consequences:** media gates fail with a clear `Tools` error when ffmpeg is absent
  (tested SKIPPED in that environment, never silently green).
- **Revisit when:** a pure-Rust AV1/WebP encoder chain reaches release-tooling quality
  (plan §7.3's rav1d is decode-only; encode stays ffmpeg).

## ADR-0011 — assets-dist layout, total_after accounting, image never-grow fallback

- **Context / date:** 2026-11-05, M3 contract. Plan §7.5 sizes the release at ~340 MB
  with per-media-step numbers; the human gate demands `files_grew: 0` and
  `total_after <= 350 MB`.
- **Decision:**
  1. `assets-dist/` carries ONLY media (+fonts); text lives exclusively in
     `assets.pak` (deflate). The pak additionally STOREs the media
     (seekable), so in release the pak can replace the whole dist tree (M9).
  2. `total_after` counts media once (dist bytes minus the pak itself) plus the
     compressed text (pak bytes minus stored media bytes) — i.e. every content byte
     counted exactly once, matching §7.5's arithmetic.
  3. Images follow the same never-grow rule as audio: if webp-lossless output
     >= source bytes, the original PNG is kept (converted numbers still reported
     separately). The gate counts a kept png as `copied`, and a resumed conversion
     can never violate `files_grew: 0` by construction.
  4. The three attract trailers (mobileRelease, riftCollabTrailer,
     boyfriendEverywhere) are excluded from dist AND pak via an explicit stem list
     (ADR line cross-referenced from the contract §1).
- **Consequences:** this yields two artifact forms (loose tree + pak) with intentional
  content duplication on disk until M9; the SIZE GATE counts bytes once.
- **Revisit when:** the release layout is finalized (M9) or the 350–380 MB ADR clause
  actually triggers.

## ADR-0009 — New dependencies for M2: notify 8.2, zip 8.2 (feature-trimmed)

- **Context / date:** 2026-11-04, M2. `oxide-vfs` needs hot reload + zip mod support; the
  plan mandates `notify`. No new native dependencies allowed this milestone.
- **Decision:**
  - `notify` version=8.2 (licence CC0-1.0, pure Rust using inotify on Linux); used only for
    "path changed → invalidate" events. Build cost: seconds (small dep tree:
    `event-listener`, `crossbeam-channel`, `*, inotify`).
  - `zip` = 8.2 (MIT) with `default-features = false, features =
    ["deflate-flate2", "deflate-flate2-zlib-rs"]` so the read path is pure Rust — the
    default feature set would pull zstd-sys/lzma/bzip2 (C build) which contradicts the
    no-native-deps promise. **Amendment (2026-11-05, reviewer-corrected):** as first
    written the feature list (`deflate-flate2` alone) was unsatisfiable — zip 8.2 declares
    `flate2` with `default-features = false`, so an explicit zlib backend is required for
    the crate to compile. The chosen backend `zlib-rs` (Zlib licence, pure Rust) matches
    the flate2 `zlib-rs` feature; no `cc`/`cmake`/`*-sys` C crates enter the lockfile
    (verified by the M2 reviewer against `Cargo.lock`).
  - `oxide-atlas` adds **no** dependencies: the lenient XML scanner is hand-rolled because
    shipped Sparrow data is not std-well-formed (unescaped `<` / `&` inside attribute
    values); `xml` crates don't offer a "strict-ish but tolerant attrs" mode without
    editing semantics.
  - `serialize_json` deps in atlas: none — reuse `serde_json`/`thiserror` (already ADR-0001).
- **Consequences:** Cargo.lock gains these crates; no `build.rs` codegen; also headless.
- **Revisit when:** mod zips in the wild ship lzma/bzip2 compression (then an extra pure-
  Rust decode feature is reconsidered with a fresh ADR).

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
