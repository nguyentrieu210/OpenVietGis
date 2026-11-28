# Acceptance matrix
| Gate | Evidence | W0 status |
|---|---|---|
| A01 source inventory | pinned tree SHAs, all 50 CSS paths counted by JSON, no truncation | observed; exact count in baseline.json |
| A02 reproducibility | node tools/audit_frontend_077.mjs --verify; optional --all after fetching pinned refs; node --test tools/tests/audit_frontend_077.test.mjs | execution pending |
| A03 existing behavior | screenshots 6 viewport widths; map/3D/CAD, keyboard, edit/undo, auth and permissions | pending |
| A04 performance baseline | same fixtures/browser/device, cold/warm build size, frame time, input p95, 100k table | pending |
| A05 candidate adoption | license + version + peer dependencies + API/IPC and Flutter spike | W1/W2/W3/W6 pending |
| A06 plugin boundary | schema validation, unknown component/capability denial, tenant isolation, v1 compatibility | W7 pending |
| A07 release | integrated CI, native packages, rollback; approved deploy | W11 pending |
Source checks alone are not runtime acceptance. Missing external branch objects fail --all; default --verify checks main baseline only. Baseline counts are scoped tracked blobs, not repository-wide metrics.
