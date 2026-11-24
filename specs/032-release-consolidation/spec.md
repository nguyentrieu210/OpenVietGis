# Spec 032 — Repository and release consolidation

Parent R015; depends on Spec 030 and static inventory Spec 031. Objective: one reviewable v1.0 candidate, not one forced live deployment.

- Inventory all Git refs and worktrees with ancestry and dirty state; retain non-ancestor history and all active work.
- Preserve the GitHub main baseline and review relevant unreleased/live fixes. Integrate selected changes only in an isolated candidate; overlapping superseded work remains preserved.
- Designate apps/web-next as release frontend (consistent with current production Dockerfile); retain apps/web until capability parity and independent browser tests are proven.
- Use a single candidate SHA, lockfile checksums, artifact hashes, QGIS checksum, and observed migration graph head. Artifacts are local/non-production until all Spec 030 gates and approval pass.
- Do not change live services, symlinks, database, permissions, main, remote refs or delete branches/worktrees in this slice.

Acceptance: selected changes included, frontend build and focused regressions pass, single migration graph head, source/connector artifact integrity checked, full inventory and unresolved risks documented. Stable release and branch retirement are separate acceptance items.
