# SPEC 066 acceptance checkpoint (2026-11-28)

Candidate: `9ebe5b9537e7fbc3a7a1c5c19cf6b500ab51a3fe`, PR #82, no live deployment.

Observed GitHub Actions evidence for this exact commit:
- [Root CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243045): 1,316 Python tests passed; contracts OK; scope gate rejected the E15 format adapter path.
- [Rust Platform CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243061): Cargo Audit, format, Clippy, Rust tests and architecture isolation passed. Core HTTP differential parity 288 passed / 0 failed. E15 full parity 378 passed. The isolated tile-cache test stopped at the layer lifecycle CHECK because the retirement fixture did not advance updated_at.
- [E11 Outbox](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243108), [E05 benchmark](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243099), [E17 web parity](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243103), [E18 operations](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243050), [E16 MCP adapter](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243075), [Rust MCP gateway](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243060), [E06/E08](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243112), [QGIS connector](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243105), [E15 module](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46127243053): all succeeded.

Corrective change: recognize only the exact feature-gated E15 codec source as a vertical owner in the scope checker. Do not relax scanning of any other file or marker. The codec is an embedded byte-format engine without database or HTTP responsibility. Repair the synthetic lifecycle fixture to satisfy the actual database constraint; preserve denial assertions and use the established session expiry fixture.

Do not mark the remaining tasks accepted on these partial results. The corrected candidate requires fresh checks; performance qualification needs raw matching baseline/soak evidence; independent review, ADR acceptance and immutable digest release with health/rollback evidence remain required. No production data or deploy-only host was modified.
