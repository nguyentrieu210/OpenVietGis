# STYLE-07 relation write guard verification

Status: partial. The test is in the PR-triggered PostGIS integration job. Two initial attempts exposed test-fixture defects; the current corrected version is pending CI and is not acceptance evidence until it passes.

Migration `0049_relation_constraints.py` already creates deferred constraint triggers for writes to `spatial.reference_features`. The test seeds a valid project relation definition in the owner fixture setup because relation management is served by the Rust API, while this integration app is FastAPI-only. It then attempts direct writes as the restricted runtime database role, outside the changeset endpoint:

- delete a referenced target row;
- change a local key to a missing target;
- duplicate a target key when cardinality is `one`.

Each write must fail at transaction commit with SQLSTATE OV409 and the relation-constraint diagnostic, then roll back; the test also confirms the relation definition remains stored. The fixture aligns all 3,000 local polygon keys with unique target point keys before creating the rule. This covers database enforcement independent of the canonical changeset implementation.

Run: `.github/workflows/e06-e08-retention.yml` → retained-postgis-contracts job (includes this integration test).

STYLE-07 remains partial: relation form configuration/layout and paired QGIS/browser acceptance are still open.
