# OpenVietGIS Windows ↔ MicroStation V8i — local receive connector (R028 pilot)

**Execution:** Windows 10/11 + Python 3.10+; for COM detection, install `pywin32` matching the Python install. Works for receiving DXF/GPKG without V8i/pywin32. A licensed **running** MicroStation V8i is required for the optional COM probe.

**Status:** R028 Windows local receive + explicit geometry-only writeback pilot; Linux mocked tests only; real V8i Windows acceptance **NOT DONE**. This is **not** a full two-way DGN editing connector. No service is installed, no background sync, no inbound listening port, no direct DGN modification, no Bentley SDK redistribution, and no credential persistence.

## Install and attach from MicroStation

1. Download the contents of this directory to Windows (or clone the OpenVietGIS repository). Double-click `install.cmd`. It copies the companion into `%LOCALAPPDATA%\OpenVietGIS\Connector` **without administrator permissions**. Ensure Python 3.10+ is available as `py -3` or `python` on PATH.
2. In **MicroStation V8i** open **Utilities → Macro → Project Manager**; create/open your own `.mvba` project, open the VBA editor (Alt+F11) and **File → Import File** → select `OpenVietGISConnector.bas` from the installed directory.
3. Load the project and run `OpenVietGIS_Connect` (via VBA Project Manager or the V8i Key-in/Macro interface). A button can be bound to that macro via V8i Customize; exact Key-in depends on your chosen project name. This VBA hook **only launches the Windows companion**. No `.mvba` binary is shipped because it requires the Bentley VBA editor/runtime to generate.
4. Optional: run `OpenVietGIS_OpenDXFDialog` in the imported macro module to open V8i’s **DXF chooser**, then select the received `geometry.dxf` with **read-only** checked. This native key-in needs an actual V8i installation for acceptance; it does not silently change a DGN.
5. Alternatively run `%LOCALAPPDATA%\OpenVietGIS\Connector\start.cmd` directly, without using V8i.
6. Optionally install pywin32 with `py -3 -m pip install pywin32` to enable **Kiểm tra V8i đang mở**. Do not use COM probe as proof of correct coordinate units.

See Bentley's [V8i VBA project loading documentation](https://bentleysystems.service-now.com/community?id=kb_article_view&sysparm_article=KB0109946).

## Receive a layer

1. In Connector enter server HTTPS origin (default `https://openvietgis.cloud`), Organization UUID, Project UUID, Layer UUID and an authorized **temporary** `project.read` service principal bearer key. The key is held in memory for a single receive operation, cleared from the GUI and **never written to disk**.
2. Click **Nhận dữ liệu từ OpenVietGIS**. The receiver calls the existing **R026 read-only Integration Hub API**, separately fetching:
   - `GET .../adapters/autocad/layers/{id}/export?format=dxf` (2D **geometry only**)
   - `GET .../adapters/qgis/layers/{id}/export?format=geopackage` (full attribute table).
