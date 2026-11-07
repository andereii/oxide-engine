# Workflow — behavior first, evidence scoped

Active process implementing the human-approved 2026-11-05 review and selective
cleanroom-inspired amendment. Policy is in `../AGENTS.md`, product scope in `GOAL.md`.
The Spanish planning folder and its old agent copies are historical, not alternate rules.

## 1. Philosophy and proportionality

Reimplement observables, not the original class structure. A simpler implementation
must preserve compatibility, safety, event order, timing and agreed budgets. Simpler
means lower total maintenance cost, not fewer lines or fewer tests. Avoid speculative
frameworks, extension points and unnecessary dependencies. Rust traits required by a
real boundary, Bevy or Kira are not forbidden merely for having one implementation.

A local fix needs a focused test. A non-trivial design needs at most five decision bullets:
behavior preserved, existing facility/simpler option, selected approach, tradeoff and
validation. Public integration changes need a contract/review. Clock/VFS/FFI, fixtures
and acceptance changes need stronger independent evidence. Do not ask for multiple
alternatives or ADRs for an obvious private helper.

## 2. Selective cleanroom-inspired specification

For parity-sensitive work, explicitly separate these artifacts:

1. **Observed behavior:** inputs/outputs and native-game/runtime traces with versions,
   source identity, units and edge cases. Original files are read-only.
2. **Derived requirements:** invariants and consequences, labelled as derived rather
   than observed. Preserve observable quirks where required; don't invent missing facts.
3. **Engine decisions/model assumptions:** Rust design and approximations, labelled
   separately. A source-informed transcription is not an independent full-game oracle.
4. **Implementation:** the implementer receives the approved behavior and integration
   boundaries, not a mandatory translation of original classes/pseudocode.
5. **Validation:** compare against actual reference execution when available, with
   additional independently derived and negative cases. Determinism alone is insufficient.

The orchestrator can perform the analysis phase; no permanent new agent is required.
For high-risk new work, use a fresh implementation session with a behavior-only brief
and no original-source snippets. Assign original-source analysis to the analyst phase;
a specification-only implementer requests missing observations instead of consulting raw
Haxe. Accidental source exposure is recorded, not hidden behind an independence claim.
Source access restrictions are procedural unless actual
read/shell boundaries have been established. Existing Rust/Haxe exposure cannot be undone
by a prompt or a fresh session. Model pretraining/exposure is not audited. This workflow
claims neither legal independence nor formal proofs/statistical Cleanroom certification.

Conductor example: `../tools/oracle/BEHAVIOR.md` distinguishes native utility observations
from model-only timelines. Broader real-game traces remain a named acceptance obligation;
matching the model does not close it. Finite test cases are not exhaustive float proofs.

## 3. Roles and ownership

- **Orchestrator:** inspect current state, identify behavior, choose the smallest useful
  decomposition, resolve objections, own integration/acceptance and update ledgers. It
  may make small coordination/tooling changes; delegation is not mandatory ceremony.
- **Implementer:** one objective and explicit write set, simplest private implementation,
  regression tests and evidence. If an interface is wrong or needlessly complex, propose
  a specific alternative and pause that part; never implement a known error blindly.
- **Reviewer:** read code, the supplied diff and reports. Check correctness, external
  compatibility, state/abstraction/duplication costs, coverage, scope and approval of
  expectation changes. Zero findings is valid. No shell/edit; request checks from the
  validator. State what was inspected and what was not.

Orchestrator is an ownership role, not exclusive permission to author gate code. An
explicit tooling task may own `xtask/`; fixtures/tolerances remain separately protected.
Shared manifests and `Cargo.lock` have one owner. No Git writes unless authorized.

## 4. Task brief (copy only the relevant fields)

```text
Objective: observable change, one sentence.
Risk: local | integration | parity/security/acceptance.
Read: repository policy + relevant contract/spec/ADR and exact files.
Behavior: inputs/outputs, units, state transitions, compatibility and edge cases.
Evidence provenance: observed | derived | engine decision/model assumption.
Reference access: analyst/source-informed | specification-only implementation.
Write set: exact files or directory prefixes, including permitted regression tests.
Protected: public consumers, reference fixtures, thresholds, manifests/config not assigned.
Integration contract: only necessary public signatures/semantics; private design is free.
Simplicity check: direct/reuse option, selected approach/tradeoff (omit for trivial work).
Objection path: propose alternative + equivalence check; pause affected part for decision.
Dependencies: allowed existing dependencies; approval required for additions.
Validation owner/slot: worker or orchestrator; command, timeout, prerequisites and coverage.
Acceptance: concrete expected behavior; quick check is not milestone acceptance.
Report: result, files, behavior/decision, actual command+exit/output, skipped/blocked areas,
        deviations/objection and next step. No fabricated command output.
```

