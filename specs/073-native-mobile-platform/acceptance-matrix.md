# Release profiles and acceptance traceability v2
Status: planned. A0 feasibility -> A1 standalone -> A2 connected -> A3 plugins -> A4 final Android; iOS I1 separate. All 78 umbrella tasks and execution leaves must map to evidence; no platform-wide acceptance inferred from a single gate. 
## Profiles
Android Local (A1): offline after local file/archive available, map-first even fresh install without network (empty canvas and import/download affordance, no fake basemap), GIS import/edit/measure/CRS/export/GPS/track and local search. Android Connected (A2): scoped auth, packages, edits, receipts/conflict/revocation. Android Business (A3): signed declarative plugin, reference house numbering + generic plugin. Android 1.0 (A4): A1–A3, qualified processing and agreed mandatory features; advanced raster/RTK separately capability-qualified, no COPC/3D/wgpu/DataFusion promise. I1: iOS build, real device, policy and all advertised profile gates separately. Android launch never blocked by unavailable iOS runner. Commerce paid distribution only after legal/store/entitlement review, not checkout/payout scope.
## FR to gate/test families
| FR | SC | Gate | Test evidence |
|---|---|---|---|
| 001,002,027 | 001,009 | A1/A4 | shell, first-run offline, local/managed isolation, accessibility |
| 003,029 | 010 | A0/A4 | cargo graph, ABI, server regression |
| 004,021 | 002,003 | A1/A4 | device render/viewport/snap/copy/resource benchmarks |
| 005,006 | 001,004 | A1/A4 | driver roundtrip, archive hostile fixtures, raster block |
| 007,008,009 | 001,005 | A1 | CRS oracle, edit/undo, validation and attributes |
| 010,011 | 001,005,009 | A1/A4 | GPS/track, camera/QR, receiver gated real hardware |
| 012,013 | 001,006,007 | A1/A2 | SQLite recovery, tile license, managed encryption |
| 014,015 | 006,007 | A2 | auth negative, sync state/receipt, cross-tenant |
| 016,017,018 | 007,008 | A3 | signature/assets, host broker, reference workflows |
| 019 | 007,008 | A3/A4 | entitlement vs permission, license policy |
| 020 | 005,010 | A4 | local/server processing differential |
| 022 | 003,004 | A4 gated | optional engine adoption/rejection evidence |
| 023,024 | 001,007,010 | A2/A4 | local/provider search distinction, collaboration scope |
| 025 | 006,007,009 | A0–A4 | threat, encryption, revoke, supply chain |
| 026,028,030 | 002,003,008,009,010 | A0–A4/I1 | toolchain, SDK quickstart, benchmark, release |
## Evidence schema
Record task ID, FR/SC IDs, gate, candidate SHA, exact paths, command/test ID, fixture checksum, device/OS/toolchain, expected/actual, PASS/FAIL/BLOCKED, reviewer and limitations. Baseline and thresholds frozen before execution. Physical device evidence required for native performance; independent oracle for correctness. Optional research rejection permitted with reviewed reason, not feature acceptance. No owner waiver implicit. W7 final integrated CI and signed artifact evidence; no live server modification by default.
