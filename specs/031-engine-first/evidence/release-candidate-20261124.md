# Candidate stabilization checkpoint — 2026-11-24

Inputs: main/live e2df5c660ee86b4f59c2b3193865c1938895a835;
R031 8cd5e452a8947d27bdc34a815ada086d4da41c31;
R048 1ffaf62adc2b1ec2362b49cd2371131f26cf990e.
R048 merged without conflicts. The only failure of both backend/validate
integration jobs on 6f78a1e was the obsolete assertion expecting 404 for
authorized retained records after legacy disable. Other lifecycle and
portability tests require authorized Core read of retained data. Reconcile
the assertion; additionally verify disabled mutation fails and foreign tenant
read is denied. No weakening of runtime authorization.

Targeted validation on this candidate plus the lifecycle test repair:
- TypeScript, Ruff, mypy 187 files, scope and architecture passed.
- 15/15 real PostGIS tests (lifecycle/security and R048 CAD fixture/link/export).
- Built default Web frontend successfully.
- R048 isolated browser: reviewed download, explicit consent, checksum,
  stale revision and revoke/abort passed; 8 requests.
- Advanced Editor browser: create/bulk/vertex/sketch/committed undo-redo passed.
- Cesium browser: 1411 visible native tile pixels, scoped resources,
  compatibility switch and permission revocation passed.
- Prior 8cd5e45 source: frozen install and 47/47 spatial workspace tests.

Release coordination: stop expanding feature scope in this candidate.
Use R031 as the sole integration branch for remaining defects and acceptance.
Treat this as a coordination rule, not an enforced repository lock.
Pin every CI/evidence run to the candidate commit and compare current main
and live provenance again before promotion. Preserve new independent work
without automatically moving the candidate while acceptance is underway.
No release certification, agent termination, branch deletion, main merge
or deployment performed. Full exact-commit CI and independent QGIS network
and release gate acceptance remain open.
