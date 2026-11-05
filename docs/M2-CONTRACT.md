# M2 contract — oxide-atlas, oxide-font, oxide-vfs

Frozen for milestone M2. Derived from the real shipped files (quoted below) and the
Haxe oracle `~/Descargas/fnf/Funkin/source`. Repository artifacts stay English (AGENTS.md).
All numeric game-unit values are copied as stored (pixels, no unit conversion).

## 0. Evidence (what the contract is based on)

- Sparrow XML root and child (real files e.g. `week6/images/portrait-bf.xml`,
  `preload/images/fonts/default.xml`):
  ```xml
  <TextureAtlas imagePath="tiny.png"><SubTexture name="X0000" x y width height
    frameX frameY frameWidth frameHeight rotated="true" /></TextureAtlas>
  ```
  - 266 `*.xml` files under `assets/` total; **205** have a `<TextureAtlas>` root,
    **61** are flixel-ui files (`<menubar>`, `<box>`, `<dialog>`…) that are NOT atlases.
  - Lenient parsing is mandatory: Haxe `haxe.xml.Parser` accepts what std-strict parsers reject
    in the shipped data — `name="<0000"` (`preload/images/alphabet.xml:87`),
    `name="&0000"` (`preload/images/fonts/default.xml:15`), attributes containing raw `<` /
    `&` elsewhere, and a XML declaration preceded by blank line
    (`preload/data/ui/chart-editor/dialogs/user-guide.xml`).
  - Measured over all 205 atlas XMLs: **8486 subtextures**, **5684** with non-zero `frame*`
    trimming, **4144** with negative `frameX`/`frameY`, **198** `rotated="true"`
    (also 9 `rotated="false"`), **20** atlas files whose images references a missing PNG asset.
- Packer = TexturePacker **JSON (hash)** and **JSON (array)**; frames entries carry exactly
  `frame{x,y,w,h}`, `rotated`, `trimmed`, `spriteSourceSize{x,y,w,h}`, `sourceSize{w,h}`;
  meta carries `image`. **No packer-format file exists in the shipped assets** — coverage is
  fixtures + unit tests only.
- Animate Atlas (flxanimate variant) `spritemap1.json` (88 dirs, always exactly
  `spritemap1.json` + `Animation.json` per dir):
  ```json
  {"ATLAS": {"SPRITES": [{"SPRITE": {"name": "0000", "x": 0, "y": 0, "w": 1401, "h": 100, "rotated": false}}]},
   "meta": {"app": "Adobe Animate (Better TA Extension)", "version": "...", "image": "spritemap1.png",
            "format": "RGBA8888", "size": {"w": 1011, "h": 577}, "resolution": "1"}}
  ```
  88 files, **6071 SPRITE entries** in total, every file has `meta.image`.
  `Animation.json` top keys: `AN`, `SD`, `MD`.
- Bitmap fonts come in **two** distinct formats (ADR-0007):
  1. Sparrow atlas fonts `preload/images/fonts/{bold,default,freeplay-clear}.xml` —
     glyph frame names are `<char><4 digits>` (e.g. `A0000`, `#0000`, `&0000`,
     `-dash-0000`), 1–4 frames per glyph. Counts: default=99 glyph prefixes /
     maxHeight 70; bold=63 / 77; freeplay-clear=10 (digits 0-9) / 24.
     Semantics = Haxe `source/funkin/ui/AtlasText.hx`: space width **40**, newline row
     advance = `maxHeight`, char advance = **first frame width of the glyph**, height per
     line = `maxHeight`, missing glyph consumes zero space.
  2. Angel-code XML font `fonts/vcr-bmp.fnt` (used by `FlxBitmapFont.fromAngelCode` in
     `PlayState.hx:812`): `<info face="vcr-bmp" size="16" ...>`, `<common lineHeight="18"
     base="14" scaleW="122" scaleH="122" pages="1" ...>`, `<pages><page id="0"
     file="vcr-bmp.png"/></pages>`, `<chars count="90"><char id="33" x="118" y="17"
     width="4" height="14" xoffset="2" yoffset="2" xadvance="10" page="0" chnl="15"/>…`.
     No `<kernings>` element in any shipped file — parse it **if present**.