A valid objection is evidence of useful implementation judgment, not insubordination.
Adding/strengthening a task's own tests is allowed. Changing goldens, tolerances or what
acceptance means requires the separate ADR/review/human path, not a hidden test edit.

## 5. Scope, concurrency and bounded jobs

From the repository root:

```sh
python3 tools/workflow.py snapshot --out target/task-baseline.json
python3 tools/workflow.py run --timeout 1800 -- cargo xtask verify --profile quick
python3 tools/workflow.py scope --baseline target/task-baseline.json --allow crates/oxide-input/
```

At most three independent subagents. Never overlap write sets; never run simultaneous
heavy Cargo, media or native-probe jobs on this machine. Workers can read/design/edit
while the validator has the build slot. Use `CARGO_BUILD_JOBS=4` as a process limit.

The queue uses a cooperative exclusive lock and a total wait/execution deadline. It
runs argv without shell evaluation and cleans up its own child group where supported.
Do not nest it. `verify` holds a separate invocation lock and invokes its gates directly.
Inspect owner/process state after an interrupted/stale lock; never delete active locks
or kill unrelated processes. Gate commands use their own POSIX process groups; the outer queue snapshots live
invocation ancestry before termination so separate-group gates are included in cleanup.
Collection races, escaped/reparented groups and Windows tree cleanup still have limits.

Snapshot/scope checks include nonignored untracked files and content changes/deletions,
not merely `git diff`. They are after-the-fact audits, not filesystem enforcement or
atomic snapshots. With concurrent shared-worktree tasks, take one batch baseline and
check the **union** of write sets; individual writer attribution is not possible. A
separate worktree isolates integration state, not permissions, and is not auto-created.

## 6. Verification profiles and reports

```sh
cargo xtask verify --profile quick
cargo xtask verify --profile milestone --milestone M4 --assets /absolute/assets --ci-evidence /absolute/artifacts
cargo xtask verify --profile release --assets /absolute/assets --ci-evidence /absolute/artifacts
```

Use the queue for these commands during agent work. No profile means quick. Reports
(default `target/verify.json`) invalidate old success before execution and include Git
identity/dirty state, OS, compiler, commands, deadlines, status, declared coverage and
prerequisite errors. Exit 0 only means the selected scope passed; quick always reports
`milestone_acceptance: false`. Invalid/missing prerequisites and unimplemented gates
remain nonzero; failures cannot be converted to skips. Help is not evidence and writes
no report. A report path write failure fails validation.

The static catalogue is cumulative, not auto-discovery. CI is required for aggregate
acceptance but does not prevent independent local corpus/conductor checks. Pending
required replay/input/jitter/render/gameplay/UI/Ruby/release gates prevent acceptance.
Native-game conductor traces are also required separately from model agreement.

Portable tests explicitly ignore external media/font checks; acceptance invokes them
strictly and missing tools/assets fail. Generic parser validation rejects empty inputs;
`--corpus` additionally checks the measured official inventory. Do not apply reference
counts to arbitrary mods. Keep external corpus paths explicit during acceptance.

Full media acceptance is deliberately expensive. Mini regressions cover conversion,
copy, duration, alignment, fidelity and packaging without reconverting the corpus per
edit. Never reuse old acceptance after a relevant input changes. No result cache is
introduced in this workflow revision.

## 7. CI evidence and trust boundary

`.github/workflows/verify.yml` runs portable quick verification on Linux and Windows and
uploads JSON reports; it does not upload original assets or conductor model snapshots.
The checked-in workflow is not evidence it ran. Download the authentic artifacts for the
same revision, inspect their run provenance, and place them as `linux.json` and
`windows.json` in the explicit `--ci-evidence` directory.

The offline checker validates scope, successful executed baseline gates, platform,
clean Git trees, matching HEAD/CI SHA, compiler and complete CI identity. It does NOT
contact GitHub or authenticate artifact origin: reports explicitly retain
`provenance_verified: false`. Human/orchestrator must verify authentic run URLs/artifact
retrieval before using them as evidence; never synthesize receipts. Missing/stale/dirty
or inconsistent evidence blocks acceptance. Local Linux success does not prove Windows.
A fork's trusted CI repository/run should be checked against the intended project.

