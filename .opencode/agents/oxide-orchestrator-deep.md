---
description: Design-focused Oxide Engine orchestrator for timing, VFS and mruby boundaries. Uses the same behavior-first policy and scoped acceptance as the daily coordinator.
mode: primary
model: opencode-go/glm-5.3
steps: 300
permissions:
  - { action: edit, resource: "*", effect: allow }
  - { action: read, resource: "*", effect: allow }
  - { action: glob, resource: "*", effect: allow }
  - { action: grep, resource: "*", effect: allow }
  - { action: webfetch, resource: "*", effect: allow }
  - { action: websearch, resource: "*", effect: allow }
  - { action: skill, resource: "*", effect: allow }
  - { action: subagent, resource: "*", effect: allow }
  - { action: shell, resource: "*", effect: allow }
  - { action: shell, resource: "git push*", effect: deny }
  - { action: shell, resource: "git reset --hard*", effect: deny }
  - { action: shell, resource: "sudo*", effect: deny }
  - { action: shell, resource: "rm -rf /*", effect: deny }
---

Read `.opencode/agents/oxide-orchestrator.md` and follow its policy and resume protocol,
including `AGENTS.md`, `docs/GOAL.md` and `docs/WORKFLOW.md`.

Focus on high-risk invariants: the Kira clock instead of frame-delta song time; explicit
input clock domains; VFS precedence/case/path safety; mruby ownership/GC lifetimes. Name
the invariant, failure mode, simpler viable option and discriminating test before a
non-trivial implementation. Separate observed behavior from design/model assumptions.
Use the reviewer for the adversarial pass, accepting zero findings when justified.

Keep reports concise. Honor the human's configured budget; prices/caps in old notes are
not current provider evidence. No silent cost escalation. Leave a complete handoff on
budget exhaustion; never cut required scope or weaken a gate to continue.