- Polymod precedence oracle: `.haxelib/polymod/git/polymod/backends/PolymodAssetLibrary.hx`:
  *"order matters -- mod files will load from first to last, with last taking precedence"* —
  **the LAST enabled mod wins, then the base assets**.
- Mod manifest: Polymod `_polymod_meta.json` at mod root, real example keys
  (from `example_mods/introMod/_polymod_meta.json`): `title`, `description`, `contributors`,
  `api_version`, `mod_version`, `license`.

## 1. `crates/oxide-atlas`

```rust
// lib.rs (re-exported modules: sparrow, packer, animate)
pub struct SubTexture {
    pub name: String,
    pub x: i32, pub y: i32, pub width: i32, pub height: i32,   // page rect
    pub frame_x: i32, pub frame_y: i32,                        // 0 when attribute absent
    pub frame_width: i32, pub frame_height: i32,               // 0 when attribute absent
    pub rotated: bool,                                         // absent|false => false
}
pub struct Atlas { pub image_path: String, pub subtextures: Vec<SubTexture> }

#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("malformed atlas at {uri}: {message}")]
    Malformed { uri: String, message: String },
    #[error("not a sparrow atlas (root element <{root_tag}>): {uri}")]
    NotAnAtlas { uri: String, root_tag: String },
    #[error("bad atlas json at {uri}: {message}")]
    Json { uri: String, message: String },   // serves parse_packer + parse_animate
}

pub fn parse_sparrow(xml: &str) -> Result<Atlas, AtlasError>;          // bytes -> &str, BOM stripped
pub fn parse_packer(json: &str) -> Result<PackerAtlas, AtlasError>;   // hash + array variants
pub struct PackerFrame {
    pub name: String,
    pub frame: [i32; 4],        // x y w h
    pub rotated: bool,
    pub trimmed: bool,
    pub sprite_source_size: [i32; 4],
    pub source_size: [i32; 2],  // w h
}
pub struct PackerAtlas { pub image: String, pub frames: Vec<PackerFrame> }

// Structural parse only (NO playback, NO frames->sprites resolution):
pub struct AnimateAtlas {
    pub image: String,                      // meta.image
    pub sprites: Vec<AnimateSprite>,        // ATLAS.SPRITES[].SPRITE
}
pub struct AnimateSprite { pub name: String, pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub rotated: bool }
pub fn parse_animate(json: &str) -> Result<AnimateAtlas, AtlasError>;
// audited counts for gate A: 88 files ok, 6071 sprites, 0 missing meta.image.

// The lenient XML scanner lives here as a pub module so oxide-font reuses it for
// angel-code fonts (its only XML consumers are these two formats):
pub mod xml;
```

Lenient scanner requirements (stdlib only, no new XML dependency): accepts attributes quoted
with `"` or `'`, tolerates raw `<` / `&` inside attribute values, tolerates an XML declaration
preceded by whitespace/BOM with any content before the first `<`, self-closing or paired tags.
Numbers: `i32` grammar `[-+]?[0-9]+` — non-numeric value on a REQUIRED numeric attribute →
`Malformed`.
Missing `name` on `SubTexture` → `Malformed`. Missing geometry attributes → 0 (matches Haxe
leniency). A root element that is not `TextureAtlas` → `NotAnAtlas { root_tag }`.
No `deny_unknown_fields`-style strictness anywhere.

## 2. `crates/oxide-font`

Depends on `oxide-atlas` (uses `parse_sparrow` for Sparrow-format fonts and the scanner for
angel-code XML). No renderer.

