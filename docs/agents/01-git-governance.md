# Agent 01 — Git governance and branch inventory

## Scope and ownership
Own: documentation report docs/qa/agent-01-git.md only.
Read: Git history and refs, ROADMAP.md, specs, docs/architecture.md.
No pushes, force updates, branch deletion, merge, rebases or stashes.
Do not assume unmerged branches represent missing production code.

## Task
1. Compare branch histories against the pinned main baseline 5baade99dcb17024610993ee0c13e84af3d9e7b8.
2. Review the six remote branches noted in docs/qa/2026-11-23-control-kit-audit.md.
3. For each, record ahead/behind and unique files/commits. If shallow history is insufficient, record NOT_VERIFIED rather than guessing.
4. Separate feature code, specs, docs and historical backups; propose keep/merge/review dispositions with evidence.
5. Identify branch protection and PR/CI gate gaps from checked-in config.

## Deliverable
Produce docs/qa/agent-01-git.md including evidence, uncertainty, risk priority and safe merge plan.
Complete only when every branch has a disposition and no refs were mutated.
