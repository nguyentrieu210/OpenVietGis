# Spec 031 — Product inventory & audit

Parent: R015 (Production & Commercial Readiness); cross-cutting R001–R015 and Spec 030 unified v1.0. Extends Spec 018 and community acceptance matrix; does not replace their gates.

## Objective
Freeze feature expansion while establishing one reproducible inventory, a prioritized remediation backlog and an evidence-based release decision. Preserve all features, history and user data.

## Requirements
- FR01: Inventory every tracked file at a pinned source commit, including frontend generations, backend, SDK, plugins, connectors, tests and release tooling.
- FR02: Extract static HTTP route decorators, MCP tool decorators and frontend control candidates with file/line references. Dynamic registrations, actual mount paths, keyboard actions and rendered visibility require runtime follow-up.
- FR03: Map product capability groups to implementation candidates, test candidates and mandatory acceptance criteria. A matching file or test is not evidence of passing execution.
- FR04: Record source/release divergence and ownership blockers without modifying production, deleting branches or merging code.
- FR05: Stable requires independent, exact-candidate runtime evidence satisfying Spec 030; unknowns remain NOT_RUN. No inferred completion percentages.
- FR06: Reproducible inventory generation and automated tests; publish a reviewable audit report and next-slice backlog.

## Acceptance
- AC01: All files in the pinned tree enumerated exactly once.
- AC02: Static candidates reference valid tracked files and source lines; parse errors explicitly recorded.
- AC03: Every registry group has an owner role, priority, source evidence (or explicit planned state), and next acceptance action.
- AC04: Inventory generation is deterministic and tests pass.
- AC05: Audit report identifies limitations, production remains untouched, release stays blocked pending actual evidence.

## Non-goals
No feature implementation, deletion, data migration, live changes or stable certification in this slice.
