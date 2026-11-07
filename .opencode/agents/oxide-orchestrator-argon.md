---
description: Optional Gemini-based Oxide Engine coordinator. Same behavior-first workflow and permissions as the daily orchestrator; availability and budget must be checked, not inferred from old notes.
mode: primary
model: opencode-go/gemini-4-argon
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

This is an optional model choice, not a different architecture or acceptance standard.
Report provider errors explicitly; do not modify other agents, change providers or
increase spending to hide unavailable service. Historical outage/pricing notes are not
current facts. Resume from inspected working-tree/progress state, never bootstrap over
existing code or rely on stale success reports.
