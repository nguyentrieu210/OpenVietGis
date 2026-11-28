# Native module SDK — compatibility 1.x

Use this SDK for reviewed, compiled-in modules. For untrusted external applications use the separately sandboxed Partner SDK (docs/partner-plugin-sdk.md). Compiled native and React modules are trusted code, not security sandboxes.

## Boundaries

| Contract | Owner |
|---|---|
| Manifest, pure commands, semantic layer roles | ovg-module-sdk |
| Session proof, CSRF, project access, activation verification | ovg-module-http |
| Transactional GIS read/lock/audit ports | ovg-storage::module_ports |
| Versioned read-only PostGIS views | extensions.spatial_features_v1 / spatial_sources_v1 |
| Client requests and scoped invalidation notices | @openvietgis/client-sdk |
| React contribution and resident runtime | @openvietgis/spatial-workspace/plugin |

Keep domain, persistence, HTTP and frontend in module-owned packages. Only the composition root may register a compiled business crate. Never import Core application internals, open unscoped DB sessions, bypass RLS, or directly write GIS feature tables. Generic SQL views are versioned and retain invoker RLS; they do not give a separate per-plugin DB identity.

## Add a module

Copy examples/module-sdk/quality-check as a reference. Declare a unique extension key, semantic version, API compatibility 1.x, namespaced permissions, commands, entity schemas and contributions. Implement OvgModule, register via ModuleRegistry at application startup and add an explicit optional feature. Pure commands must be deterministic, bounded and side-effect-free; persistent commands need an authorized transactional HTTP adapter. The registry snapshots semantic roles and pure command declarations, rejects duplicate/invalid roles, and pins the manifest at registration.

## Project lifecycle and HTTP

All endpoints below are relative to /api/v1/organizations/{org}/projects/{project}/extensions/{key}. Use the existing authenticated client. Register and activate the exact manifest first. Unsafe methods require CSRF. Read access requires unrestricted project.read, compatible active runtime, declared ext.{key}.read and the actor's effective module permission. Explicit allowlists can narrow Core access; an absent policy inherits existing role access. An empty allowlist denies all module actions. Disable removes runtime access but preserves configured bindings and policies. Version transitions must use the activation CAS API; do not treat identical keys as interchangeable versions.

| Method / path | Purpose |
|---|---|
| GET /runtime-contract | Discover versioned semantic roles and exposed pure commands |
| GET /layer-bindings | Read schema-pinned source-role links |
| PUT /layer-bindings | Replace with expected_revision; human manager and module manage |
| GET /spatial-summary/{role} | Count features and present geometries in ready, active, explicitly bound sources with matching schema |
| POST /commands/{command} | Execute pure bounded computation with declared Core and extension permissions |
| GET /permissions/self | Read own override, not a grant elevation |
| GET project /extensions/change-events | Replay durable configuration invalidation using cursor |

The aggregate response contains only role, source_count, total and geometry_present. Zero bound sources is not a successful completion assessment. No arbitrary SQL, attribute export or private household values are exposed. Raw feature APIs and persistent commands need separately reviewed scoped contracts.

## TypeScript example

Use the SDK from a trusted project workspace with the existing ApiClient instance:

```ts
import {getModuleRuntimeContract,getBoundSpatialSummary,dispatchModuleCommand,subscribeModuleChanges} from "@openvietgis/client-sdk";

const contract = await getModuleRuntimeContract(client, org, project, "quality_check");
const summary = await getBoundSpatialSummary(client, org, project, "quality_check", "survey");
const result = await dispatchModuleCommand<{percent:number|null}>(client,org,project,"quality_check","completion_rate",{total:100,complete:75});
const stop = subscribeModuleChanges(client,org,project,"quality_check",notice => {
  // Re-read the authoritative API; do not treat a notice as permission or data.
});
// Call stop() when the workspace is disposed.
```

React official contributions use StudioPlugin.useStudio and the shared StudioHost contract. Register a reviewed module frontend at modules/{key}/frontend/studio.tsx alongside its manifest for build-time discovery. Background modules must not initiate map changes; only the foreground contribution owns global map filters and toolbar. Cleanup subscriptions on unmount and remove contributions on permission loss. A frontend must not be shipped unless the corresponding backend runtime is included.

## Acceptance checklist

Check Core build without module feature, registry manifest compatibility, activation/disable, foreign tenant rejection, missing or stale layer binding, CSRF, deny/restore permission, UI state retention, audit identity, and unchanged geometry/editing behavior. Run disposable PostGIS fixtures; do not use live data in automated tests. The sample is a reference, not a published community package or fully isolated third-party execution environment.
