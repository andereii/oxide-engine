# M3 contract — offline media packer (`oxide-media`, `oxide pack`, `assets.pak`)

Frozen for milestone M3. Binding rules: PLAN-ES.md §7.1–§7.7 + the orchestrator brief.
Build-time only: nothing here runs at game runtime; ffmpeg/ffprobe are EXTERNAL
binaries spawned by the tools, never linked and never spawned by
oxide-game/oxide-audio/oxide-input (ADR-0011). No runtime decoding, no asset loading,
no game code. oxide-atlas/font/vfs must not be touched beyond a strictly-needed pak
reader (if any: one ADR + a test; expected: none this milestone).

## 0. Measured corpus (this repo's assets, `~/Descargas/fnf/Funkin/assets`)

- 1.273 GB total; extensions present: 685 png (114 MB) · 432 json (28 MB) · 431 ogg
  (504 MB) · 413 mp3 (477 MB) · 266 xml · 114 hxc · 23 ttf · 2 otf · 14 txt · 12 frag ·
  14 videos = 8 mp4 (122 MB) + 6 mkv (21 MB) · 1 jpg · 1 fnt · 8 md · 7 yml · 7 srt ·
  3 fla · 1 gitignore/gitattributes.
- Tools present on this machine: ffmpeg 8.1.3 with libsvtav1, libwebp, libopus.
- Probe results (validated recipes): webp lossless → raw RGBA compare: byte-length
  equal, pixel identical (NOTE_hold_assets.png); bopeebo/Inst.ogg is 92 kbps → GROWS
  at 112k (939,552 → 1,213,643 bytes) → must be COPY per the never-grow rule;
  voices/inst relative first-sample delta stable (−8..0 samples, decoder boundary).
