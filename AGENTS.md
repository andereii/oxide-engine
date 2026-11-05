# AGENTS.md — Oxide Engine

Instructions for coding agents (and humans) working in this repository.
This file is authoritative: if a task conflicts with it, follow this file and say so.

## 1. Non-negotiables

1. **English only** in all repository artifacts: identifiers, comments, file names, docstrings,
   commit messages, docs. No Spanish in code.
2. **Verification-first.** Work is done when a gate command exits 0 and its output is quoted.
   "It should work" is not a result.
3. **Never weaken a gate.** Do not edit tests, tolerances, fixtures, or thresholds to make
   something pass. Gates change only through an ADR in `docs/DECISIONS.md`.
4. **One milestone at a time.** No starting M(n+1) before M(n) is green (see §5).
5. **Data formats are frozen.** Charts, metadata, characters, stages, note styles and levels
   are read exactly as V-Slice writes them. No invented fields, no normalisation, no
   `#[serde(deny_unknown_fields)]`.
6. **No runtime media transcoding.** Conversion happens only in `oxide pack` (offline).
7. **No scope creep.** Out of scope, permanently: Chart Editor, Stage Editor, HScript
   compatibility, V-Slice mod compatibility, Newgrounds, web, mobile, native 4K, KTX2/BC7.

## 2. Commands

```bash
cargo xtask verify      # the only command that matters: fmt + clippy + tests + gates
cargo xtask fmt         # cargo fmt --all -- --check
cargo xtask clippy      # cargo clippy --workspace --all-targets -- -D warnings
cargo xtask test        # cargo test --workspace
cargo xtask parse-all --dir <assets>   # every known JSON parses (M1 gate)
cargo xtask fixtures-conductor         # Conductor vs Haxe oracle, delta <= 1 ms (M4 gate)
cargo xtask replay                     # input replays are deterministic (M5 gate)
cargo xtask perf-budget                # 60 fps on the heaviest chart (M5 gate)
cargo xtask media-check                # stem alignment, sizes, "no file grew" (M3 gate)
oxide pack | new | check | dev         # mod + asset tooling (part of the binary)
```

If a gate is not implemented yet it must exit **2** with `not implemented yet`, never 0.

## 3. Layout

| Crate | Responsibility |
|---|---|
| `oxide-data` | serde schemas + `EventRegistry` + validation; no I/O |
| `oxide-atlas` | Sparrow (XML), Packer (JSON), Animate Atlas |
| `oxide-font` | bitmap fonts + TTF |
| `oxide-vfs` | mod layers, zip, path resolution, hot reload |
| `oxide-audio` | Kira clock, multitrack Inst/Voices, offsets, seek, calibration |
| `oxide-input` | high-precision input capture |
| `oxide-script` | mruby engine, Ruby API, events, sandbox |
| `oxide-game` | Bevy app: states, gameplay, characters, stages, notes, UI, saves |
| `oxide-media` | offline packer + media verification |
| `oxide-tools` | fixtures, oracle runner, visual diff, fuzz |

Dependency direction is one way: `data`/`atlas`/`font`/`vfs`/`audio`/`input`/`script` must not
depend on `game`. `game` is the only crate allowed to know about Bevy plugins and states.

## 4. Code conventions

- `rustfmt` + `clippy -D warnings` are the style guide. No `#[allow(...)]` without a comment
  explaining why.
- Errors: `thiserror` per crate. **No `unwrap`/`expect`** on data from files, network, user
  input, or any external source. `unwrap` is allowed only in tests.
- Public API: `pub` only what another crate needs. Everything else `pub(crate)`.
- No speculative abstractions, no traits with one implementation, no `Box<dyn>` where a match
  on an enum is clearer.
- Every `unsafe` block needs a `// SAFETY:` comment and a test.
- Numbers that come from the game (BPM, offsets, beat lengths) are `f32`/`f64` in **milliseconds**
  as V-Slice stores them; do not convert units silently.

## 5. Milestones

M0 Skeleton · M1 Data · M2 Assets · M3 Media · M4 Clock+Input · M5 Vertical slice ·
M6 Full gameplay · M7 Meta/UI · M8 Scripting/VFS · M9 Release.

Rules: no starting the next milestone before the current gate is green; M3 and M7 may run in
parallel with M4-M6; if a milestone spends 2× its session budget, cut scope **inside** that
milestone and record it in `docs/PROGRESS.md`.

## 6. Ledgers (update them, they are the memory of the project)

- `docs/PROGRESS.md` — append-only: date, milestone, gate command, key output, sizes, next step.
- `docs/DECISIONS.md` — one ADR per non-trivial decision: context, options, decision,
  consequences, and what would make us revisit it.

## 7. Dependencies and assets

- Bevy is pinned to `=0.20`; `Cargo.lock` is committed.
- Adding a dependency requires an ADR line: crate, version, licence, build cost, alternative.
- Generated media (`assets-raw/`, `assets-dist/`, `assets.pak`, `*.zip`) is never committed.
- Test fixtures (`tests/fixtures/**`) are committed and are the ground truth: do not regenerate
  them to make a test pass.
- The reference implementation (Haxe) lives outside this repo and is used as an oracle; its
  build instructions are in `~/Descargas/fnf/Funkin/BUILD-LINUX.md`.

## 8. Forbidden actions

- `git push`, `git rebase`, history rewriting, or committing generated assets.
- Editing files outside the crate named in the task.
- Editing `tests/fixtures/**`, gate thresholds, or `xtask/` from an implementation task.
- Adding a native dependency (C/C++) without an ADR — this project's promise is "no DLLs on
  Windows, no LGPL surprises".
- Running long builds in parallel from multiple agents on the same machine (compile contention).
