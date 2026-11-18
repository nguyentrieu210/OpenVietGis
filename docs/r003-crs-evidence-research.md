# R003 CRS evidence research notes

Status: evidence preparation, not an accepted transformation profile or survey certification. R002 convergence remains a prerequisite to R003 runtime implementation.

## Verified references and candidate controls

- [Decision 83/2000/QĐ-TTg](https://vanban.chinhphu.vn/?docid=7812&pageid=27160): official national reference-system decision.
- [PROJ projinfo](https://proj.org/en/stable/apps/projinfo.html): inspect exact CRS definitions/candidate operations, geographic extent, accuracy and grid availability; no ballpark substitute. Pin runtime, definitions and operation.
- [RFC 7946 section 4](https://www.rfc-editor.org/rfc/rfc7946.html#section-4): GeoJSON uses WGS84 longitude/latitude (OGC CRS84), not an arbitrary native projected encoding.
- [Government-hosted conversion appendix candidate](https://datafiles.chinhphu.vn/cpp/files/vbpq/2026/01/02-bxd.pdf), page 4: contains paired geographic VN-2000/WGS84 entries HĐ1/HĐ2/HĐ5–10 in southern port waters. Its extracted appendix header has blank regulation number/date; identify signed final publication and verify source provenance and reuse terms before accepting as a reference. Displayed precision and unknown conversion method do not establish survey uncertainty.
- [BXD 50/2025/TT-BXD candidate appendix](https://moc.gov.vn/Images/FileVanBan/BXD_50-2025-TT-BXD_15122025.pdf): agent found possible paired geographic northern positions; coordinator retrieval failed (502), so unverified candidate only.
- [TCVN 9401:2024 official catalogue](https://tieuchuan.vsqi.gov.vn/tieuchuan/view?sohieu=TCVN+9401:2024): active GNSS engineering survey standard; copyrighted full text is sold. Obtain a lawful copy and clarify datum, definitions and permitted reuse before considering any examples as reference controls. No purchase made.
- [National public service for survey/mapping data](https://vpcp.dichvucong.gov.vn/p/home/dvc-chi-tiet-thu-tuc-hanh-chinh.html?ma_thu_tuc=1.011672): documented acquisition path for authorized nonrestricted products; no data requested or obtained.

## Verification matrix

1. WGS84 geographic/CRS84 vs EPSG axis conventions: explicit source axis and units, asymmetric control values, negative tests for swapped invalid latitude, unknown CRS, unknown/unsupported transform, missing grid, non-finite numbers, out-of-domain, and known GeoJSON path.
2. Web Mercator: independent published projection formula and published control values verified against the exact original source; check forward/inverse and display-only distortion. Synthetic fixture may measure algorithm correctness, not an independent VN-2000 datum control.
3. Selected VN-2000 case: record area of use, datum/epoch, projection and units, official definition and exact operation ID/parameters, grids/checksums and license. Validate independent geographically distributed paired controls, with source uncertainty and accepted tolerance documented before measurement. Never infer projection zones, central meridian or transformation parameters from a label.
4. Candidate port-water paired conversions may support a geographically bounded coarse reference comparison after provenance/tolerance approval, but are neither independently surveyed ground-truth nor certification for any projected VN-2000 cadastral profile. Same-engine PROJ/PostGIS/QGIS round-trip is not independent validation.
5. For R003–R005, independently verify source/control values, resulting coordinates, geometry/count/schema/null consistency, extent and CRS provenance; report errors and tolerance clearly. If required control data cannot lawfully be obtained or verified, keep that profile unavailable rather than claim survey precision.

No authoritative independent survey control-pair dataset has been accepted. The VN-2000 evidence gate remains open.

## 2026-11-18 authoritative-source follow-up

- [Decision 05/2007/QĐ-BTNMT, Article 1](https://chinhphu.vn/default.aspx?docid=21337&pageid=27160): authoritative seven-parameter datum conversion definition (three translations, three rotations in arc-seconds, scale factor). It is not itself independent point evidence. Obtain the authority's usage guidance and establish transformation direction, rotation convention and scale representation before configuring PROJ; no guessed pipeline.
- [Final Circular 50/2025/TT-BXD, official record](https://vanban.chinhphu.vn/?classid=1&docid=216232&pageid=27160) and [signed final Gazette PDF, 31 December 2025, pp. 43 onward](https://congbaocdn.chinhphu.vn/CongBaoCP/CongBao/2025/12/46876/60438-1-1751-1752.pdf): verified public geographic VN-2000/WGS84 pairs for Quảng Ninh port waters, printed to 0.1 arc-second. Article 1 identifies converted positions. This resolves the earlier northern-source provenance gap only; conversion outputs are not independently measured control pairs, and printed precision is not uncertainty. No numeric fixture, tolerance or implementation accepted.
- [HCMC Geoportal CORS metadata](https://geoportal-stnmt.tphcm.gov.vn/geonetwork/geonetwork/api/records/06055d6a-0e0e-4d4f-ad98-72f6fc18fece): catalogue identifies a 65-station reference network with VN-2000 and ITRF2014 coordinates; the actual coordinate values are not present in the exposed metadata. Copyright/access constraints and fee-based access are listed. ITRF2014 is not automatically interchangeable with a specified WGS84 realization/epoch. This is a lawful candidate acquisition path, not permission to acquire, publish, purchase or treat these values as verified independent controls.
- [Official technical explanation of Circular 03/2020/TT-BTNMT](https://vupc.mae.gov.vn/---do-dac-ban-do-va-thong-tin-dia-ly/1565/do-noi-mang-luoi-tram-dinh-vi-ve-tinh-quoc-gia): identifies separate adjustment procedures and requirements for VN-2000 and ITRF; nominal technical requirements are not a substitute for individual point uncertainties or documented datum realization.

- [Circular 02/2026/TT-BXD, Gazette record](https://congbao.chinhphu.vn/van-ban/thong-tu-so-02-2026-tt-bxd-468760.htm): final publication record for Đồng Nai; the final Gazette PDF could not be retrieved. The earlier candidate appendix remains a converted-position reference only, not accepted independent evidence.

Keep the R003 evidence gate open. If lawful dual-frame coordinates with provenance, epoch/realization, independent measurement and uncertainties cannot be obtained without new authority, report that as a real profile blocker. Do not publish source data in the public repository or promise certified precision.
