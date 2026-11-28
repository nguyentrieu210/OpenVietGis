# 066 implementation slice 21 — bounded MCP results/cancellation and QGIS managed-sync guards

Parent candidate: e10a729ff85e045664c7815ae7ebda30b76e05b8.

## T036 — bounded tool results and QGIS synchronization
- MCP Python and Rust both reject command/result payloads above 85,000 serialized bytes. Focused tests cover an oversized workspace result in both implementations.
- Python synchronous MCP execution clears an unfinished browser command if the caller coroutine is cancelled. Rust Store cancellation removes the pending command, while the HTTP candidate wraps short operations in a Drop cancellation guard.
- QGIS managed connector mirrors server limits client-side before network/mutation: bootstrap 1..5000 rows/page, delta 1..500 events/page, commit/diff at most 1000 operations, at most 100 managed layers per load and a 1,000,000-feature aggregate snapshot ceiling.
- Managed HTTP JSON responses are read with a hard 32 MiB cap rather than unbounded response reads. Oversized responses fail closed before JSON parsing.
- Snapshot loading checks cancellation before every page. New contract coverage verifies cancellation after the first page prevents a second request.
- Cached reconnect remains revision/permission-epoch pinned. Delta catch-up is bounded to eight pages and falls back to a full authorized snapshot if cursor, revision, ACL epoch or history continuity cannot be proven.
- QGIS contract suite after the changes: 45/45 PASS. It covers page/cursor envelopes, cross-page revision/schema/CRS consistency, cache revalidation, receipt-before-replay, native save/delete behavior and the new bounds.
- Full retained MCP suite: 48/48 PASS. Rust MCP: 9/9 PASS on pinned Rust 1.85.1 offline container. Ruff/Python compile/git diff checks PASS.
- PyQGIS is not installed on this VPS, so no new GUI runtime smoke is claimed in this slice. The headless PyQGIS smoke remains in tools/smoke_qgis_native.py for a QGIS-capable release runner; final platform regression is part of T045.

This closes T036 without changing production sessions or data.
