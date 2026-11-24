# Spec 037 — Unified GIS/CAD/3D workspace
Parent R004/R006/R015; depends on Spec 033–036.
- A project with verified unrestricted project.read may open CAD or 3D with a business plugin active, without a forced Core switch. Preserve restricted-account denial and server checks.
- Permission readiness is distinct from loading map features; only a configured 3D scene needs the map camera. Empty 3D and CAD vault do not require configured geometry.
- Keep GIS canvas mounted; use ModeCoordinator to prepare, activate and dispose adapters. Stale or cancelled candidates must not clear unrelated active engine state or publish stale selection.
- Preserve existing dirty/saving blockers, version receipts, linked feature pins, plugin filters and scope isolation.
- Verify browser transitions, empty and configured 3D, CAD upload/version actions, in-flight save blocking, revocation and plugin-active mode navigation.
Non-goals: no production deployment, new CAD engine, or claim of complete native DWG editing/real-data rendering acceptance.
