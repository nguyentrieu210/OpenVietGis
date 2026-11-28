# Performance qualification — SPEC 074 revision 2
All targets proposed, not measured results. W0 freezes fixture hashes, hardware/OS/driver, exact build SHA, dependency versions and workload scripts. Reference tier: 8 logical CPU threads, 16 GiB RAM, NVMe SSD, integrated GPU, 1920x1080 viewport; Apple Silicon equivalent documented separately. Test each advertised OS on actual hardware; CI smoke is not performance certification.

## Workload
F1: 100k mixed simple vector features, realistic attributes.
F2: 1m vector features, spatial index and fixed viewport cardinality.
F3: 10m vector features in indexed local store, multiple scales; no all-feature rendering claim.
F4: 20 GiB tiled COG/raster dataset; no full raster memory load.
F5: large CAD and COPC fixture only for qualified adapters; preserve IDs/CRS.
F6: 10k edits and reconnect/replay/conflict scenario, with isolated server.
Use seeded reproducible synthetic or licensed public data. Record dataset hash, complexity, visible features, tile byte counts and source CRS.

## Initial gates (subject to W0 evidence and documented revision)
- Cold launch to usable shell p95 <= 2 s on reference SSD; measure app launch separately from first data view.
- Warm viewport F2 indexed query p95 <= 100 ms for bounded result set (record geometry complexity and rows).
- F1 2D pan/zoom target median >= 55 FPS, p95 frame time <= 25 ms; F2 report same metric with declared visible tile workload.
- UI input response p95 <= 50 ms during background import/processing.
- Tile cache <= 256 MiB RAM by default; record total process-tree peak RSS and GPU memory. F3/F4 must not materialize full source or unbounded queues; peak host process-tree RSS target <= 2 GiB at fixed viewport (GPU allocation reported separately).
- Stale viewport cancellation releases queued resources; p95 cancellation observation <= 250 ms; no stale result replaces current revision.
- No whole-layer JSON payload; measure payload bytes/copies, IPC time, native compute time, decode and GPU upload separately.
- F6 correctness gates: zero duplicate committed mutations, zero lost acknowledged edits, explicit conflicts. Measure incremental transfer bytes and latency, not a fixed network-independent speed claim.

## Measurement
Cold/warm runs independently; >=30 measured interactions after warmup per OS/fixture. Report p50/p95/p99, peak RSS for process tree, GPU memory where measurable, CPU, disk/cache, frame pacing, cancellation and data transfer. Record failures and outliers; no fabricated FPS or skipped failure. Compare Web baseline on same fixture, machine and comparable workload (remote/network vs local split disclosed). Compare candidate providers on exact same semantics/output before adopting. If a target fails, fix, narrow explicitly advertised capability or revise target with documented evidence/approval; never mark acceptance solely from tests running.
Native GPU provider is optional; adopt only if profiling demonstrates material rendering bottleneck and measurable improvement without correctness/security regressions.
