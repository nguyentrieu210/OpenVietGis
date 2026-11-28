# SPEC 073 W0 — security and qualification boundaries
Status: candidate decision record; not acceptance. No live data, credentials or release signing.
| Threat | Boundary | Required evidence |
|---|---|---|
| Archive/path traversal and decompression | bounded private staging; reject symlinks, path escapes and excessive entries/expanded bytes | malicious fixtures and atomic cleanup |
| GIS parser crash | isolated bounded jobs; validate geometry, source size, handles, panic/error mapping | sanitizer/fuzz and Android lifecycle tests |
| Mobile/server credential confusion | dedicated human public-client PKCE and explicit typed session; preserve existing cookie/CSRF and Bearer service precedence | existing contract negatives plus future real mobile code/replay/refresh tests |
| Lost ACK and duplicate sync | stable operation id, exact payload hash, receipt lookup; one ACID domain | isolated PostGIS and crash/unknown receipt tests |
| Cross-tenant/cache leakage | server/account/org/project scoped key, encrypted store, lease expiry lock, no backup plaintext | inspect SQLite WAL/temp/index/blob/export, cross-scope and reboot/revoke |
| Untrusted plugins | separate v2 signatures, content hashes, scoped broker, bounded graph/actions | v1 golden regression, independent v2 canonical vectors, tamper, revocation and action tests |
| Supply chain | immutable source refs, registry source hashes, SBOM, APK provenance; no release secrets in qualification | review dependency license and reproduced payloads |
## Candidate budgets and devices
Retain normative 4GB/8GB ARM64 memory profiles, 512/768 MiB RSS, 96/192 MiB cache, 2/4 worker and 1/2 job limits, cold p95 <=2s, snap p95 <=50ms, frame p95 <=16.7ms at specified visible load. Physical device model, OS, resolution, refresh rate, thermal and brightness are not frozen because no authorized physical device is connected. Device evidence mandatory before A0 transport qualification and W7 performance acceptance.
## Independent fixtures
Native smoke uses 4326->3857 at longitude 1°, latitude 0° (analytic equatorial reference 111319.49079327357 m from R=6378137), tolerance 1 mm; square polygon analytic area 4; GeoPackage Unicode point round-trip and RTree query. Not VN-2000 regional qualification. Existing independent spatial fixtures from baseline must be reviewed and frozen for T028. The MapLibre fixture is an inline synthetic GeoJSON test point, not a production basemap.
## Compatibility test
The Rust v1 verifier golden vector uses the public RFC8032 test key and independent Python canonical JSON signing. New test checks byte equality, valid signature, changed label and added mobile field rejection. Credential tests exercise mounted router with deliberately untrusted test inputs, no database or production. These tests do not prove mobile human authentication or v2 runtime.
## Licensing
Registry pinned vcpkg candidate versions are listed in infra/mobile/toolchain/qualification.json; license texts and source digests must be collected from actual build. Static linkage is a feasibility spike only; no distribution approval. GEOS LGPL obligations and GDAL dependency licensing need review; SQLCipher/managed storage not qualified. No commercial checkout/payout scope. Android guidance: https://developer.android.com/guide/practices/page-sizes.
