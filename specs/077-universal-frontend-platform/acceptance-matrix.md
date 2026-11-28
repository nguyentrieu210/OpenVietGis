# Acceptance matrix
| Gate | Evidence | W0 status |
|---|---|---|
| A01 source inventory | pinned tree SHAs, all 50 CSS paths counted by JSON, no truncation | observed; exact count in baseline.json |
| A02 reproducibility | node tools/audit_frontend_077.mjs --verify; optional --all after fetching pinned refs; node --test tools/tests/audit_frontend_077.test.mjs | PASS: 3/3 unit tests; 6/6 pinned scope trees; 0 differences |
| A03 existing behavior | screenshots 6 viewport widths; map/3D/CAD, keyboard, edit/undo, auth and permissions | pending |
| A04 performance baseline | same fixtures/browser/device, cold/warm build size, frame time, input p95, 100k table | pending |
| A05 candidate adoption | license + version + peer dependencies + API/IPC and Flutter spike | W1/W2/W3/W6 pending |
| A06 plugin boundary | schema validation, unknown component/capability denial, tenant isolation, v1 compatibility | W7 pending |
| A07 release | integrated CI, native packages, rollback; approved deploy | W11 pending |
Source checks alone are not runtime acceptance. Missing external branch objects fail --all; default --verify checks main baseline only. Baseline counts are scoped tracked blobs, not repository-wide metrics.

## W1 token acceptance
| Gate | Status |
|---|---|
| 59 typed token values, light/dark and three density modes, contrast/touch/reduced-motion validation | PASS |
| Deterministic generated CSS/Tailwind/Dart, checked artifact drift | PASS |
| SPEC072 aliases and isolated Chromium computed style, 8 checks | PASS |
| Production Tailwind compile and integrated screenshots | pending W0/T101c |
| Flutter analyze/test and device theme integration | pending T101c |

## W2 React foundation
| Gate | Status |
|---|---|
| Base UI 1.9.0 / React 19.3.1 exact pins, isolated npm lock, runtime MIT notices | PASS candidate |
| TypeScript and Vite production demo build | PASS |
| Playwright Chromium 9/9: dialog focus, controls, dark portal, six viewports | PASS |
| Root pnpm integration, existing app facade switch, map interactions, final visual/accessibility and distribution review | pending T201c |
