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

## W3 workbench
| Gate | Status |
|---|---|
| Dockview React/core 8.10.0 exact installed lock, MIT notices | PASS candidate |
| Tab/split/float/redock, resize, scoped storage, malformed/unknown/popout fallback | PASS isolated browser |
| Canvas state and mount ID during tested moves | PASS synthetic only |
| Reset default recovery and host veto contract | recovery PASS; host integration pending |
| Native window IPC, real renderer lifecycle, keyboard DnD, performance and integrated rollback | pending T301c |

## W4 GIS/CAD components
| Gate | Status |
|---|---|
| Group visibility, filtering, selection, disabled layer controls, legend | PASS isolated |
| Host CRS selection, axis/range validation, absolute/relative input, snap capability | PASS isolated |
| Symbol/ramp preview, typed property updates and revision reset | PASS isolated |
| TypeScript/Vite and regression suite 18/18 | PASS |
| Real map/CAD host wiring, renderer and print parity | pending T401c |

## W5–W11 evidence
| Gate | Status |
|---|---|
| W5 100k synthetic grid, <60 mounted rows at sampled positions, edit/select/filter | PASS focused |
| W6 Flutter component and widget test source | implemented, unexecuted |
| W7 read-only UI presentation and invalid binding guards | PASS focused; actual SPEC075 session integration pending |
| W8 command collision/capability/palette and W9 preferences | PASS focused; app integration pending |
| W10 frozen pnpm install, Web build and legacy 312 Vitest + 53 Node tests | PASS |
| W10 six viewport screenshot captures, no document overflow, modal CRUD smoke | PASS fixture; no full image/renderer comparison |
| W11 latest isolated combined test run | interrupted/timeouts; focused rerun 3/3 PASS; final consolidated run pending |
| W11 native builds, full production integration, renderer/device benchmark/security | pending |

## Follow-up integration qualification
| Gate | Status |
|---|---|
| Web account appearance persistence, shared CRS and modal/chat stacking | PASS on isolated real application with mocked server |
| Existing attribute table uses TanStack window; bounded rows and selection after scroll | PASS fixture |
| Dockview real MapLibre camera + feature state on same mounted renderer | PASS isolated fixture, not full Studio |
| Integrated pnpm UI suite 21/21; Web 314 Vitest + 53 Node | PASS local (plugin presentation refactor separately focused 2/2) |
| Mobile host integration PR #120 | code committed, native qualification pending |
| SPEC075 host integration PR #121 | build, frozen lock, client SDK 100/100 and lifecycle 2/2 PASS; E2E/CI pending |
| Full app migration, Cesium/CAD/native windows, remote CI and release | pending |