Python helpers need Python 3.11+; static agent lint additionally uses existing PyYAML.
Rust is pinned to the locally verified compiler, not moving `stable`. CI availability
of that toolchain and runtime behavior are unverified until the workflow actually runs.
Hardware tests remain platform-specific: uinput does not prove Windows Raw Input,
headless simulation does not measure real audio/GPU latency, and tests must not request
sudo/device ACL changes automatically.

## 8. Permissions, providers and capability claims

The active OpenCode configuration uses V2 `permissions[]` (locally observed 2.2.5), not
an assumed V1 schema. Reviewer denies shell/edit; implementation editor access defaults
to deny and opens only source/tooling roots, with goldens denied. These configured-tool
policies do not stop arbitrary shell processes from writing elsewhere. Dynamic per-task
OpenCode ACLs are not assumed supported. Strong isolation requires a separately verified
filesystem/executor boundary; until then write-set audits are procedural controls.

Observed 2026-11-06 (OpenCode 2.2.5, config resolution only, no live enforcement test):
`opencode debug agents` loads all five agents with the intended rule lists. Rules are
evaluated last-match-wins; `*` becomes `.*` and therefore crosses `/`, so `crates/*`
covers nested files. Edit resources are paths relative to the session directory: launch
OpenCode from `oxengine/`, otherwise implementer edits (`oxengine/crates/...`) are denied.
An action whose last rule is `* deny` (reviewer shell/edit) hides that tool entirely.

`tools/workflow_lint.py`/outer `preflight.sh` only lint static files; no model/network
calls or live security claims. Verify actual service configuration before asserting
runtime enforcement. Do not test destructive commands against real project files.

Models remain as configured. Fallback attempts/notifications are bounded; no automatic
unapproved cost escalation. If price/availability cannot be verified, pause for a human
rather than infer an allowance from a model name. Fallback never expands tool rights.
Review independence comes from evidence, not merely choosing another model.

## 9. Resume, budgets and decisions

`/oxide-continue` and `/oxide-kickoff` inspect state and resume; neither recreates the
workspace. Read policy, goal, latest progress, then the relevant contract/evidence; do
not blindly trust an old `Next` if local changes or unresolved reviews contradict it.

Record attempts (including failures), elapsed work and tokens/cost only when actually
available. After two fixes without new evidence, stop edits and run the smallest
experiment separating the hypotheses. Budget exhaustion means diagnose/propose, never
remove required functionality or weaken a gate. Ask for scope/licence/native-dependency,
fixture/tolerance, system-permission, provider-cost or external-upload changes. Ordinary
private design choices and stronger regressions proceed autonomously within scope.

## 10. Rollout and pilot

Approved rollout: baseline -> unified policy -> behavior briefs -> rounding/reference
correction -> false-green regressions -> explicit profiles/CI -> permissions/queue ->
pilot and resume M4. No general engine rewrite or implementation of M5+ is included.

Use the workflow regression suite to exercise valid/invalid scopes, queue contention,
timeouts (including separate-group descendants), argv safety, permission lint, reference
expectations and blocked acceptance. The following pilot is a **synthetic protocol
example**, not a real OpenCode model session or milestone evidence:

- Objective: preserve Conductor conversions for an existing Rust consumer. An initial
  contract proposes a converter trait plus factory with one current implementation.
- Implementer objection: reuse the existing concrete Conductor API; the extra trait and
  factory add coordination without a current boundary. Keep public callers unchanged;
  validate with conversion/offset/boundary regressions and the approved fixture gate.
- Orchestrator resolution: accept the concrete option; retain behavior/consumer contract
  and remove only the speculative private design requirement. No product/tolerance change.
- Reviewer outcome: zero findings is valid after checking those callers, the smaller diff
  and actual tests; list native-game/hardware/platform limitations rather than inventing
  an objection to fill a quota.
- Negative acceptance: missing CI evidence or a pending native Conductor/input gate must
  remain blocked even if all portable tests pass. CLI/catalogue regressions exercise it.

Actual LLM behavior, runtime permission enforcement and Windows execution need separate
observation; do not call the synthetic example a live model evaluation.
See `WORKFLOW-REPORT.md` for the applied before/after report and validation limitations.
