# Spec 033 — Core architecture standardization
Parent: R001/R014/R015; depends on Spec 030/031/032.
Objective: reduce cross-layer coupling without deleting capabilities or changing security semantics.
- Canonical UI is web-next. Route all legacy runtime imports through one explicit lazy compatibility bridge; retain old shell until parity evidence.
- Move HTTP-specific official module discovery into HTTP adapters; domain must not import transport/persistence frameworks.
- Move shared snap and projected survey calculations into spatial-workspace public exports; preserve compatibility exports and behavior.
- Keep module discovery generic and isolated; no hard-coded vertical dependencies in Core.
- Enforce ownership via architecture checks and positive/negative fixtures in existing CI validation.
- No DB migration, API contract change, production deployment or destructive retirement.
Acceptance: source boundaries pass; both frontends typecheck; canonical build, spatial math, backend contracts and mocked workbench browser flows pass. Full real-data/browser acceptance and complete legacy retirement remain independent gates.
