# Implementation quickstart
1. Start clean isolated worktree at verified main; record SHA, read CLAUDE.md, constitution, relevant ADRs and 062/064/065. Verify no active branch collisions.
2. Run git ls-files, cargo metadata --locked, pnpm workspace inventory, import graph, route manifest and CI trigger inventory. Do not execute stale root commands blindly.
3. Reproduce current pinned build and existing checks in isolated CI/staging. Record baseline using performance-budget.md. Inspect browser traces and network waterfall.
4. Implement tasks by dependency order. Run affected tests before committing; run full mapped quality gate on exact final candidate. Compare before/after metrics.
5. Review and security review; stage immutable image; rehearsal rollback with nonproduction data. Only deploy with appropriate authorization. Verify health and exact digest.