```rust
#[derive(Debug, Error)]
pub enum FontError {
    #[error("font parse error in {uri}: {message}")]
    Malformed { uri: String, message: String },
    #[error("glyph {glyph:?} rect (x={x}, y={y}, w={w}, h={h}) overflows page {sw}x{sh}")]
    GlyphOverflowsPage { glyph: String, x: i32, y: i32, w: i32, h: i32, sw: i32, sh: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case { Both, Upper, Lower }

pub struct SparrowFont {
    pub atlas: oxide_atlas::Atlas,
    pub glyphs: std::collections::BTreeMap<String, SparrowGlyph>, // key = anim prefix (see below)
    pub case_allowed: Case,
    pub max_height: u32,
}
pub struct SparrowGlyph { pub width: u32, pub height: u32 }   // first frame of the prefix

impl SparrowFont {
    pub fn from_atlas(atlas: oxide_atlas::Atlas) -> Result<SparrowFont, FontError>;
    pub fn glyph_prefix(&self, ch: char) -> String;   // AtlasText.getAnimPrefix table verbatim
    pub fn measure(&self, text: &str) -> (u32, u32);
}

pub struct AngelFont {
    pub info: AngelInfo,                  // face: String, size: i32
    pub common: AngelCommon,              // line_height u32, base u32, scale_w u32, scale_h u32
    pub pages: Vec<AngelPage>,            // id u8, file String
    pub chars: std::collections::BTreeMap<u32, AngelChar>,
    pub kerning: std::collections::BTreeMap<(u32, u32), i32>,   // empty when <kernings> absent
}
pub struct AngelChar {
    pub id: u32, pub x: i32, pub y: i32, pub width: u32, pub height: u32,
    pub xoffset: i32, pub yoffset: i32, pub xadvance: i32, pub page: u8,
}
impl AngelFont {
    pub fn parse(xml: &str) -> Result<AngelFont, FontError>;
    pub fn validate(&self) -> Result<(), FontError>;   // every char rect inside (0..scale_w, 0..scale_h)
    pub fn measure(&self, text: &str) -> (u32, u32);   // width = sum(xadvance + kern(first,second)),
                                                       // height = lines * line_height; unknown char skipped
}
```

`SparrowFont::glyph_prefix` replicates Haxe `AtlasText.getAnimPrefix` **verbatim**, including
quirks (`&` → `-andpersand-`, `,` → `-comma-`, …, default = the char itself). `measure` per
oracle: space = fixed advance 40; `\n` = xReset + lineHeight `maxHeight`; glyph advance =
first-frame `width`; missing glyph = zero advance; height = `lines * max_height`.
No kerning in Sparrow fonts (format has none) — API has none; kerning lives on `AngelFont`.
`freeplay-clear` contains only digits: both regexes `^[A-Z]\d+$` / `^[a-z]\d+$` fail →
`Case::Both` (Haxe leaves caseAllowed Both when containsUpper == containsLower).

## 3. `crates/oxide-vfs`

New deps (ADR-0008/0009 as amended): `notify = "8.2"` (Linux inotify backend; pure Rust,
licence CC0-1.0), `zip = "8.2"` with `default-features = false, features =
["deflate-flate2", "deflate-flate2-zlib-rs"]` — zip 8.2+ declares `flate2` with
`default-features = false` so an explicit backend is mandatory to compile (ADR-0009
amendment); both `zlib-rs` (Zlib licence) and `miniz_oxide` are pure Rust, no C build.
Case-exact lookup on every platform via name index built from `read_dir` / zip entry
table — naive direct paths are NOT acceptable (this includes the manifest lookup and
invalidate: event casing is never trusted; the index is always re-derived from
`read_dir`). Mod roots are canonicalized on add (validates existence and stores the
on-disk casing). Mod ids are caller-supplied (the loader owns naming; ADR-0008
derivation rule applies to the future loader, not to this crate).

