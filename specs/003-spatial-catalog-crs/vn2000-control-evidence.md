# R003 VN-2000 CRS Control Evidence

**Status**: research evidence only; does not authorize R003 runtime implementation before R002 convergence/promotion.

**Captured**: 2026-11-18

## Authoritative / independent references

- Vietnam Prime Minister Decision 83/2000/QD-TTg establishes VN-2000, uses the WGS 84 reference ellipsoid and the national coordinate framework:
  https://vanban.chinhphu.vn/?docid=7812&pageid=27160
- Circular 973/2001/TT-TCDC remains listed as effective in the national legal database. It defines use of VN-2000 and provincial central meridians; Quang Ninh uses 107°45' for the 3-degree cadastral projection and k0 = 0.9999:
  https://vbpl.moj.gov.vn/botainguyen/Pages/vbpq-toanvan.aspx?ItemID=138922&Keyword=973%2F2001%2FTT-TC%C4%90C
- Decision 05/2007/QD-BTNMT governs the WGS84 <-> VN-2000 conversion parameters:
  https://vupc.mae.gov.vn/vien-tham/3491/quyet-dinh-so-05-2007-qd-btnmt-ngay-27-02-2007-ve-su-dung-he-thong-tham-so-tinh-chuyen-giua-he-toa-d
- Ministry of Construction Circular 50/2025/TT-BXD appendix publishes paired geographic VN-2000 and WGS-84 coordinates for Quang Ninh port-water boundary points. These pairs are independent of OpenVietGIS/PROJ and are rounded to 0.1 arc-second:
  https://moc.gov.vn/Images/FileVanBan/BXD_50-2025-TT-BXD_15122025.pdf
- The Ministry of Agriculture and Environment public consultation repository publishes the 2025 EIA for the Quang Ninh temporary transshipment project. Table 1.4 publishes the same four anchor points simultaneously as VN-2000 TM-3 107°45' projected X/Y, VN-2000 geographic coordinates and WGS-84 geographic coordinates, providing an independent projected control fixture:
  https://thamvan.mae.gov.vn/Uploads/26072025/CL25.7.%20BC%20DTM%20DA%20CHUYEN%20TAI%20HANG%20HOA%20QUANG%20NINH.pdf

## EPSG definitions under consideration

| EPSG | Name | R003 research disposition |
| --- | --- | --- |
| 4326 | WGS 84 | definition accepted; explicit application coordinate convention still required |
| 3857 | WGS 84 / Pseudo-Mercator | definition accepted for web display/reference transformation |
| 4756 | VN-2000 | definition accepted; geographic transform to WGS 84 has independent Quang Ninh control evidence below |
| 3405 | VN-2000 / UTM zone 48N | definition candidate; EPSG scope is Vietnam onshore west of 108°E |
| 3406 | VN-2000 / UTM zone 49N | definition candidate; EPSG scope is Vietnam onshore east of 108°E |
| 5899 | VN-2000 / TM-3 107-45 | definition accepted for the first Quang Ninh projected profile; independent projected/geographic/WGS-84 control evidence verified below |

Lookup references:
- https://epsg.io/4756
- https://epsg.io/3405
- https://epsg.io/3406
- https://epsg.io/5899

EPSG exposes operation 6960 ("VN-2000 to WGS 84 (2)") as the default Vietnam-onshore geographic transformation with stated 1 m accuracy. R003 records separate acceptance tolerances for each independent control fixture based on its published coordinate precision.

## Runtime actually measured

The isolated OpenVietGIS review database reports:

- PostgreSQL 18
- PostGIS 3.7.0
- PROJ 9.9.0
- PROJ network access disabled

The checked `spatial_ref_sys` contains EPSG 4326, 3857, 4756, 3405, 3406 and 5899. Its legacy WKT label for 5899 is not treated as the product display authority; R003 must key definitions by authority/code and canonical metadata, not a stale local alias string.

## Independent geographic control check

Input coordinates below are the VN-2000 geographic values in the Circular 50/2025 appendix. Expected coordinates are the paired WGS-84 values from the same appendix. OpenVietGIS research executed PostGIS `ST_Transform(..., 4326)` from SRID 4756 and measured geodesic distance to the independently published WGS-84 target.