3. Both downloads **must** have valid SHA-256, matching content/schema revision, CRS and feature count. Only explicit projected metre `EPSG:5899` or `EPSG:3857` passes. The source must have 1–10,000 features and a supported 2D geometry. The server may reject an unauthorized, unsupported, incomplete or conflicting layer.
4. One immutable bundle becomes visible under `%LOCALAPPDATA%\OpenVietGIS\Incoming\<layer>-<revision>-<hash>\` containing `geometry.dxf`, `attributes.gpkg` and `receipt.json`. A failed/inconsistent download never publishes a new bundle.
5. In V8i, inspect the DXF in a separate design/attach as a reference **if supported in your licensed V8i workflow**. Compare with known control points and confirm coordinates, model master units and origin before using it for any parcel or cadastral work. **Never treat raw 2D DXF import as a certified VN-2000 cadastral conversion.** The GeoPackage holds the data table missing from DXF.

## Explicit V8i → Cloud pilot (geometry only, disabled by default)

After receiving an **authorized editable** layer, select **Chọn phiên đã nhận**, open the appropriate *existing* DGN in V8i, and select exactly one CAD Line/LineString/Shape object. Verify model master units, global origin and control points against the received CRS; tick the verification checkbox. Click **Ánh xạ đối tượng đang chọn** and enter an **existing** `ovg_feature_id` present in `attributes.gpkg`. The connector refuses a link unless the CAD element geometry already equals that feature’s original geometry within 1e-7 source units. It never attempts fuzzy parcel matching or automatically links IDs.

Repeat this for explicitly approved features. After editing those same elements in the DGN, click **Tạo bản thay đổi**. A local `outbound-review.gpkg` and `outbound-plan.json` are created without modifying `attributes.gpkg` or the DGN; only mapped geometry changes are included, with a 100-feature/5000-scan ceiling. Click **Xem trước thay đổi trên Cloud** to call the existing R026 `qgis/sync-preview`; all operations must match the mapped IDs, only update geometry, and have the original pinned revision. The preview does not commit.

**Production write-back is OFF.** For an explicitly authorized **non-production test project only**, an operator may temporarily set two Windows environment variables before launching the app:

```bat
set OVG_V8I_WRITEBACK_PILOT=ALLOW_EXPLICIT_TEST_WRITE
set OVG_V8I_TEST_PROJECT_ID=<approved-test-project-uuid>
start.cmd
```

The operator then re-enters their `project.edit` bearer key, clicks **XÁC NHẬN GỬI lên Cloud**, enters the first 12 characters of the exact preview SHA-256, and confirms a second dialog. The client runs a fresh preview before invoking the existing R026 `sync-push` changeset endpoint. It journals its operation ID **before** the remote write; ambiguous network outcomes stop further retries until someone checks the authoritative server audit/change history. Auto sync, automatic deletion, DGN editing/backup and R027 durable pairing remain disabled.

After a successful writeback, re-download a new bundle at the new server revision before another round. The R028 mapping is tied to the original revision and must not be recycled.

### Limitations of this pilot

- COM execution has **not** been verified with real V8i; no SDK DLL/redistributed Bentley binaries supplied.
- Attribute changes in DGN, compound entities, arcs, cells, holes, XRefs, Z/M, multipart/split/merge, projection transformations, device pairing and durable bidirectional event feeds are not supported.
- `IMPORT DXFFILE` is a user-driven file selector; the application never silently attaches/imports into the current DGN. Geometry mapping requires exact source-coordinate agreement; a false master-unit/CRS attestation must not be bypassed.
- Existing R026 API supports a two-step *manual* upload; this is NOT an R027 live two-way synchronization release.

## Safety and limits

- Export only, no writeback/deletions; no automatic `ActiveModelReference.AddElement`, no updates to your DGN.
- URL must be `https://` origin with no redirect; verified certificate, no cross-host token forwarding; bounded response 32 MiB per export.
- The files are **staged** for user-controlled inspection, not automatically consumed by MicroStation. V8i COM probe attaches to an existing application; it never launches V8i.
- This is a local inbound **pull** bridge. Future two-way execution will require approved R027 server pairing, durable external ID mapping, R006 changeset acknowledgements, manual collision review, V8i real Windows acceptance and backup/restore tests.
- Avoid putting tokens in screenshots/bug reports. Store nothing in a DGN file. Revoke the bearer key from OpenVietGIS if the computer is compromised.

## Developers

```bash
python -m unittest discover -v -s integrations/windows-v8i-receiver/tests
```

No Windows service installer or `.exe` binary is produced by this source-only build. In the repo, run integration fixtures with `PYTHONPATH=apps/api:integrations/windows-v8i-receiver /opt/openvietgis/apps/api/.venv/bin/python -m unittest discover -v -s integrations/windows-v8i-receiver/tests`. Future packaging as a signed `.exe` is a separate release gate.