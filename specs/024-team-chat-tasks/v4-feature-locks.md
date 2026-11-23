# R011-2 / V4: Feature edit leases

- Parent: R011 Team Collaboration; R006 atomic changesets and revision validation are dependencies.
- Full-project human editors claim a 45-second renewable lease on an existing feature (organization/project/source/feature). A server-generated authenticated actor identity and client-generated lease token protect ownership and stale releases.
- Source row transaction lock serializes lease acquisition with all Core changesets, including bulk, reverts, scoped and service edits. Active leases held by a different actor block commitment; stale revision checks remain unchanged. No transaction is kept open while editing.
- Lease lifecycle: release on cancel, successful commit, or expiry after network loss. SSE sends only scoped invalidations; list endpoint returns owner name/expiry, no token. Expired leases are invisible and reclaimed on acquisition.
- First UI workflow: single-feature point movement, vertex editing, and erase; other edit paths are protected at commit, not automatically claimed. Scoped accounts remain able to edit permitted features but cannot enumerate or claim project-wide leases.
- Same-account sessions share actor identity for commit authorization; token separates acquisition and release. No automatic draft rebasing or simultaneous geometry merging.
- Gates: real database integration (contention, expiry, cancellation, access scope and commit enforcement), TypeScript build, browser smoke, migration convergence before production.

## Evidence

- PostgreSQL integration: feature lease contention, same-account token protection, expiry/reacquisition, release on commit, unrelated feature writes, and scoped member denial/commit enforcement passed. Existing scoped GIS and WebSocket presence regression also passed (4 tests).
- Production TypeScript/Vite build passed. Collaboration smoke passed, and advanced editor browser smoke verified rejected claim, successful claim with visible owner, release after saving, bulk edit, create, undo/redo and sketch.
- SSE network integration passed (2 tests): committed spatial edits, lease acquisition and release delivered project-scoped invalidation events, plus private message and task regression.
- Core changeset revision checks still apply across the source; concurrent unrelated edits may require refreshing and reapplying a draft. Migration convergence and release security review remain pending.
