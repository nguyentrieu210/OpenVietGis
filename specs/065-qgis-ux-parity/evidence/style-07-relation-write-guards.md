# STYLE-07 relation write guard verification

Status: partial. The targeted restricted-role PostGIS integration test passes in CI. STYLE-07 remains partial because personal relation configuration, shared form layout, and paired QGIS/browser acceptance are still open.

Migration `0049_relation_constraints.py` installs deferred constraint triggers for writes to `spatial.reference_features`. Under the restricted runtime role, the test attempts direct SQL writes outside the changeset endpoint:

- delete a referenced target row;
- change a local key to a missing target;
- duplicate a target key when cardinality is `one`.

Each write must fail at transaction commit with SQLSTATE OV409 and the relation-constraint diagnostic, then roll back. The test also confirms the relation definition remains stored. All 3,000 local polygon keys are aligned to unique target point keys before fixture setup.

The valid relation definition is seeded with admin SQL because relation management is served by the Rust API, which is not registered in this FastAPI integration app. The tested feature writes still run as the restricted runtime role. Migration 0049 also pins the GIS ACL predicate to `pg_catalog` and qualifies PostGIS calls in `public`, so it remains resolvable when invoked through the relation trigger's restricted search path.

Verification: `.github/workflows/e06-e08-retention.yml` → `retained-postgis-contracts` passed for commit `77af5af45e94619545d5f783704a7756fa46c858` (run [46148890322](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46148890322)).