| Point | Published VN-2000 | Published WGS-84 | Error |
| --- | --- | --- | ---: |
| VH1 | 21°24'40.0"N, 108°01'10.0"E | 21°24'36.4"N, 108°01'16.8"E | 1.288 m |
| VH2 | 21°24'52.1"N, 108°03'35.3"E | 21°24'48.5"N, 108°03'42.1"E | 1.333 m |
| VH10 | 21°14'10.5"N, 107°56'46.0"E | 21°14'06.9"N, 107°56'52.8"E | 1.410 m |
| CT1 | 20°58'14.3"N, 107°45'40.8"E | 20°58'10.7"N, 107°45'47.6"E | 1.556 m |
| QY1 | 20°50'07.8"N, 106°52'59.2"E | 20°50'04.2"N, 106°53'06.0"E | 1.213 m |

Observed range: **1.213–1.556 m** for these five points. The inverse EPSG:4326 -> EPSG:4756 check, started independently from each published WGS-84 target, produces the same 1.213–1.556 m residual range against the published VN-2000 coordinates. Both directions are therefore covered by independent published targets, not merely by an implementation round-trip.

## Independent projected control check — EPSG:5899

The Quang Ninh EIA Table 1.4 states the projected system as **VN-2000, central meridian 107°45', 3° zone** and publishes X (northing), Y (easting), VN-2000 geographic and WGS-84 geographic values for the same four anchor points. EPSG:5899 is VN-2000 / TM-3 107-45 with metre units and easting/northing axes; the tested PostGIS geometry therefore uses `POINT(Y, X)`.

OpenVietGIS research transformed each projected point first to EPSG:4756 and independently to EPSG:4326, then measured geodesic distance to the published target values.

| Point | Published X / Y (m) | Error to published VN-2000 geographic | Error to published WGS-84 |
| --- | --- | ---: | ---: |
| HVS-QN1 | 2,291,039.83 / 438,442.69 | 0.125 m | 0.075 m |
| HVS-QN2 | 2,291,878.33 / 437,957.55 | 0.189 m | 0.129 m |
| HVS-QN3 | 2,292,185.83 / 438,891.96 | 0.147 m | 0.052 m |
| HVS-QN4 | 2,291,378.72 / 439,365.92 | 0.168 m | 0.096 m |

Observed residuals are **0.125–0.189 m** against the published VN-2000 geographic values and **0.052–0.129 m** against the published WGS-84 values.

The inverse checks were run independently from the published geographic targets back to EPSG:5899. VN-2000 geographic -> 5899 residuals are **0.125–0.189 m** and WGS-84 -> 5899 residuals are **0.052–0.129 m**, matching the forward fixture within rounding. This verifies both advertised directions against the independent published coordinates rather than relying on a round-trip through the implementation itself.

For the first R003 Quang Ninh projected profile, use a conservative **1 m acceptance tolerance** for this fixture. This is a software acceptance threshold consistent with EPSG's stated 1 m transformation accuracy and the source's published precision; it is not a claim that the EIA coordinates are cadastral control or that OpenVietGIS guarantees survey accuracy.

### Research conclusion

For the first R003 slice, the evidence supports treating **EPSG:4756 -> EPSG:4326** as a validated geographic transformation in the Quang Ninh control region with a conservative **3 m acceptance tolerance**. The 3 m number is an OpenVietGIS test tolerance for this rounded public control table, not a claim of survey/cadastral accuracy and not a replacement for RTK/geodetic control.

The inverse 4326 -> 4756 may be tested against the same pairs but must be recorded separately before acceptance if shipped.

The evidence supports **EPSG:5899 <-> EPSG:4756** and **EPSG:5899 <-> EPSG:4326** for the first Quang Ninh projected profile with the 1 m control-fixture tolerance above. EPSG:5899 therefore moves from a definition-only candidate into the selected verified VN-2000 profile set for R003.

No independent projected easting/northing control has yet been accepted for EPSG:3405 or EPSG:3406. Their definitions may remain discoverable research candidates, but R003 must not advertise those coordinate-changing transformation edges as validated until comparable independent controls are accepted.

This separation preserves the product distinction between a known CRS definition and a validated coordinate transformation, as required by the R003 specification.
