# 066 implementation slice 17 — bounded blocking/CPU execution qualification

Parent candidate: 1c0c67d7d7c932615f15c5bcc505acd00c39a533.

## T026 — allocations, blocking Tokio work and bounded Rayon/jobs
- A commit-pinned static inventory covers Rust sources under apps/api-rust/src and crates. It records all spawn_blocking, subprocess, semaphore and Rayon sites rather than assuming async code is non-blocking.
- Dedicated processing execution is bounded by ExecutorConfig defaults: max 2 concurrent jobs, max queue depth 16, 2 Rayon worker threads and 60-second default timeout. The executor raises 429 on queue overflow, uses a Tokio semaphore before Rayon submission and supports cooperative cancellation.
- Exact pinned-toolchain cargo test -p ovg-processing PASS: 8/8. Focused executor tests prove strict peak concurrency <= configured bound, queued cancellation, hard queue admission and timeout failure.
- Heavy format/engine subprocess boundaries are separately admitted: CAD 4, transfer 4, field package 2, provider 4 and raster compute 1 concurrent worker(s), each with an explicit timeout. Collaboration and managed-sync listeners are separately bounded at 40 and are not CPU worker pools.
- The prior WKB allocation benchmark provides comparable before/after CPU evidence for a concrete hot path. Encoding 90k-coordinate MultiLineString p95 improved 0.754292 -> 0.511459 ms; MultiPolygon p95 0.645500 -> 0.388243 ms. Candidate output byte lengths and fixture checksums match baseline exactly, and round-trip/non-finite correctness tests already PASS.
- The three spawn_blocking sites are finite request-scoped operations (cartography render preparation, artifact probe and avatar normalization); this slice does not increase their concurrency or create a new unbounded executor.
- This closes T026's qualification requirement. It is not a claim that every CPU path has zero allocations; final system-wide soak/RSS comparison remains T044.

No production service or executor configuration was changed in this slice.
