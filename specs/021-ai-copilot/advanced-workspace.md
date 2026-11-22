# Advanced workspace MCP — raster, models, layouts, installed extensions

The same Keycloak OAuth endpoint now offers 36 tools: previous 17, eighteen advanced tools plus operation status. The browser must advertise workspace_version=2. Existing version-1 tabs can still use vector tools but reject advanced operations. Every browser write uses current GIS session, Core permissions, CSRF, explicit command approval and bounded typed inputs. No arbitrary HTTP, SQL, shell, plugin execution or credentials are exposed.

## Coverage
- Project rasters: list, run existing GDAL operations (NDVI/NDWI/NDBI, terrain, clip, mosaic, resample, RGB, calculator etc.), zonal/contour reports, download GeoTIFF. Requires existing project raster; does not upload files on behalf of AI.
- Vector processing: Buffer, Clip, Dissolve and durable job inspection. Existing Core pins input revisions when submitting a job.
- Processing Modeler: list, create/update saved DAG, execute saved exact revision in dependency order, up to 12 steps. Each step produces a new independent layer. On failure the result identifies completed outputs and remaining steps. No rollback of already-created layers. Raster create is not idempotent; inspect output catalog before retry.
- Layout Designer: list/create/update exact saved document, actual browser MapLibre capture and SVG composition into PDF or PNG using the existing physical-page renderer; JSON download also available.
- Extensions: list registered manifests/activations, activate/disable existing extensions through Core, read/create/update schema-bound records. Does not execute arbitrary plugin code or automatically grant organization/project privileges. Plugin-specific APIs outside the common registered-extension contract still require dedicated adapters.

## Long operations
Raster processing/reports/download, processing run, model run, and layout export return an operation ticket immediately, then the user approves in the GIS tab. Use gis_operation_status until done/denied/expired. Terminal receipt consumed once; do not resubmit a write if receipt is unavailable. One pending operation per paired session. Deadline limited by OAuth expiry and browser session (15 minutes), heartbeat and permission checks. Browser checks time budget before processing steps and stops starting new steps if disconnected. An already-submitted Core request may finish; inspect project catalog if disconnected. No durable job queue is added in MCP. Core remains the system of record.

## Frontend
The adapter sources in apps/mcp/web correspond to apps/web-next/src/ai-copilot in frontend. Layout export uses existing LayoutRender helpers; LiveStudio supplies the live map and current visible layer legend. Default production frontend is independent of the AI preview.
