# W3 implementation checkpoint
Candidate 47c043ba8151efa76472dabd3c23e5132f3939d9. Owner requested early W3 source work; A0, W1, W2 acceptance still open. No release, no parent or leaf accepted.

- a746778: Geo 0.31 validation for bounded regional WGS84 XY topology, point/line/polygon drafts, exact interior vertex split and coincident endpoint merge, local tangent snap; typed fields and GGA parser.
- 4032f97: SQLite migrations, persistent local edit groups with before/after images, revision journal and undo/redo transaction, typed schema, paginated attribute query, indexed candidate snapping, track segments/recovery/GPX.
- 8033a86: typed FRB host APIs, durable command ID migration, test fixtures. Initial compile failure lines_iter corrected at b370310.
- b370310: Flutter draw preview, snap/undo/redo, attributes list, explicit OS GPS permission and waypoint. [Focused run 46170034589](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46170034589) SUCCESS: 12 shared spatial tests (44 observations), 11 engine tests, 6 storage tests = 29 PASS, including 4 new W3 tests. Android/iOS dependency guard passed; no native device proof.
- 7d00cfd: foreground track stream/pause/resume and candidate XChaCha20-Poly1305 blob vault with external scoped key, hash, fsync, staging cleanup. No encrypted managed DB, OS keystore, capture UI or committed attachment queue. Blob test pending.
- 2acf58d: atomic split/merge feature commands, revision-aware JSON attribute editor and explicit GPX share, test pending. New grouped operations reuse existing local journal. Non-edit mutation/schema change invalidates undo stack; no managed data. 
- 47c043b: Dart FRB nonmutation/commit/history test source and analyzer lint fixes. Flutter runtime/device not confirmed.
- Prior W2 GDAL host raster run 46169569047 PASS. Prior Flutter run 46169490828 failed only reported curly_braces lint at native_map.dart, addressed in source; candidate builds still outstanding. W1 APK CargoKit patch not yet verified.

Outstanding: advanced geometry editing/touch vertex handles, snap physical p95 and race proof, full typed forms, background service and screen-off tracking, camera/QR capture/consent, durable encrypted attachment reference queue, NTRIP/receiver integration/correction-loss handling, encrypted scoped managed DB and OS key wrap with WAL/temp/backup inspection, physical A1 standalone acceptance, independent exports and earlier gates. W3 candidate is local-only; no protected managed write, no production changes.
