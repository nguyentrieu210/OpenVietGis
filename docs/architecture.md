# OpenVietGIS Architecture

OpenVietGIS is a platform-first monorepo. Governance lives in the Constitution, replaceable
technical decisions live in ADRs, feature intent lives in Spec Kit artifacts, and runtime behavior
lives in source code.

## Ownership boundaries

- apps/web-next: the single canonical public/live frontend (/ and /app); production Docker, Playwright and pnpm workspace build this app.
- apps/web: retired by Spec 066 after route/offline/OIDC parity gates. Historical specs and QA reports keep the old path as immutable evidence; see archive/legacy-frontend-history.md.
- `apps/api`: application backend and HTTP adapter.
- `packages/*`: reusable public or cross-application contracts with explicit ownership.
- `modules/*`: vertical domain modules downstream of platform contracts.
- `tools/*`: repository validation/development tools.
- `infra/*`: deployment/development infrastructure definitions.

## Allowed dependency direction

```text
UI -> client/application contracts -> application/domain ports -> infrastructure adapters

vertical module -> public Core/platform contract
Core/platform   -X-> vertical module
```

Core must build and run when `modules/` is empty or absent.

## Artifact roles

- `docs/product.md`: product intent, target workflows, release outcomes, and decisions still due.
- `ROADMAP.md`: stable capability IDs, scope boundaries, dependencies, milestones, feature status,
  and links to the feature specifications; provisional future labels are not implemented features.
- `.specify/memory/constitution.md`: non-negotiable project laws.
- `docs/adr/`: consequential but replaceable technical decisions.
- `specs/`: feature WHAT/WHY, plan, contracts, tasks, and validation guidance.
- runtime source: implementation only.

Before creating a feature, read [Product](product.md) and its entry in the [Roadmap](../ROADMAP.md).
New feature specs link their roadmap entry in Input/summary and the roadmap links back to the spec.
Specs 001 and 002 have companion `roadmap-reference.md` files so traceability does not rewrite their
accepted/in-progress requirements. Product/roadmap changes do not silently supersede the Constitution,
Accepted ADRs, or feature acceptance criteria. See the roadmap for isolated parallel agent work.

## Extending architecture rules

Add an automated boundary rule only when a real ownership boundary exists and a violation would be
costly or difficult to detect in normal review. Each new rule must:

1. name the owning and downstream areas;
2. include one controlled failing fixture and one valid fixture;
3. ignore generated/vendor content such as `node_modules`, virtual environments, and build output;
4. produce an actionable source path in its diagnostic.

Do not create rules for empty future packages.

## Spec 001 boundary

Spec 001 proves the architecture runway only. It does not implement map rendering, GIS editing,
production identity, tenant billing, field/offline behavior, processing, or House Numbering.
Future folders/services are created only when their first real feature needs them.

After the Spec 002 P1 browser gate, migration 0005 retires the disposable FoundationSampleRecord.
The real tenant Project slice now exercises the same create/read, process restart and persistent
browser acceptance path. Health endpoints, dependency boundaries and historical R001 evidence
remain available; the retired sample data is not a product domain or a migration target.

## Core boundary standardization (Spec 033)

The canonical frontend is web-next. The former parallel apps/web implementation
has been retired; application-specific workbench code remains in web-next while
reusable snap, CRS/session and engine contracts live in spatial-workspace or
client-sdk. Shared browser calculations do not replace authoritative backend
CRS/geometry validation.

Official module discovery is an adapter responsibility: HTTP router discovery
lives under adapters/http/official_modules.py, and generic browser discovery
lives in the respective extension registry. Domain code must not import
transport or persistence frameworks. Reusable frontend runtime packages cannot
depend on application internals; test fixtures may exercise applications.
These constraints are checked by tools/check_architecture.py as part of the
existing CI architecture gate. The static check is not a substitute for
security review or real runtime acceptance.

See [Spec 033](../specs/033-core-standardization/spec.md) and
[implementation evidence](../specs/033-core-standardization/AUDIT.md).
