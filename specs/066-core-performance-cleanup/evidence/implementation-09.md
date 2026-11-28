# 066 implementation slice 09 — reconcile active 065 UX
Parent candidate: f39845df16231e868c3092c57eb4f68e4e525804.
- Pinned active UX branch: 065-qgis-ux-parity at 1efc76f6077a5701a9457768f65149f1bd89c57b; merge base main 5bfc83826161acd83b98a925e85690cebde53ae1.
- Imported the complete 065 tree into 066. Only LiveApp.tsx conflicted. Resolution preserves 066 direct CAD resume, lazy GIS initialization, scoped authorization/cache invalidation and cross-tab revocation while retaining 065 QGIS selection epoch/toggle, scoped UI preferences, server-side attribute queries, locator and keyboard/UI changes. CoreMapTools merged automatically.
- Canonical SessionEntry remains the sole identity/session entry; 065's older embedded LiveApp auth bootstrap is not reintroduced.
- Validation after resolution: TypeScript PASS; web-next 20 Vitest files / 100 tests PASS; 39 Node tests PASS; production build PASS. tools/check_qgis_parity.py reports 71 contract rows not accepted because the 065 matrix itself is still evidence-gated; this is an upstream 065 status, not a runtime failure.
- T001 coordination is materially complete for the pinned 065 SHA, but 065 itself still has 101 unchecked evidence tasks. 066 must not claim 065 final acceptance or overwrite that branch. No live deployment.