```rust
#[derive(Debug, Error)]
pub enum VfsError {
    #[error("manifest {path:?} is invalid: {detail}")]
    Manifest { path: PathBuf, detail: String },
    #[error("zip {path:?} has an unsafe entry {entry:?} (absolute or contains '..')")]
    UnsafeZipEntry { path: PathBuf, entry: String },
    #[error("duplicate mod id '{id}'")]
    DuplicateMod { id: String },
    #[error("mod {path:?} has no _polymod_meta.json at its root")]
    MissingManifest { path: PathBuf },
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("zip: {0}")] Zip(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResKind { Image, Sound, Music, Inst, Voices, Chart, Json, Frag }

#[derive(Debug, Clone)]
pub enum Source {
    Dir(PathBuf),                       // absolute, already-resolved concrete file
    Zip { path: PathBuf, entry: String },
}

pub struct Vfs { /* ... */ }
impl Vfs {
    pub fn new(base_dir: PathBuf) -> Self;
    pub fn add_mod_dir(&mut self, dir: PathBuf, id: String) -> Result<(), VfsError>;   // grounded mod root
    pub fn add_mod_zip(&mut self, zip_path: PathBuf, id: String) -> Result<(), VfsError>;
    pub fn mod_ids(&self) -> &[String];
    /// Case-exact resolution. Search: last-added mod first ... then base dir.
    pub fn resolve(&self, rel: &str) -> Option<Source>;
    /// Kind mapping (see table), each candidate tried with extension fallbacks, case-exact.
    pub fn resolve_kind(&self, kind: ResKind, key: &str) -> Option<Source>;
    pub fn read(&self, src: &Source, size_limit: u64) -> Result<Vec<u8>, VfsError>;
    /// Invalidate the cached directory/state for exactly this path (hot reload primitive).
    pub fn invalidate(&self, path: &Path);
    /// Background watcher: invalidate exactly changed paths. Deterministic
    /// invalidation is available via invalidate() so tests drive it directly.
    pub fn watch(self: std::sync::Arc<Self>) -> Result<VfsWatcher, VfsError>;
}
impl ResKind { pub fn candidates(&self, key: &str) -> Vec<String> }  // ordered candidate rel paths
pub struct VfsWatcher { handle: std::thread::JoinHandle<()> }
impl VfsWatcher { pub fn stop(self) {} }
```

Kind table (oracle `source/funkin/Paths.hx`; `Constants.EXT_SOUND = 'ogg'` non-web):

| kind  | pattern (before ext fallbacks)   | fallback extensions |
|-------|----------------------------------|---------------------|
| Image | `images/{key}`                   | `png`               |
| Sound | `sounds/{key}`                   | `ogg`, `mp3`        |
| Music | `music/{key}`                    | `ogg`, `mp3`        |
| Inst  | `songs/{song}/Inst` (key=song)   | `ogg`, `mp3`        |
| Voices| `songs/{song}/Voices` (key=song) | `ogg`, `mp3`        |
| Chart | `data/songs/{key}` (key=`{ song }/ { song }-chart[-variation]`) | `json` |
| Json  | `data/{key}`                     | `json`              |
| Frag  | `shaders/{key}`                  | `frag`              |

Song ids lowercase like Haxe `inst/voices/chart` do (`song.toLowerCase()`): for
Chart/Inst/Voices the segment before the first `/` (the song id) is lowercased, the
remainder of the key is kept verbatim (Haxe builds `$id-chart[-variation]` with the
variation passed as-is — no invented normalization). Manifest/zip reads are capped: the
manifest is fetched through the same bounded-read primitive as content (unbounded reads
are a mod-root hostility surface).

The 61 non-atlas XML disposition is part of the gate, not the VFS: see gate A format.

## 4. Gate commands

- **Gate A** `cargo xtask parse-atlas --dir ~/Descargas/fnf/Funkin/assets` (subcommand owned
  by the atlas task, exact printed block):
  ```
  xml visited: 266
  atlas ok: 205
  atlas failed: 0
  non-atlas xml (skipped): 61
  subtextures: 8486
  trimmed (non-zero frame*): 5684
  negative frameX/Y: 4144
  rotated true: 198
  missing png refs: 20
  packer ok 0 failed 0 (none in assets; fixtures cover parser)
  animate ok 88 failed 0 sprites 6071
  ```
  Exit non-zero on ANY atlas/animate failure. Broken-XML fixture
  `tests/fixtures/atlas/broken/numeric-garbage.xml` must make the tool exit 1 (xtask test).