- 2hot/Inst.ogg = 105 kbps stereotypical; cartoons/*.ogg include 13–17 kbps SFX
  (never re-encode below target).

## 1. `crates/oxide-media` (library; headless, build-time)

```rust
#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg/ffprobe is missing or old: {0}")]
    Tools(String),
    #[error("ffmpeg failed on {path:?}: {detail}")]
    Convert { path: PathBuf, detail: String },
    #[error("unexpected asset file {path:?} (extension .{ext})")]
    Unknown { path: PathBuf, ext: String },
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("zip: {0}")] Zip(String),
    #[error("verification failed: {0}")] Verify(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind { Audio { layer: AudioLayer }, Image, Video, Text, Font, Skip }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioLayer { Inst, Voices, Sfx }   // targets 112k / 64k / 48k

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    ConvertAudio,      // .ogg -> .opus, target per layer
    ConvertImage,      // png/jpg -> webp lossless (fallback: copy when it would grow)
    ConvertVideo,      // mp4/mkv -> webm av1+opus
    CopyAudio, CopyImage, CopyFont,       // never-grow / pass-through copies
    DropMp3, ExcludeVideo,                // counted removals
    TextOnly,                             // text: pak-only, deflate
    Skip,                                 // .fla / git metadata (reported)
}

pub struct PlanEntry {              // one source file
    pub rel: String,                // exact relative path from assets root (forward slashes)
    pub kind: EntryKind,
    pub disposition: Disposition,
}

impl Plan {                         // deterministic: sorted by rel
    pub fn build(root: &Path) -> Result<Plan, MediaError>;
    pub fn entries(&self) -> &[PlanEntry];
}

pub const EXCLUDED_VIDEOS: [&str; 3] = ["mobileRelease", "riftCollabTrailer", "boyfriendEverywhere"];

impl Layout: layer/extension classification (pure, unit-tested):
  - `songs/<song>/Inst*.ogg` -> Audio{Inst} (112k); `songs/<song>/Voices*.ogg` -> Audio{Voices} (64k);
    every other .ogg -> Audio{Sfx} (48k). `.mp3` anywhere -> DropMp3.
  - `.png|.jpg|.jpeg` -> Image (webp lossless). `.mp4|.mkv|.webm` under videos/ -> Video;
    one of EXCLUDED_VIDEOS (exact stem match) -> ExcludeVideo. Video files outside videos/ -> Unknown.
  - `.json|.xml|.txt|.rb|.frag|.hxc|.fnt|.md|.yml|.srt` -> Text (deflate in pak only).
  - `.ttf|.otf` -> Font (copy + STORE).
  - `.fla|.gitignore|.gitattributes|.git` -> Skip (reported count). Walker rules:
    symlinks are skipped (the assets tree is a real filesystem tree; symmetrical
    loop protection for free), a `.git` DIRECTORY is skipped like a `.git` file.
  - anything else -> Err(Unknown).

pub struct PackReport { /* counts + bytes + sha256 of pak, filled by run */ }
pub fn pack_and_verify(in_dir: &Path, out_dir: &Path, jobs: usize) -> Result<PackReport, MediaError>;
```

Conversion rules (verbatim from plan §7.4 with the never-grow guard):

- **Audio**: `ffmpeg -i in.ogg -c:a libopus -b:a <TARGET>k -vbr on out.opus`.
  Effective source bitrate = `format.size * 8 / format.duration` via ffprobe; rounded
  DOWN to the nearest kbps integer. If `src_kbps <= target` → COPY the source bytes
  unchanged (name keeps `.ogg`); the converted file is `<stem>.opus`. DURATION unchanged
  is checked post-hoc: `|dur_src - dur_out| <= 0.05 s`.
- **Images**: `ffmpeg -i in.png -c:v libwebp -lossless 1 -compression_level 6 out.webp`.
  If the output bytes >= source bytes → keep the original PNG/JPG (never grow, copy).
- **Video**: `ffmpeg -i in.mp4 -c:v libsvtav1 -preset 6 -crf 44 -svtav1-params tune=0
  -vf scale='min(1280\,iw)':-2 -c:a libopus -b:a 48k -f webm out.webm` (mp4 AND .mkv
  sources). EXCLUDED_VIDEOS are neither converted nor packed (exact list, not glob).
- **Text**: bytes only inside `assets.pak` as **Deflated**; fonts and media entries
  (kept .ogg, .opus, kept .png, .webp, .webm) go into the pak as **STORE**.
- **Determinism** (structural, tested): pak entries sorted by rel path; every
  modification timestamp fixed to 2000-01-01 00:00:00 UTC; fixed deflate level; spawn
  commands with a fixed environment (no locale/locale-dependent flags into ffmpeg).

Job parallelism: at most `jobs` ffmpeg processes at once (default 4; consistent with
the compile-contention rule). Probed conversions are the same recipes as §7.4.

## 2. `assets-dist` layout (release content, media-check semantics)

Mirrors the assets root:
- converted audio `songs/<song>/Inst.opus` or kept `.../Inst.ogg`;
- images `xxx.webp` or kept `xxx.png`;
- videos `videos/videos/xxx.webm` (the `videos/videos` nesting on disk is real and kept);
- the three excluded trailers: ABSENT from dist AND pak;
- `.mp3` everywhere: ABSENT, counted;
- text: ABSENT from the loose tree, present only in `assets.pak` (Deflated);
- fonts: copied loose (a loose dist tree must be a self-sufficient VFS root) AND STOREd
  in the pak;
- `assets.pak` itself is written at `<out>/assets.pak` holding the text (Deflated) AND
  every media + font entry (STORE), keyed by the same rel paths as dist.

`total_after` semantics (gate): bytes of the dist tree EXCLUDING assets.pak itself,
plus the pak bytes MINUS the media bytes it stores (i.e. media counted once, text
counted once as compressed). This matches plan §7.5 arithmetic (~340 MB expected).

## 3. Gates (xtask-owned, report format frozen)

`cargo xtask media-check --in ~/Descargas/fnf/Funkin/assets --out assets-dist`:

```
media-check: <in> -> <out>
tools: <ffmpeg version first line> | <ffprobe ok>
audio  before: <B> MB  after: <B> MB  (converted <n>, copied <n>)
images before: <B> MB  after: <B> MB  (converted <n>, kept png <n>)
video  before: <B> MB  after: <B> MB  (converted <n>, excluded <n>)
text   before: <B> MB  after: <B> MB  (deflate in pak, <n> files)
fonts  before: <B> MB  after: <B> MB  (<n> files)
skipped (non-runtime files): <n> (fla/git metadata)
mp3 dropped: <n> files, <B> MB
files_grew: 0
align_ok: <checked>/<checked>
duration_ok: <checked>/<checked>
video_ssim_min: <x.xx> (threshold 0.93)
image_pixels_identical: <n>/<n>   (pixels differing only where alpha==0: <m>)
total_after: <X> MB (limit 350)
excluded_videos: mobileRelease.mp4, riftCollabTrailer.mp4, boyfriendEverywhere.mp4
pak sha256: <hex>
```

Exit 0 iff: `files_grew == 0`, `align_ok == n/n`, `duration_ok == n/n`,
`image_pixels_identical == n/n`, `video_ssim_min >= 0.93`, `total_after <= 350 MB`.
If total_after lands in (350, 380]: still print true numbers, exit non-zero, and the
orchestrator writes the ADR (per the human rule).

`cargo xtask pack --in <assets> --out <assets-dist>` (default: repo-relative
`assets-dist/`) runs the packer once and prints the sha256 + entry count.
`oxide pack --check-tools` prints the tool versions and exits non-zero when missing.

**Determinism**: `cargo xtask pack` twice → byte-identical paks is proven STRUCTURALLY
by a committed integration test over `tests/fixtures/media/mini/**` (pack twice,
compare bytes AND sha256); a `--twice` flag repeats the full run on real assets when
wanted (the full run is expensive; the structural property is the same code path).

**Verification methods (headless, precise):**
- Images: decode source AND dist artefact via `ffmpeg ... -f rawvideo -pix_fmt rgba`
  and compare byte-for-byte in memory: identical or differing only in RGB where both
  alpha bytes are 0 (both counters printed).
- Audio alignment (AMENDED 2026-11-05 after measurement): fixed-threshold onset
  detection is NOT a valid alignment metric — real assets include gradual crescendos
  (`milf/Inst.ogg` crawls across the threshold for ~53 ms; opus decorations shift the
  crossing) which produce false 50 ms "drift". The metric is instead **relative
  cross-correlation delay**: decode source and dist artefact to s16le mono 48 kHz,
  decimate both by 6 (mean filter) to 8 kHz over the first 3 s, take the source peak
  sample as the anchor, then correlate the source and dist windows for integer
  delays in +/-512 samples at 8 kHz and refine +/-64 samples at 48 kHz around the
  best coarse delay — the per-file delay is the refinement's argmax (normalized by
  energy; a stem silent across the 6 s probe is re-probed uncapped around its
  first/loudest late signal, not via a threshold fallback). Pair budget UNCHANGED:
  `|delay_inst − delay_voices| <= 128 samples` (~2.7 ms) computed on source vs dist
  delays — i.e. conversion must not change the relative stem alignment.
- Audio alignment pairing: each `Voices[...].ogg` stem is verified against its matching
  `Inst[-variant]` (exact variant suffix; otherwise the unvariant Inst). `checked`
  counts every Voices stem found in both source and dist (126 on the real corpus;
  `blazin/Inst` has no Voices counterpart and is duration-checked only).
- Video: SSIM (`ffmpeg -ssim`) is computed for CONVERTED videos only; copies are
  byte-compared like all copies (hash equality), so a copied video's fidelity is
  its source's own.
- `files_grew`: per source file cross-checked against its dist artefact (converted or
  copy). Copies are byte-compared (hash equality), converted must be strictly smaller.
- Coverage floor: every CONVERTED artefact must have its check performed (duration /
  pixel / ssim) — a conversion without a check fails the gate (`unverified > 0` is
  non-zero exit), so an empty or mis-pointed `--in` cannot pass silently.

## 4. Fixtures (orchestrator-owned, committed)

`tests/fixtures/media/mini/`: `data/micro.json` (text), `images/micro.png` (8×8
hand-built PNG), `songs/testsong/Inst.ogg` + `Voices-bf.ogg` (1 s ffmpeg-made),
`sounds/beep.ogg` (SFX 48k), `sounds/quiet.ogg` (re-encoded 32k to exercise
copy-on-target), `videos/tiny.mp4` (0.5 s 64×64 h264), `videos/boyfriendEverywhere.mp4`
(Stub name to prove the exclusion list exactness), `fonts/micro.ttf` (copied from the
smallest real font), `script/module.hxc`; plus one `.fla` stub to prove Skip counting.
Unit/integration tests over this fixture are ffmpeg-guarded (`--check-tools`): they
skip with a notice when ffmpeg/ffprobe are absent.

## 5. Deliberate deferrals

- Runtime pak reading through the VFS release mode (`Source::Pak`): M8 — M2's VFS is
  untouched this milestone.
- alpha-premultiplication / 1-px edge extrusion (plan §7.2 optional fringe kill): M5
  renderer decision, no-op in the packer.
- `assets.pak` as THE release payload (dist folder dropped): M9.
- mp3 handling for a hypothetical web build: out of scope (the drop is release-wide).
