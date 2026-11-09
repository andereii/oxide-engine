# Product goal — Oxide Engine

Reimplement Friday Night Funkin' V-Slice as a Rust + Bevy engine with content parity,
optimized bundled assets and a pure-Rust Rhai modding API (Ruby/mruby is reserved for a
separate Oxide flavor/fork and is not implemented in the base branch). This is the active
product scope; `WORKFLOW.md` defines execution, not a different product.

## Required release behavior

- Play the reference content: 28 songs / 61 chart files, their declared difficulties,
  21 stages and 43 editorially counted characters, with score, health, combo, game over
  and results. File-classification counts are not unique-character counts.
- Preserve required V-Slice data compatibility and relevant event/timing semantics;
  compare conductor timing with the original within the approved 1 ms budget. Replay,
  rendering, performance and hardware input require their own evidence.
- Freeplay/albums, Story/10 levels, Options, Credits, Codex, compatible saves, week-6
  dialogue, video/subtitles and censorship variants.
- Own mod format (`mod.toml`), layered assets, Rhai script registration/events, hot reload and
  `oxide new/check/pack/dev`. No promise of V-Slice/HScript mod compatibility.
- Windows and Linux; 1280x720 base rendering with the agreed scaling modes.
- Release binary plus `assets.pak` within the approved 350 MB budget, self-contained on
  Windows without external DLLs. Report bytes and units explicitly; the media-only
  historical gate is not proof of the final binary-plus-assets budget.
- Offline Opus/WebP/AV1 conversion; runtime decoding only. Preserve no-file-growth,
  stem-alignment and fidelity criteria in the accepted media contract. Do not silently
  reinterpret MB/MiB or adjust limits while changing report labels.
- Approved stack: Bevy `=0.20` when required, Kira, serde and Rhai (`rhai` in M8 exclusively
  on the base branch; `mruby` is reserved for a separate flavor/fork); exact dependencies
  are governed by manifests/lockfile and ADRs, not an unverified future-version list.
- Required acceptance on both platforms, with traceable reference/fixture provenance.

## Exclusions

No Chart Editor, Stage Editor, HScript compatibility, V-Slice mod compatibility,
Newgrounds/medals, web/mobile build, native 1440p/4K, KTX2/BC7, or speculative support
for those features. Changing scope requires explicit human approval.

## Evidence and legal boundaries

Existing historical M0-M3 greens are scoped records, not fresh proof of CI or release.
M4 phase 1 exists; input/replay/jitter and broader native-game parity remain pending.
`cargo xtask verify` alone is a quick check, never product completion.

Behavior specifications may be derived from observing/analysing the original; Rust need
not reproduce its internal structure. This is neither formal Cleanroom certification
nor retroactive independent legal clean-room authorship. Keep original licences and
attribution; workflow separation does not grant rights to source code or bundled assets.
Do not upload the original corpus or source to CI/providers without the required rights
and authorization. Tests/fixtures intended for redistribution need provenance/licences.
