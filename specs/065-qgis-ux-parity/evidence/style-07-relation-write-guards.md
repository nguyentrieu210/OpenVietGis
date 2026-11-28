# STYLE-07 relation write guard verification

Status: partial. This change adds a targeted integration test; the test has not yet been executed in CI, so it is not acceptance evidence.

Migration `0049_relation_constraints.py` already creates deferred constraint triggers for writes to `spatial.reference_features`. The added test creates a valid project relation through the authenticated API, then attempts writes as the restricted runtime database role outside the changeset endpoint:

- delete a referenced target row;
- change a local key to a missing target;
- duplicate a target key when cardinality is `one`.

Each write must fail at transaction commit and roll back; the test also confirms the valid relation remains readable. This covers database enforcement independent of the canonical changeset implementation.

Run: `uv run --project apps/api pytest apps/api/tests/integration/spatial/test_relation_constraints.py -q`

STYLE-07 remains partial: relation form configuration/layout and paired QGIS/browser acceptance are still open.
