# 066 implementation slice 14 — large-map viewport/LOD qualification and bounded transfer contracts

Parent candidate: 1862bc9fa006f0949b8c3a59d1332e2bcb4b1a26.

## T029 — viewport BBOX, LOD and incremental rendering
- The client uses professional vector tiles for authorized spatial layers above 1,000 features and auto-enters performance mode when an eligible layer exceeds 50,000 features. In performance mode whole-layer feature paging is skipped; attributes/geometry are fetched only for selected tile features using pinned layer/source revisions.
- The Rust viewport contract caps JSON reads at 1,000 features and validates finite WGS84 BBOX, revision pins, source/project scope and response CRS. Native MVT caps a tile at 10,000 ordered features, 200,000 input positions and 4 MiB, marks truncation, and remains private/no-store.
- 10k browser evidence: 100 measured map-open and 100 selection trials on the pinned 10,000-feature Core profile; p95 open 2887.685 ms against the 3000 ms budget and p95 selection 390.639 ms against the 1000 ms budget, with external network blocked.
- 30k/100k client render-diff evidence: candidate p95 109.398/383.521 ms versus baseline 170.668/679.757 ms with identical delta output. The candidate reuses source geometry references and only regenerates scalar render properties.
- Scale-specific incremental benchmark now runs 10k/100k/1m arrays with one selection change and explicit source-geometry identity checks. Before the ordered-identity fast path, p95 was 171.10/1318.14/16546.82 ms; after it, p95 is 21.08/525.32/1955.32 ms. Each scale produces exactly one update and reports source_geometry_unchanged=true. The fast path validates unique defined IDs once per array and falls back to the existing Map diff when order changes; duplicate IDs fail closed to full reload.
- Local attribute-table pagination is explicitly capped at 100 DOM rows; a 1,000,000-record unit fixture verifies first/last valid pages and rejects negative pages. Server-side table mode retains its separate bounded query path.
- 30k/100k/1m PostGIS BBOX evidence: five EXPLAIN ANALYZE samples per scale. Full-scope returned 121 rows at every scale with the same ordered ID SHA-256; restricted scope returned 41 with the same restricted ordered hash. Existing GiST eligibility reduced qualified narrow-BBOX medians to 6.209/1.778/1.659 ms for 30k/100k/1m full scope in the isolated fixture. The rejected alternative SQL did not replace the safe path.
- Geometry correctness is separately covered by exact WKB/EWKB child-order/SRID/round-trip/non-finite tests and the differential geometry gate (T033). No source geometry mutation, source rewrite or production DB change is introduced here.
- This closes T029 for the specified 10k/100k/1m scale classes. It does not claim production CWV, GPU memory or final soak; those remain T044.

## T028 — bounded transfer payloads, streaming and cancellation
- Export reads use keyset paging in batches of 1,000 and flush staged export data at 64 KiB. Download bodies stream in exact 64 KiB chunks instead of materializing the complete artifact in memory.
- Uploads are capped at 25 MiB and transfer feature count at 50,000. The existing authorized cancel endpoint now shares a pure terminal-safe cancellation transition: succeeded/failed/denied/cancelled jobs are immutable; queued/active/recoverable_failed jobs transition to cancelled with a completion timestamp and actor-cancel code.
- Focused Rust 1.85.1 no-default-features tests PASS with exit 0: cancellation + explicit transfer bounds 2/2; bounded file stream 1/1. The stream fixture writes CHUNK*2+17 bytes, asserts exact chunk sizes [64 KiB, 64 KiB, 17], and verifies byte-for-byte concatenation.
- Slow download bodies are created only after the scoped project DB transaction/lock has been released; cache headers remain private/no-store.
- This closes T028. Format/parity correctness remains independently covered by T033 and the retained transfer boundary suites.
