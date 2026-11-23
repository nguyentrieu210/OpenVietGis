# OpenVietGIS Windows ↔ MicroStation V8i — local receive connector (R028 pilot)

**Execution:** Windows 10/11 + Python 3.10+; for COM detection, install `pywin32` matching the Python install. Works for receiving DXF/GPKG without V8i/pywin32. A licensed **running** MicroStation V8i is required for the optional COM probe.

**Status:** source/package and Linux-mocked tests only; real V8i Windows acceptance **NOT DONE**. This is **not** a full two-way DGN editing connector. No service is installed, no background sync, no inbound listening port, no direct DGN modification, no Bentley SDK redistribution, and no credential persistence.

## Install and attach from MicroStation

1. Download the contents of this directory to Windows (or clone the OpenVietGIS repository). Double-click `install.cmd`. It copies the companion into `%LOCALAPPDATA%\OpenVietGIS\Connector` **without administrator permissions**. Ensure Python 3.10+ is available as `py -3` or `python` on PATH.
2. In **MicroStation V8i** open **Utilities → Macro → Project Manager**; create/open your own `.mvba` project, open the VBA editor (Alt+F11) and **File → Import File** → select `OpenVietGISConnector.bas` from the installed directory.
3. Load the project and run `OpenVietGIS_Connect` (via VBA Project Manager or the V8i Key-in/Macro interface). A button can be bound to that macro via V8i Customize; exact Key-in depends on your chosen project name. This VBA hook **only launches the Windows companion**. No `.mvba` binary is shipped because it requires the Bentley VBA editor/runtime to generate.
4. Alternatively run `%LOCALAPPDATA%\OpenVietGIS\Connector\start.cmd` directly, without using V8i.
5. Optionally install pywin32 with `py -3 -m pip install pywin32` to enable **Kiểm tra V8i đang mở**. Do not use COM probe as proof of correct coordinate units.

See Bentley's [V8i VBA project loading documentation](https://bentleysystems.service-now.com/community?id=kb_article_view&sysparm_article=KB0109946).

## Receive a layer

1. In Connector enter server HTTPS origin (default `https://openvietgis.cloud`), Organization UUID, Project UUID, Layer UUID and an authorized **temporary** `project.read` service principal bearer key. The key is held in memory for a single receive operation, cleared from the GUI and **never written to disk**.
2. Click **Nhận dữ liệu từ OpenVietGIS**. The receiver calls the existing **R026 read-only Integration Hub API**, separately fetching:
   - `GET .../adapters/autocad/layers/{id}/export?format=dxf` (2D **geometry only**)
   - `GET .../adapters/qgis/layers/{id}/export?format=geopackage` (full attribute table).
3. Both downloads **must** have valid SHA-256, matching content/schema revision, CRS and feature count. Only explicit projected metre `EPSG:5899` or `EPSG:3857` passes. The source must have 1–10,000 features and a supported 2D geometry. The server may reject an unauthorized, unsupported, incomplete or conflicting layer.
4. One immutable bundle becomes visible under `%LOCALAPPDATA%\OpenVietGIS\Incoming\<layer>-<revision>-<hash>\` containing `geometry.dxf`, `attributes.gpkg` and `receipt.json`. A failed/inconsistent download never publishes a new bundle.
5. In V8i, inspect the DXF in a separate design/attach as a reference **if supported in your licensed V8i workflow**. Compare with known control points and confirm coordinates, model master units and origin before using it for any parcel or cadastral work. **Never treat raw 2D DXF import as a certified VN-2000 cadastral conversion.** The GeoPackage holds the data table missing from DXF.

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

No Windows service installer or `.exe` binary is produced by this source-only build. Future packaging as a signed `.exe` is a separate release gate.