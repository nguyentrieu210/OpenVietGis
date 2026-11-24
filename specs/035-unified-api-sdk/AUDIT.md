# API and SDK transport findings

Baseline 562fd12 (Spec 034). No live changes or new backend routes.

## Fixed
- managed-sync.ts passed /api/v1 in the request path while ApiClient already owned /api/v1. This generated a double prefix for listManagedSessions, revokeManagedSession and sendDesktopCommand. The QGIS manager previously swallowed list errors into an empty result, so this could look like absence of sessions. Corrected relative project paths and added regression assertions for all three requests, HTTP verbs, CSRF and command body.
- Shared projectApiPath now constructs scoped paths for maps, spatial, transfers, platform, integration hub and managed sync. ApiClient.url is the single base-join function for REST and migrated SSE.
- ApiClient rejects already-prefixed, absolute, network-path, fragment and traversal paths before fetch. This is input hygiene, not server authorization.
- ApiError.readPayload parses a cloned response once into code/detail/message/validationMessages. Map, Spatial and Transfer specialized error handling uses it while retaining existing subclasses, status and original response.
- Web-next scoped GeoJSON export uses requestRaw and centralized ApiError; integration and collaboration event streams use the configured SDK URL. Existing credential and CSRF semantics retained. No automatic mutation retries added.

## Contract coverage
| Area | Current source of contract |
|---|---|
| Auth/session, tenant core, service admin | Generated OpenAPI types |
| Spatial catalog, map workspace, import/export | Generated OpenAPI types |
| Editing, queries, processing, CAD, collaboration, marketplace, integration and managed sync | Hand-written SDK and/or feature-local interfaces; parity pending |
| SSE | Native EventSource with shared base for the two migrated streams; event schemas and runtime revocation acceptance pending |
| QGIS Python / MCP | Independent language clients and auth profiles; cross-client contract tests pending |

Public download URL helpers intentionally return browser navigation URLs and are not ApiClient request paths. External Sentinel services and separately hosted AI/OAuth endpoints are not rewritten into /api/v1.

## Verification
SDK suite 83/83 tests in 18 files passed; focused HTTP/managed tests 6/6 passed again after a test-only lint fix. SDK typecheck and lint passed. web-next production build passed. Six existing OpenAPI documents validated; generated SDK artifacts regenerated without diff. Architecture and documentation navigation passed. No production data or credentials used.

## Remaining work
Full backend route inventory to OpenAPI/SDK parity, browser integration, client runtime validation, unified pagination rules, API compatibility/version lifecycle, download URL base portability, authentication/revocation and real QGIS/MCP testing remain open. No changes to backend response wire format or authorization rules in this slice.
