# Oxide Engine

Rust + Bevy reimplementation of Friday Night Funkin' V-Slice with optimized bundled
assets and a new Ruby/mruby modding API. Early milestones; M4 clock/input is partial.
There is no claim of full gameplay, native-game parity or release acceptance yet.

## Development workflow

Read [AGENTS.md](AGENTS.md), the [product goal](docs/GOAL.md) and
[behavior-first workflow](docs/WORKFLOW.md). Resume OpenCode with `/oxide-continue`;
`/oxide-kickoff` also safely resumes and never overwrites an existing workspace.

Prerequisites: pinned Rust 1.99.1 (rustfmt/Clippy), Python 3.11+. Static agent lint uses
PyYAML (`tools/requirements-workflow.txt`). Media/native probes need separately installed
tools; portable tests do not silently count those checks as passed.

```sh
python3 tools/workflow_lint.py
python3 -B -m unittest discover -s tools -p test_workflow.py -v
python3 tools/workflow.py run --timeout 1800 -- cargo xtask verify
cargo xtask verify --help
```

No profile means **quick**, never milestone acceptance. Explicit milestone/release
profiles require their full cumulative evidence and fail closed on pending work. Reports
are written to `target/verify.json`; original assets are not uploaded to CI. See
[the migration report](docs/WORKFLOW-REPORT.md) for before/after changes and validation.

Reimplement required observables rather than Haxe architecture. Specifications separate
observed behavior, derived requirements and model/design assumptions. This is selective
cleanroom-inspired engineering, not formal certification or a legal authorship guarantee.