- **Gate B** unit+integration tests in `oxide-font` (run under `cargo xtask test`):
  glyph counts for the three real Sparrow fonts are NOT in fixtures (real assets dir, do
  not duplicate); assert from `micro-font.xml` fixture: glyph prefixes `{A,B,-dash-}`,
  `A`=45x59, `B`=38x59, `maxHeight=59`, `caseAllowed=Upper` (names `A0000`/`B0000` match
  `^[A-Z]\d+$`, none matches `^[a-z]\d+$`, `-dash-0000` matches neither — oracle rule
  `containsUpper != containsLower`); `measure("AB")=(83,59)`,
  `measure("A B")=(123,59)` (space=40), `measure("A\nB")=(45,118)` (widest-line semantics:
  each `\n` starts a new line; reported width = max over lines, here line 1 "A"=45 > "B"=38),
  missing glyph zero-width; `measure("AA\nA")=(90,118)` discriminates widest-line from
  last-line accumulation (the last-line value would be 45).
  Real-font checks (guarded by assets-dir presence): default=99 glyph prefixes,
  max height 70; bold=63 / 77; freeplay-clear=10 / 24; case: default=Both,
  bold=Upper, freeplay-clear=Both; oracle quirk kept verbatim: `glyph_prefix('&') =
  "-andpersand-"` which does NOT exist in default.xml (only raw `&0000` frames exist),
  so `measure("&")` consumes zero advance — parity with AtlasText beats plausibility.
  Angel (`vcr-lite.fnt`): 90–char source trimmed to 5; `info.size=16 face="vcr-bmp"`,
  `lineHeight=18`, page file `vcr-bmp.png`; `measure("AB")=(20,18)`; glyph rect check
  `validate() == Ok(…)`. Overflow fixture `angel-overflow.fnt` → `validate() == Err(GlyphOverflowsPage{ … })`
  (crate integration test asserts the Err; that is the "exit 1 on overflow" check).
  Angel `measure` uses the SAME widest-line semantics as Sparrow; discriminating case
  `measure("AB\nC") = (20, 36)` (line 1 = 10+10; line 2 = 0 since `C` is unknown: a
  last-line implementation reports 0, a sum-implementation reports 30 — decisive),
  with a positive `(A,B)` kerning test also run across a newline to pin that the
  kerning pair is not applied across lines.
  Additional pinned decisions: negative `width`/`height` in a SubTexture frame →
  `Malformed` (defensive; real data has none); glyph metrics take the LOWEST numeric
  4-digit suffix per prefix (mirrors flixel `FlxFrame.sortFrames` numeric ordering, not
  document order).
- **Gate C** integration tests in `oxide-vfs` (runtime-built temp trees + committed
  `tests/fixtures/vfs/**`): (a) override precedence across two mods — last mod wins, both
  base and first-mod both shadowed; (b) zip mod via `zip` crate, manifest at archive root,
  traversal entry rejected (`..` and absolute), bad zip → clear Err, (c) case-exactness —
  `Bf.PNG` does NOT resolve `bf.png` on any platform (also via invalidate events: a
  differently-cased event path must not make an on-disk `bf.png` resolvable as `BF.PNG`);
  (d) duplicate ids rejected;
  (e) `invalidate(path)` re-reads changed file content AND (decisive part) a file created
  AFTER the index cache was built does not resolve until `invalidate` is called on it —
  the test must fail if `invalidate` is a no-op;
  (f) malformed manifest fixture → `VfsError::Manifest`, not panic.

## 5. Deliberate deferrals

- Animate Atlas **playback** (`AN` frames → display list): M5/M6 spike with `bf` (plan §8).
- Sparrow `rotated` rendering geometry: stored as a parsed fact, consumed by the renderer
  in M5+ (rotation/transposition is render-side).
- Build-time atlasing (`assets.pak`, packed assets): `oxide pack` is M3/M9 territory.
- No window, no rendering, no gameplay: the M2 gates are headless and numeric per the
  milestone non-goals.
