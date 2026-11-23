# R027 — QGIS ↔ OpenVietGIS manual two-way workspace (experimental)

**Implementation:** QGIS plugin 0.3.0 on the R027 feature branch. **This is a reviewed/manual GeoPackage workflow, not an always-on realtime/automatic connector.** R027's persistent server-side connector binding registry remains disabled; the manual workspace uses R026's authenticated HTTPS export and revision-pinned R006 changeset endpoints. It does not activate a server binding.

## Operator prerequisites

- QGIS Desktop 3.28+ (native PyQGIS); local installation and desktop acceptance remain **unverified** in CI.
- Service Principal Bearer with correct **project.read**, and **project.edit** for posting changes; server GIS ACL must independently authorize the target layer/features/fields.
- The selected layer must be a **single-part 2D** Point/LineString/Polygon with a verified explicit CRS and **one** GeoPackage feature table.
- Maximum upload size **16 MiB**, at most **25,000 features**, at most **1,000 proposed reconciliation operations**. Existing R026 server-side limits remain authoritative.
- Only identical source and destination CRS and schema can be merged. No implicit reprojection, no deletes, no field remapping.

## User workflow

1. In QGIS, install OpenVietGIS Connector from `integrations/qgis_plugin/openvietgis_connector/` (ZIP or QGIS plugin directory). Open from **Web → OpenVietGIS**.
2. Enter HTTPS URL, organization UUID, project UUID and short-lived service bearer key. The key is kept in the dialog's memory only and never written to `workspace.json`.
3. Load authorized layers and select a valid layer. Click **1. Tạo bản làm việc** (R027 panel). The plugin downloads a SHA-256-verified GeoPackage and creates:
   - `ovg_qgis_<layer_uuid>/baseline.gpkg` — last common acknowledged snapshot.
   - `ovg_qgis_<layer_uuid>/working.gpkg` — local editable layer loaded into QGIS.
   - `ovg_qgis_<layer_uuid>/workspace.json` — nonsecret project, content/schema revisions, baseline hash and optional recovery journal.
4. Edit `working.gpkg` in QGIS, retaining `ovg_feature_id` as a stable, **unique, nonblank** ID for each feature. For new features, provide a unique ID before saving. Do not change table/column names or CRS. **Save edits and turn off the QGIS edit mode**.
5. Click **2. Xem trước 2 chiều**. The plugin downloads a fresh server snapshot into a separate temporary preview directory, computes the three-way diff, and shows how many edits go **QGIS → OpenVietGIS** and **OpenVietGIS → QGIS**. When any feature or geometry has a conflict, the commit button is disabled and no writes are sent.
6. Review each change count and digest, then click **3. Xác nhận đồng bộ**. After confirmation, the plugin closes its own editable map layer, merges conflict-free server edits into a **staged** local copy, requests an R026 non-mutating `sync-preview`, checks the exact expected server operation count, and (if local updates exist) sends **one** confirmed R006 `sync-push` with a durable operation ID.
7. The plugin downloads a server read-back after the R006 receipt; only after source/target data and pinned content revision match is `working.gpkg` atomically replaced, a new common `baseline.gpkg` saved, and `workspace.json` advanced. The QGIS layer is reloaded from the resulting local file.

## Conflicts / recovery

- Both sides editing a **different field** on the same feature: the merged feature incorporates both authorized changes.
- Both sides editing the **same field differently** or changing geometry differently: **conflict**. The whole manual batch is held, without overwriting either side.
- A feature disappearing from either side: **delete candidate conflict**, never an implicit delete/recreation.
- Schema/CRS mismatch, missing feature IDs, duplicate IDs, large payload, stale server revision, changed local file after preview: **fail closed**.
- On timeout after a server push, the plugin persists `.pending-merged.gpkg` and a nonsecret `pending` record with operation ID and revision. **It does not automatically replay the write.** Choose **Khôi phục giao dịch** to fetch the current server data: recovery finalizes only if it exactly matches staged changes. Otherwise the pending state is preserved for investigation. Open the existing workspace with **Mở workspace** after restarting QGIS.
- If the working layer is in edit mode, close/commit edits first. Other QGIS layers referencing the same local file must be closed to protect SQLite/OGR file handles.

## Security and release limitations

- No autonomous/live polling, webhooks, durable external-identity server mapping, agent pairing, or background execution.
- No external PostGIS, ArcGIS, AutoCAD or MicroStation two-way writes from this plugin.
- R006 is the only OpenVietGIS write path; the standalone workspace client never opens the internal OpenVietGIS database.
- The plugin uses a dedicated project-scoped service bearer, not an OIDC interactive login. A future server-approved agent auth/pairing protocol remains in the R027 backlog.
- Unit coverage exercises real GeoPackage file parsing/writes with mock HTTP responses. An additional **HTTP/PostGIS integration test** performs a real two-way manual round trip using a TestClient session adapter and real R006/R026 endpoints. **PyQGIS Desktop acceptance, actual bearer-client network validation, security review and rollback drill remain required before general release.**

## Development tests

```bash
uv run --project apps/api python -m pytest -q \
  apps/api/tests/unit/integration_hub/test_qgis_workspace.py
uv run --project apps/api python -m pytest -q \
  apps/api/tests/unit/integration_hub
uv run --project apps/api ruff check \
  integrations/qgis_plugin/openvietgis_connector/{client.py,workspace.py,plugin.py}
```
