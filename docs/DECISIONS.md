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
