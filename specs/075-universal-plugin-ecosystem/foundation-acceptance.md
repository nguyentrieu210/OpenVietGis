# Wave 0 foundation acceptance — candidate after slice 18
Parent dac99e49dc38ca09170eaae585d938918fc3d8d5; candidate is the commit containing this file. This is a component-level evidence index, not independent approval or whole-Wave sign-off. No task is closed solely on unit tests.

| Gate | Implemented evidence | Remaining acceptance / blocker |
|---|---|---|
| F001 | Existing registry, manifests and API/storage route families inventoried; native tips recorded in cross-platform contract | Independent exact integrated inventory including all native and existing domain mappings |
| F002 | Canonical signed fixture, schemas and shared invocation vectors; V1 contract preserved | Independent ADR and cross-host contract qualification |
| F003 | Signed release membership, root policy, durable checkpoint, key rotation, revocation, full member integrity before filtering | Recovery/root replacement procedure, full integrated catalog/rotation and deployment test |
| F004 | Bounded solver, stateless graph operations, CAS, grant and graph epoch floors | Actual graph DB switch/rollback test, lifecycle integration |
| F005 | Scoped declarative broker, permissions, receipts, timeout, processing proposal | Authenticated end-to-end scope matrix, attempt/concurrency budgets, cancellation/cleanup |
| F006 | Web host, native adapter candidates and shared wire vectors | Dart/Flutter and Desktop managed transport qualification, real-device and accessibility acceptance |
| F007 | Rust/TS/Python signing tools, reference fixtures, Dart candidate | Full generated SDK parity and timed sample publish/install on two hosts |
| F008 | Developer submission/review/export and rotation | Release publication integration, channels, private catalog, full browser workflow |
| F009 | Sandbox ledger, signed offer acceptance, reconciliation, certificates, central floors | Provider webhook/outbox, renewal and settlement lifecycle, offline device lease issuance |
| F010 | Append-only RLS storage, scoped receipt export and graph receipts | Full migration chain on integrated DB, backup/restore, data-preserving uninstall/export |
| F011 | Four signed reference documents; House Numbering 45 unit tests and QGIS stdlib regression | Two installed reference workflows through auth, database, actual host; native QGIS bilateral sync |
| F012 | Component tests and reproducible resolver load | Exact merged candidate, reference HTTP load, full regression, independent security/supply-chain review and rollback |

## SC-005 load evidence
Runner: shared Debian development VPS, reported 6 logical CPUs and 11906 MiB total memory, not the stipulated isolated 4 vCPU/8 GiB reference. Rust debug test, 1,000 synthetic pins (100 keys, ten patch versions each), one fixed root with two dependencies, 100 requests distributed over four threads. Each timed call rebuilds the dependency index. No warmup or HTTP, DB, signing, serialization, or activation overhead.
Observed p50 4.295 ms, p95 5.502 ms, p99 12.347 ms, max 12.382 ms, 0/100 errors. Test checks exact deterministic lock and rejects more than 1024 candidates. Reproduce with cargo test --locked -p ovg-module-sdk --test spec075_load -- --nocapture.
SC-005 remains OPEN: catalog V2 accepts at most 100 releases; existing V1 validator accepts 500. Signing tools have matching V2 bound. Increasing limits alone cannot qualify throughput, memory, pagination or signature verification. Current API validates all release signatures and contents, including incompatible releases, before admission. V1 lookup is indexed by publisher/key/version. Four concurrent verification slots reject overload; no 100-concurrent-reader SLA has been demonstrated. A bounded verified snapshot/search design with checkpoint invalidation and a real reference runner is needed.

## Current component checks
Rust module SDK 21/21, API 115/115 (no-default-features, non-root execution), HTTP gates 4/4, synthetic resolver load 1/1. Prior slice 17: House Numbering 45/45, storage lib 59/59, QGIS stdlib regression 4/4. Prior slice 16: TS 15/15, SDK/Web typecheck, four signature vectors. These are not a single integrated CI run. Previous isolated DB tests remain scoped to their respective migration fixtures.
