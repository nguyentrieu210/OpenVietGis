# Research decisions
- D01 Existing SSE is adequate as notification transport; it carries no durable cursor and only project-level invalidation. Add durable transactional event log, not WebSocket replacement. Inspect all R006 mutation paths before defining outbox coverage.
- D02 Existing plugin 0.8.0 hooks afterCommitChanges and uses working.gpkg + three-way preview; this cannot itself guarantee server-first native save. Provider/interception spike required; fallback explicit managed save is a separately labeled capability.
- D03 Existing integration SSE authorizes connection at entry; add periodic live revalidation and revoke signal. Event payloads should be minimal and delta reads current-ACL-filtered.
- D04 Never grant internal PostGIS credentials or bypass R006. Use human OAuth authorization code + PKCE/system browser or verified device flow (provider support to verify); existing scoped service keys remain separately identified automation only.
- D05 Project event ordering cannot use naive sequence allocation alone (commit order may differ); reuse project serialization locks or proven transactional cursor. No external timestamp ordering.
- D06 Avoid arbitrary AI desktop execution. Authenticated opt-in local bridge or outbound authenticated session; no publicly exposed desktop port. Native processing tools must be individually allowlisted and validated.
- D07 ACL changes may remove access to previously cached items; use scoped visibility reset, expiry and best-effort cache cleanup. No promise to delete copies outside managed client.
- D08 Initial P1 uses R006 revision pinning; feature-level concurrency may be optimized after correctness baseline. No CRDT for polygon geometry or unconditional last-write-wins.
