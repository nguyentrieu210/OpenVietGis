# Dedicated-origin OpenCAD bridge and version-pinned georeference (candidate)

## Safety boundary
- Editor is disabled when VITE_OVG_CAD_RELAY_ORIGIN is absent or not an exact HTTPS origin (loopback HTTP is accepted for synthetic testing). It must differ from the GIS origin. Never embed the CAD release in the authenticated GIS origin.
- GIS fetches and hashes the version before handing the bytes to a sandboxed cross-origin relay. No access token, CSRF token, project identifier or API path is sent to the relay. The relay and engine accept only the pinned source window/origin, a fresh channel identifier, protocol/version/capabilities, bounded bytes and permitted messages. The untrusted editor cannot itself authorize writes.
- Only latest revision with project.edit is writable. Old revisions are read-only, save uses expected_version, and success requires a matching returned version, SHA-256, byte count, format and ID. The GIS host sends an acknowledgement after persistence, not before. Revocation closes the editor; dirty/saving state blocks context switches.
- The engine actually tested was deployed release 2026.46 with the release manifest in infra/cad-relay/release.sha256. Embedded release metadata reports 3976af345b0f244e59f5c5e8c40c0e42805a72d2; that commit is not present in the local OpenCAD checkout (141d417). The asset pin is evidence of tested bytes, not proof of trustworthy source provenance or license compliance. Production origin configuration, DNS/TLS, cookie and CSP isolation, license/source/build verification and approval remain required.
- The checked-in staging and serving scripts refuse non-loopback parent origins; they are synthetic-only, not a production deployment recipe. Staging verifies the complete file inventory and strips analytics and control script. No production data has been transferred.

## Version-pinned DXF placement API
Base: /api/v1/organizations/{org}/projects/{project}/cad/documents/{id}/versions/{version}.
Both POST endpoints require unrestricted human project.edit and CSRF; no caller-provided original bytes or alternate source ID.
- /inspect-georef: crs_code=3857, units=meter|centimeter|millimeter, controls=3–100 pairs of [CAD x,y] and [grid e,n], tolerance_m, max_scale_deviation (fraction, 0–0.2). Reads immutable verified bytes; non-reflecting similarity transform in grid metres, before geometry validation. Returns digest, fingerprint, geometry_valid, entity_summary (placement, extent, version), loss_report. No layer written.
- /import-georef: same profile plus layer_name, expected_sha256, expected_inspection_fingerprint, acknowledge_loss=true. Rejects stale/changed inputs. Transactional layer/source creation uses the existing bounded DXF import pipeline; records CAD document/version, fit parameters, original digest, fingerprint, audit and conversion loss. Repeated same profile in same project returns existing successful conversion; serialized by transaction advisory lock. Source drawing remains untouched.
- Only supported homogeneous 2D DXF geometry; not DWG, terrain, native CAD curves, BIM or automatic CRS inference. EPSG:3857 grid lengths/area are not ground survey measurements. No VN-2000 guessed transformation; future VN-2000 requires explicit projection/profile and tests. 3D remains the existing GIS footprint view with explicit height attribute; conversion does not invent heights.

## Synthetic replay
1. Copy/access the exact tested CAD asset directory, not live storage, and run:
   python infra/cad-relay/stage-test.py /path/to/pinned/assets /new/path/to/stage --parent-origin http://127.0.0.1:4280
2. Serve stage on 127.0.0.1:4281 via serve-test.py (root, port, parent_origin arguments); serve infra/cad-relay/tests on 127.0.0.1:4280 using python -m http.server 4280 --bind 127.0.0.1 --directory infra/cad-relay/tests.
3. Run node infra/cad-relay/tests/smoke.cjs with PLAYWRIGHT_CORE and CHROMIUM_BIN set as appropriate. Real WASM smoke uses only the checked-in synthetic DXF. Stop both loopback servers.
4. Web host mock browser smoke: build web-next in an isolated output directory with VITE_OVG_CAD_RELAY_ORIGIN=http://127.0.0.1:4281, serve on :4187 and run apps/web-next/cad-editor-georef-smoke.mjs. Do not publish that build.
5. Server integration: fresh dedicated PostGIS test database and apps/api/tests/integration/spatial/test_cad_georef.py plus test_cad_documents.py. Never use production or preflight databases.
