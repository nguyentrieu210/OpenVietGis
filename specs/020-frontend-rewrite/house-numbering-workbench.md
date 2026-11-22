# House Numbering workbench integration — 2026-11-23

The activated House Numbering module contributes a "Nghiệp vụ số nhà" toolbar entry to the shared GIS Studio. The modal keeps the map mounted and exposes existing API-backed panels: registry, bulk numbering, proposals, reconciliation, survey/progress, documents/plates, and search/handover. Registry supports map handoff without depending on a Router in the new Studio host.

Verification on VPS:
- TypeScript typecheck and isolated Vite production build: PASS.
- Playwright `apps/web-next/house-numbering-workbench-smoke.mjs`: all seven tabs render, no uncaught page errors; shared map remains mounted, plugin chunk and domain API requests observed. Mocked fixture API, not production data.
- Existing scoped GIS authorization remains fail-closed; restricted accounts do not get access to unadapted House Numbering business endpoints. A dedicated authorization adapter and real API end-to-end acceptance are still needed.
- Canonical frontend not overwritten by this change: concurrent Core frontend edits were observed after the isolated build. Include the workbench in the next verified integrated frontend deployment.
