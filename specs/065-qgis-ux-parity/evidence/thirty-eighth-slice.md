# Thirty-eighth implementation slice (GitHub only)

- PROC-09: client now cross-checks the declared GDAL affine matrix, nondegeneracy, geographic footprint and per-GCP native/pixel residuals against submitted points. Added WGS84 and Web Mercator test cases, plus deliberately mismatched footprint/residual tests.
- TABLE-02/03/06/08: shared query response verification rejects missing, oversized or non-string FID, malformed attribute records, invalid fingerprints and sort direction before population/selection/calculation. Added regression cases.
- Source inspection only for STYLE-01, TABLE-06/08, COLLAB-02/04/06/09; manifest now records partial coverage and residual acceptance gaps. No new functionality claimed for these inspected rows beyond the shared query guard.
- Source commits: `0c1483098faf1d886894fb37ebe48c8524efdeeb`, `4a119474d295474ce946008abbab3ebc105db56c`, `c6f0b9806689e09242878d8dea891e6c4bba117a`, `4b740ce4b6b987db390b1257487e5f06b5a8302b`.
- Tests added, not executed; no build, QGIS comparison, merge or live deployment. No acceptance tasks closed.
