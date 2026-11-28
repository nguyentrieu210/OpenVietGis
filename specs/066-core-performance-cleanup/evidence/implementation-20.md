# 066 implementation slice 20 — image qualification and isolated rollback

## T038 — multi-stage image / BuildKit qualification
- Exact local images were rebuilt from candidate beb4fdbe93ced1a54983e1ff261372a96261cac1 with OCI revision labels. Sizes: web 64,702,802 B; Rust API 586,099,140 B; migration 683,732,000 B; PostGIS 722,734,500 B.
- Web dependency/source layering demonstrates a measured cache win: controlled cold build 67.734 s and identical warm rebuild 1.794 s.
- Exact-candidate warm build/load timings after cache population: web 89.072 s, API 37.739 s, migrate 156.794 s, postgres 41.097 s.
- Isolated docker-container BuildKit emitted SPDX Document and SLSA provenance v1 attestations. Rust full-profile release compile completed in 20m49s; full SBOM/provenance pipeline in 23m47.805s.
- Exact API smoke: health/live 200 without DB and readiness intentionally 503. Exact web smoke: root 200 and hashed JS 200 with immutable one-year cache policy.
- T038 is complete as a build/image qualification. Exact final GHCR attestation is still a release gate under T040/T047.

## T042 — isolated rollback / restore / schema compatibility
- Exact candidate PostGIS and migration images initialized a dedicated network and volume; migrations completed to 0046_map_display_config and candidate Rust readiness returned 200.
- Synthetic database dump was 601,938 bytes with SHA-256 6f0f11e69a144d9027c4fabc194ddadbb1e8671fadb62db8bb4946a2d7ea29c0. Restore to a second database preserved revision and the probe row.
- App rollback on the same synthetic schema succeeded candidate -> retained previous local API image -> candidate; readiness was 200 for both versions and the probe remained unchanged.
- Recovery contract suites test_backup_restore.py and test_recovery_authority.py PASS.
- T042 is complete for isolated rehearsal. In-place DB downgrade remains forbidden; production rollback requires final registry digest receipts.

No production service, database, registry package or live volume was modified.
