# Eleventh implementation slice
Base 1efc76f6077a5701a9457768f65149f1bd89c57b.

- PROC-01/02: vector workbench filters the real Rust registry to supported Buffer/Clip/Dissolve contracts, checks project write permission, requires explicit ready authorized input, restricts Clip to distinct polygon source, validates group field against schema, shows input CRS/revision. No new spatial algorithm or preview.
- PROC-04: account/org/project-scoped sessionStorage preserves an immutable pending request across closing the panel and reload in the same tab. Validate response operation ID, algorithm, name and status. Ambiguous error retains same request; no new operation is issued automatically. History includes job ID, attempt, output CRS and error; queued job can be explicitly executed via existing retry route.
- TypeScript PASS; 78 Vitest + 36 Node tests PASS; production build and targeted Chromium exit 0. Synthetic intercepted 503 then reopen workbench and resend exact payload.
- Server still pins actual input revision at submission, not client-selected revision; no new precondition. Session storage is required for safe submit; ambiguous unreconciled jobs may remain blocked. No real queue/geometry or paired QGIS acceptance.
