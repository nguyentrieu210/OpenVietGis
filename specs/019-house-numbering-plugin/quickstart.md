# Validation guide — planned acceptance
## Current executable subset
From isolated branch, pure rules only:
`python3 -m unittest discover -s tests/modules -p 'test_house_numbering_domain.py' -v`
No production credentials or personal data.
## Integration prerequisites
Verified/published Core contracts R002–R014, R015 backup and artifact recovery, R018 gaps resolved, exact commit and test database isolated from VPS production. Existing deployment remains untouched.
## Scenarios
1. Enable module in two projects; third remains disabled. Assert isolation, role revocation, module absent, Land Asset regression, disable/re-enable without loss (SC-001).
2. Draw route and houses (100,102,106; alley 155) with verified CRS; click/inspect, preview and apply, stale revision fails; approved locks and duplicate controls (SC-002/003).
3. Stage CAD/GIS/Excel fixture of known CRS with known source counts, conflict and replay; confirm three-source groups and unmatched separately (SC-004/005).
4. Offline phone 50 synthetic surveys with photos, close/restart/reconnect; verify no loss/duplicate, delegated permission and progress (SC-006/007).
5. Export inspection Word and approved-certificate PDF, missing image warnings, reissue, batch 100; verify owner audit and no PII in ordinary outputs (SC-007/008).
6. Export scoped DXF, GeoPackage and Excel/CSV; reopen independently, measure 100 control points and attribute counts. DWG only if capability verified (SC-009).
7. Measure 10k/100k fixture, 50 concurrent users with configuration and p95, observe new user search. Run browser workflow and isolated restore, including artifacts. Capture exact candidate, logs and negatives, do not mark pass on placeholders (SC-010/011).
