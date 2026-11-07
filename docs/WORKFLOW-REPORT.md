# Workflow migration report — 2026-11-05

Implemented the human-approved workflow review and selective cleanroom-inspired
amendment. This is a before/after report, not acceptance of M4 or release. No Git
commit, branch, push, external install, OpenCode session/provider probe or original-game
edit was performed as part of the migration. Implementation and independent review did
use subagents of this Zed session; that is not a live test of OpenCode agents. Native probes read original source and compiled
separate harnesses. Existing product scope and numeric tolerances remain unchanged.

## Baseline and scope

- Initial worktree: clean; HEAD `733c39c` (`m4: ledger for phase 1`).
- Initial state: M0-M3 historical green entries; M4 phase 1 core/model exists, input,
  replay, jitter and independent full native Conductor comparison remain pending.
- Installed compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`; Cargo 1.99.1; PyYAML 6.0.3.
  OpenCode 2.2.5 was observed in the preceding read-only audit; no live policy/service
  enforcement was exercised. Haxe 4.3.7/Neko 2.4.1/ffprobe 8.1.3 provenance is recorded.
- Authorized changes: policy/roles/briefs, scope/queue helpers, stronger gates and tests,
  CI configuration, explicit acceptance reports, and the identified rounding correction.
  This does not implement the rest of M4, M5+ gameplay, Ruby runtime or a new renderer.
- ADR-0013 records workflow/acceptance changes; ADR-0014 records specific authorization
  and evidence for the corrected expectations. `Cargo.lock` adds an already-existing
  workspace path dependency (`oxide-audio` for xtask); no new Rust package dependency.

## Before / after

| Area | Before | After |
|---|---|---|
| Instruction source | Outer plan/prompts/templates plus manually copied active configuration | Repository-local AGENTS, GOAL and WORKFLOW; role-specific prompts; retired outer templates point to them |
| Simplicity | Exact internals prescribed before implementation; no design/negotiation | Observables/public boundaries first, private design freedom, brief direct/reuse alternative for non-trivial choices |
| Contract defects | Implement anyway and report later | Concrete objection + equivalence check; resolve affected part before implementation |
| Original architecture | Source-informed transcription could dictate structure | Behavioral specification separates observation, derived requirement and engine/model choice |
| Cleanroom claims | No explicit distinction | Selective specification/implementation/validation separation; no formal proof or retroactive legal independence claim |
| Task scope | One crate absolutely; tooling ownership contradicted Rust delegation prohibition | One objective + explicit files; coherent tooling tasks allowed; shared manifests have one owner |
| Reviewer | Arbitrary shell despite read-only label; pressure to find something | Shell/edit/network/subagent denied in its tool policy; supplied evidence and requested checks; zero findings valid |
| Implementation permissions | Editor and shell allowed everywhere | Editor defaults deny, allows source/tooling roots, protects goldens; shell escape limitations explicitly disclosed |
| Validation meaning | `verify` implemented only part of the advertised full suite | Default quick clearly non-acceptance; cumulative milestone/release profiles, pending obligations fail closed |
| Corpus coverage | Empty inputs/traversal omissions could approve; Packer failures excluded | Empty/missing/error paths fail; optional official corpus inventories; Packer status included |
| Media prerequisites | Category coverage/pairing defects could be discovered after output started | Official inventory and all 126 voice/instrumental pairings checked before tools/output/conversion |
| External tests | Missing tools/assets could return normally and count as passed | Explicit ignored portable tests; strict inclusion fails on missing prerequisites; roots provided via OXIDE_ASSETS_ROOT |
| Mini audio regression | Four copies, zero conversions/duration checks | Existing copies retained plus two synthetic source stems: two actual conversions, duration checks and alignment pairs |
| Conductor rounding | Binary32 interpretation shared by Rust, tests and Haxe model | Real native FlxMath utility probe; binary64 floor(x + 0.5) semantics, reviewed expectation correction |
| Conductor gate | Command unimplemented; deterministic generated set mistaken for oracle strength | Exact 61-chart + separate smoke replay, all updates including silent ones; trusted embedded input manifest prevents renamed-smoke substitution |
| Reference provenance | Citations/determinism, but circular semantic assumption | Native utility evidence and source hashes, model assumptions labelled, candidate review and independent native-game obligation pending |
| Build coordination | Per-process job count and instructions to wait indefinitely | Cooperative total-deadline queue, no nested runs, process-tree cleanup and explicit stale-lock recovery |
| Scope audit | Prompt-only crate restriction | Snapshot/delta audit includes untracked files; aggregate union for parallel batches; not a security sandbox |
| CI | No versioned workflow in checkout | Linux/Windows portable workflow and scoped artifacts; configuration is not claimed execution evidence |
| CI receipts | No explicit current-revision acceptance evidence | Explicit artifact directory, platform/revision/compiler/result checks; offline authenticity boundary remains explicit |
| Resume | M0/M1 creation instructions on an existing M4 repo | Both kickoff/continue inspect state and resume; never overwrite the workspace |
| Budget/model fallback | Successful sessions as metric, automatic scope cuts, cross-provider fallback | Count failures, diagnose before further edits, no automatic scope cuts; global fallback limited to existing DeepSeek with bounded notified attempts |

## Conductor correction and reviewed fixture changes

`tools/oracle/probe_rounding.sh` compiled the real installed FlxMath method to native
hxcpp while slicing unrelated graphics fields without replacing the retained method.
Ten discriminating cases cover positive/negative halves, precision and a value above
i32 range. For example, `0.5 @ 0 -> 1` and `1.23456789 @ 6 -> 1.234568`.

The native utility result justified correcting the binary32 assumption, not certifying
all original Conductor logic. Full timelines still come from a source-informed model.

Candidate review (`tools/oracle/candidate-review.json`):

- 41 timeline files changed; 21, including smoke, are unchanged.
- 86,131 total rows retained, with identical per-kind counts/index sequences.
- 127 matched rows shifted by at most 1 ms; the monster boundary formerly coalesced by
  binary32 is separated correctly. Dispatch ordering within an update is unchanged.
- Existing 1 ms and relative length tolerances were not loosened.
- Generator/replay manifest and input/source/runtime hashes are recorded. Verification
  never repairs missing fixtures; alternate descriptors must match the trusted baseline.

## Validation performed

All Rust commands during the initial 2026-11-05 run used `RUSTUP_TOOLCHAIN=stable
CARGO_NET_OFFLINE=true CARGO_BUILD_JOBS=4` after verifying that the stable alias was
1.99.1. On 2026-11-06 follow-up verification, Rustup installed the exact named toolchain
`1.99.1-x86_64-unknown-linux-gnu` (`rustfmt`, `clippy`), and all checks below also passed
directly without the `RUSTUP_TOOLCHAIN=stable` override, including full-corpus `media_corpus`
conversion/validation under the M4 milestone profile (`target/verify-m4.json`, expected exit 2
due to pending native Conductor/input/replay/jitter gates and missing CI receipts).

| Check | Actual result |
|---|---|
| `bash -n ../preflight.sh`; `bash ../preflight.sh` | Pass; static-only notice, 0 lint errors |
| `python3 -B -m unittest discover -s tools -p test_workflow.py -v` | 48 tests passed, including queue contention, separate-group timeout cleanup, literal argv, scope and static permission cases |
| `cargo test --locked --workspace` | Pass; external/tool/real-asset tests explicitly ignored, not counted as external coverage |
| `OXIDE_ASSETS_ROOT=/home/alex/Descargas/fnf/Funkin/assets cargo test --locked --workspace -- --include-ignored` | Pass, including real data/atlas/fonts, actual mini media conversions and strict missing-prerequisite/negative paths |
| `cargo xtask verify --profile quick` | Pass: formatting, Clippy -D warnings and portable tests; milestone_acceptance=false |
| `cargo xtask parse-all --corpus --dir /home/alex/Descargas/fnf/Funkin/assets` | Pass: 432/432 JSON, 61 charts, 78 metadata, 21 stages; measured taxonomy retained |
| `cargo xtask parse-atlas --corpus --dir /home/alex/Descargas/fnf/Funkin/assets` | Pass: 266 XML, 205 Sparrow, 61 non-atlas, 88 Animate, no parser failures |
| `cargo xtask fixtures-conductor` | Pass: 61/61 model timelines, 86,020 chart boundaries + 111 separate smoke boundaries, worst delta 0.000 ms |
| `bash tools/oracle/snapshot_validate.sh` | Pass: two builds and regenerations match frozen timelines and manifest byte-for-byte |
| Native real-utility probe | Passed during delegated correction; exact stdout/source/build provenance stored as reviewable files, original tree untouched |
| `cargo xtask verify --profile milestone --milestone M1 --assets /home/alex/Descargas/fnf/Funkin/assets --report target/verify-m1.json` | Expected exit 2: baseline + official data + strict real-data tests passed, but missing authentic dual-platform receipts and dirty local tree blocked acceptance |
| CI YAML structure | Parsed and checked for Linux/Windows jobs and scoped upload; not executed on GitHub |
| `git --no-pager --no-optional-locks diff --check` | Pass |

Full fixture set hash including replay manifest:
`ec4cc791930e20de96613da519dc85d4dd0c05b7c042d2ee2521c68104c4c41f`.

Initial focused validation exposed two remaining old binary32 integration expectations;
they were corrected against ADR-0014's observed semantics. A Unix-socket negative test
was replaced with a Linux proc read-error case because the sandbox forbids sockets;
the error-path assertion remains, rather than converting the test into a silent skip.

Independent review additionally found and prompted regressions for renamed smoke corpus
substitution, pre-conversion orphan stems and timed-out descendant processes. Those
counterexamples are now covered. Policy review verified safe resume, explicit limitations
and read-only reviewer configuration. The synthetic contract-objection/zero-finding pilot
in WORKFLOW is a protocol example, not an actual OpenCode behavior evaluation.

## Remaining limits and next objective

- M4 is NOT accepted. Real native-game Conductor traces, input/replay/jitter and actual
  cross-platform evidence remain pending and mandatory in the catalogue.
- Windows/GitHub CI, actual OpenCode permission enforcement and live LLM behavior were
  not executed here. No OpenCode model/provider probes, package installation or service
  restart were made; Zed implementation/review subagents were used. Review/reload the existing service manually when appropriate.
- CI artifact checks validate consistency offline, NOT origin/authenticity. Obtain real
  artifacts for the same clean revision and verify authentic run provenance; never
  synthesize receipts. Current dirty working tree cannot match clean CI acceptance.
- Full-corpus media conversion was NOT rerun for this migration. Strict synthetic media
  end-to-end regressions passed; historical 334.2 MB media output is not fresh acceptance
  and does not include the final release binary. Run the full media gate when accepting M3+.
- Native utility probes are finite, not formal proofs. The full model may still share
  unobserved assumptions with Rust; explicit native-game comparison remains pending.
- Queue/editor/scope controls are cooperative/application-level, not an OS sandbox.
  Snapshot races, detached/reparented groups and best-effort Windows cleanup have limits;
  an uncertain Rust cleanup retains its validation lock for manual inspection.
- Exact budget/prices/provider availability were not queried. Primary model selections
  remain unchanged; removing automatic Grok/Argon fallback is intentional, not a claim
  that DeepSeek always responds or enforces a dollar cap.

Resume with `/oxide-continue`: reconcile M4's model evidence with native original traces,
then continue timestamped input/replay/jitter work in separately scoped tasks. Do not
start M5 or claim release because quick verification is green.
