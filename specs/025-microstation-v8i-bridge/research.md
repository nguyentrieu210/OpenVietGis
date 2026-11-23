# R025 Research and Known Limitations

- The existing `apps/api/openvietgis_api/domains/external_sources` R010 provides allowlisted HTTP sources (WMS, WMTS/XYZ, COG/GeoTIFF, WFS OGC Features) and read-only capabilities, **not** CAD COM automation.
- Existing `docs/developer-integrations.md` describes service-application credentials with a `project.read` ceiling; it **does not** authorize CAD sync writes.
- V8i has COM/VBA automation on an installed Windows desktop; actual CAD entity extraction and CAD roundtrip need Windows V8i validation. Treat pywin32 COM attach as experimental, not evidence of parcel geometry support.
- Bentley V8i SDK/typelibs may be proprietary; do not bundle SDK or claim the bridge is a drop-in V8i runtime.
- Native DGN V8 server-side GDAL access can depend on proprietary ODA build; don't assume a default GDAL installation handles V8.
- Cadastral semantics are not guaranteed by .dgn geometry: parcel attribution may live in FAMIS/gCadas/ViLIS/external linked tables, with geographic projection variants for VN-2000.
- For Phase 1, prefer fixture-safe envelope and checksums; no network upload and no automatic geometry guesses.
- Questions for Windows PoC: exact V8i build/bitness; COM ProgID; active-model Scan behavior; Element.ID stability; support for line strings/shapes/cells; source units, VN-2000 zone and auxiliary attribute store.
