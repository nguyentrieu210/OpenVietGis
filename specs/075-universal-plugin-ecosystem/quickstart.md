# Developer quickstart and validation runbook
1. Read constitution, spec, plan, tasks, catalog, acceptance and contracts. Checkout isolated branch from current approved baseline. Inventory actual build/test commands and existing registry/SDK; document in F001, do not invent scripts.
2. Run existing baseline smoke tests, record SHA; map existing house numbering and QGIS plugin identities. Review schema and manifest compatibility.
3. Create synthetic two-org/two-project/three-role fixture; register publisher; generate signed sample using canonical SDK; publish through review; install and activate; execute allowed command; deny cross-project; revoke/disable/export; verify audit.
4. Build each assigned plugin: use PLG A–D tasks and oracle, real implementation not stubs. Store fixture hash and expected values before evaluation. Record failures and prerequisites.
5. After scope coding complete run consolidated tests/CI, then integration against merged candidate; report exact commit, logs, migrations, performance, platform/device profile and reviewer. No full CI per small commit; no production mutation in test.
6. Staging backup/restore and rollback drill, signed artifact and release matrix; production deploy only after explicit authorization and passing relevant gates.
