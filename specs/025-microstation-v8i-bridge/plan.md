# Implementation Plan: R025 — MicroStation V8i Bridge

**Branch:** `feature/025-microstation-v8i-bridge` | **Date:** 2026-11-23 | **Spec:** [spec.md](spec.md)

## Summary

Add a Windows-local, outbound-only bridge and source-CRS-aware exchange contract. Keep CAD-specific code outside platform Core, reuse project boundaries and future authorized API integration, and explicitly gate Windows V8i acceptance.

## Technical context

- **Bridge:** Python 3.10+ (stdlib validation, optional `pywin32` only for COM), Windows runtime for V8i.
- **Server:** existing FastAPI + PostGIS, R010 External Sources (read-only) and partner service credentials (current viewer scope) as reference, not an implicit write privilege.
- **Wire:** strict JSON snapshots, idempotency key/digest, explicit CRS, typed `Point`/`LineString`/`Polygon`. No implicit WGS84.
- **Transport (future):** outbound HTTPS + device scoped token; server must bind project context from verified credentials rather than device-provided IDs.
- **Testing:** Python unittest / pytest on Linux fixture; real Windows COM/V8i smoke and database authorization integration as later gates.
- **Deployment:** opt-in Windows Bridge, separate from Linux API. No DCOM inbound, no Bentley redistribution.

## Constitution check

- Platform/module isolation: PASS — stand-alone adapter, no cadastral special case in Core.
- Interoperability: PASS — versioned open JSON wire format, no proprietary runtime dependency for fixture mode.
- Tenant isolation: BLOCKED until P2 server auth and RLS implemented and tested; no production ingress now.
- CRS correctness: PASS contract rejects unspecified CRS; **transform correctness unverified** until test control points.
- Audit/history: no server writes in P1; P2 requires changeset + audited approvals.
- Production readiness: BLOCKED until Windows V8i compatibility and security review.

## Design decisions

1. Choose Windows-local COM automation for V8i **probe** and initial read adapter, rather than requiring ODA/GDAL DGN V8 support on VPS.
2. **Do not** reuse R010 External Sources as a mutable CAD source; R010 is allowlisted HTTP external read-only.
3. Separate the COM extraction (untrusted/nonportable) from the exchange validator and diff engine (portable/testable).
4. Unsupported CAD types and ambiguous CRS are hard stops for claiming cadastral completeness.
5. Event-driven incremental sync and write-back remain separate features; begin with explicit batch preview.

## Project structure

```text
specs/025-microstation-v8i-bridge/{spec.md,plan.md,research.md,data-model.md,tasks.md,quickstart.md,contracts/}
integrations/microstation-v8i-bridge/{README.md,bridge.py,bridge_contract.py,windows_com.py,fixtures/,tests/}
```

## Validation and rollback

1. Run fixture tests; demonstrate explicit CRS validation and stable diff with **deletion candidates only**.
2. On Windows with V8i, run safe COM probe and record version/bitness compatibility. Read extraction is unverified until CAD fixtures prove it.
3. Before P2, add scoped service endpoint, logs, per-device revocation, tests using real PostGIS and browser pairing UX.
4. Rollback P1 by removing standalone adapter files; no DB migrations or live user data touched.
5. Any future two-way writes require staged backups and revalidation with V8i source.
