# SPEC 073 execution ledger — W0
Snapshot based on implementation commit db2ef12c08a9943ee805cec1ebb78725665f4a34; evidence workflow run [46166872664](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46166872664). No task has met the full parent acceptance requirements. Ownership labels are intended roles, NOT running agents. No acceptance inferred from green audit.

| Engine | Wave | Task | Owner role | Dependencies | Leaves | Status | Commit / evidence | Next action / blocker |
|---|---:|---|---|---|---:|---|---|---|
| E01 | 0 | T001 | Architecture / native build | none | 3 | IN PROGRESS | db2ef12 PASS graph; 07b6ae9 mounted inventory PASS | Complete inventory coverage and independent review |
| E01 | 0 | T002 | Architecture / native build | T001 | 3 | SPIKE IN PROGRESS | 08daa37; c366b9d setup PASS | Freeze complete provenance; exact SDK and locks |
| E01 | 0 | T003 | Architecture / native build | T002 | 4 | SPIKE IN PROGRESS | 46167470966 native job | Build/link, then device 4K/16K and iOS feasibility |
| E01 | 0 | T004 | Architecture / native build | T002 | 3 | SPIKE IN PROGRESS | ff52556 map retry; f3f032a FRB job | APK and device render/FFI/copy proof remain |
| E16 | 0 | T005 | Security/QA/release | T001 | 4 | SPIKE IN PROGRESS | 07b6ae9 threat model | Freeze physical devices, fixtures, SBOM/license review |
| E10 | 0 | T006 | Dart SDK/auth/sync | T001 | 4 | SPIKE IN PROGRESS | 07b6ae9 existing route/auth tests PASS | Verify existing negative cases; freeze mobile auth/lease contract |
| E11 | 0 | T007 | Plugin runtime | T001 | 3 | SPIKE IN PROGRESS | 07b6ae9 marketplace v1 tests PASS | Run v1 golden and freeze separate v2 envelope |
| E01 | 0 | T008 | Architecture / native build | T003, T004, T005, T006, T007 | 3 | BLOCKED | A0 not accepted | Requires all mandatory A0 evidence |
| E02 | 1 | T009 | Portable kernel / FFI | T008 | 4 | IN PROGRESS | e6a00ba portable graph PASS | Strict XY/shared regression and A0 |
| E02 | 1 | T010 | Portable kernel / FFI | T009 | 3 | IN PROGRESS | fe7cef1; engine 3/3 PASS | Additional parallel close, layer handles and A0 |
| E02 | 1 | T011 | Portable kernel / FFI | T010 | 3 | IN PROGRESS | bb6ca36; WKB test PASS | Generate bridge, device/copy and scoped batches |
| E03 | 1 | T012 | Flutter shell | T008 | 3 | IN PROGRESS | bb6ca36 Flutter source | Build candidate and fresh offline device test |
| E03 | 1 | T013 | Flutter shell | T012 | 3 | IN PROGRESS | bb6ca36 navigation and vi/en | 360px layout/accessibility and permission tests |
| E03 | 1 | T014 | Flutter shell | T012 | 3 | IN PROGRESS | bb6ca36 native workspace host | Generated bridge and restart test |
| E08 | 1 | T015 | Storage | T009 | 3 | IN PROGRESS | cbc45a2; local tests PASS | GPKG oracle, migration and device qualification |
| E08 | 1 | T016 | Storage | T015 | 3 | IN PROGRESS | cbc45a2; snapshot test PASS | Disk full/crash injection and quota proof |
| E08 | 1 | T017 | Storage | T016 | 3 | IN PROGRESS | cbc45a2 local journal PASS | Managed encrypted outbox/receipt/cursor not implemented |
| E16 | 1 | T018 | Security/QA/release | T011, T013, T014, T017 | 3 | IN PROGRESS | 46168095743 native tests PASS | Final integration and regression; A0 not accepted |
| E04 | 2 | T019 | Map rendering | T011, T013 | 3 | IN PROGRESS | e560b60 MapLibre source and lifecycle | Device rotation/background/source selection |
| E04 | 2 | T020 | Map rendering | T019, T015 | 3 | IN PROGRESS | e560b60 local layer and hit state | Multi-layer styles and native hit-test device evidence |
| E04 | 2 | T021 | Map rendering | T020 | 3 | IN PROGRESS | 29380f4 cache/viewport tests PASS | MVT/LOD and 10k/100k/1M physical benchmark |
| E04 | 2 | T022 | Map rendering | T019, T016 | 3 | IN PROGRESS | 29380f4 MBTiles/PMTiles local tests PASS | Native renderer adapter and licensed download/resume/eviction |
| E05 | 2 | T023 | Native file IO | T011, T016 | 3 | IN PROGRESS | de11161 picker/ZIP/companion tests PASS | OS share-in, UI cancellation and hostile archive coverage |
| E05 | 2 | T024 | Native file IO | T023 | 3 | IN PROGRESS | 29380f4 strict GeoJSON local roundtrip PASS | GDAL independent oracle and GPKG/GPX/SHP/TAB/MIF/KML adapters |
| E05 | 2 | T025 | Native file IO | T023 | 3 | IN PROGRESS | 29380f4 CSV explicit columns and CRS PASS | XLSX and DXF, UI mapping and loss oracle |
| E05 | 2 | T026 | Native file IO | T024, T025 | 3 | IN PROGRESS | e560b60 bounded GeoJSON export/share | More formats, staged cleanup and independent GDAL/QGIS oracle |
| E06 | 2 | T027 | CRS | T011, T003 | 3 | IN PROGRESS | 29380f4 shared CRS control suite PASS | Pin verified PROJ DB/grids, device tests |
| E06 | 2 | T028 | CRS | T027, T005 | 3 | IN PROGRESS | 29380f4 VN-2000 same-datum oracle PASS | Regional provenance review/device qualification |
| E06 | 2 | T029 | CRS | T027 | 3 | IN PROGRESS | 29380f4 GeographicLib oracle PASS | Projected area/bearing profiles and device integration |
| E05 | 2 | T030 | Native file IO | T023, T027 | 3 | IN PROGRESS | 67cc9ec bounded GeoTIFF ABI | Build/runtime results, FFI, COG and hostile decoder evidence |
| E16 | 2 | T031 | Security/QA/release | T021, T022, T026, T028, T029, T030 | 3 | IN PROGRESS | 46169490774 targeted tests PASS | Full format and physical device acceptance; prior gates open |
| E07 | 3 | T032 | GIS editing | T020, T017, T027 | 3 | IN PROGRESS | b370310 topology and draft test PASS | Physical map preview/commit and touch tests |
| E07 | 3 | T033 | GIS editing | T032, T021, T029 | 3 | IN PROGRESS | b370310 indexed snap test PASS | Touch handles, stale race and physical p95 |
| E07 | 3 | T034 | GIS editing | T032, T017 | 3 | IN PROGRESS | b370310 persistent history PASS; 2acf58d groups | Latest split/merge test, crash injection |
| E07 | 3 | T035 | GIS editing | T032, T015 | 3 | IN PROGRESS | b370310 typed schema PASS; 2acf58d editor | Typed form UI, constraint and virtual table acceptance |
| E09 | 3 | T036 | Field survey | T013, T029, T015 | 3 | IN PROGRESS | b370310 OS foreground GPS UI | Physical recording, denial and bearing UI |
| E09 | 3 | T037 | Field survey | T036, T017 | 3 | IN PROGRESS | b370310 track journal and GPX test PASS | Background OS service/screen-off, independent GPX oracle |
| E09 | 3 | T038 | Field survey | T013, T016 | 3 | IN PROGRESS | 7d00cfd scoped encrypted blob vault | Camera/QR consent UI, OS keys, committed queue and cleanup |
| E09 | 3 | T039 | Field survey | T036, T028 | 3 | IN PROGRESS | b370310 GGA checksum/fix test PASS | NTRIP, transport, real receiver, correction loss |
| E08 | 3 | T040 | Storage | T016, T017, T022, T038 | 3 | IN PROGRESS | 7d00cfd encrypted blob candidate only | Encrypted DB, key wrapping, WAL/temp/backup, lock lifecycle |
| E16 | 3 | T041 | Security/QA/release | T031, T033, T034, T035, T037, T038, T040 | 3 | IN PROGRESS | 46170034589 focused native PASS | A1 blocked: prior gates, APK and device workflow |
| E10 | 4 | T042 | Dart SDK/auth/sync | T006, T008 | 3 | IN PROGRESS | 99e2cb5 Dart SDK targeted tests PASS | Generated OpenAPI/version negotiation/diff guard |
| E10 | 4 | T043 | Dart SDK/auth/sync | T042, T006 | 3 | IN PROGRESS | 5dc552f candidate credential repository; PKCE SDK PASS | Reviewed migration/mounted auth, OS keychain and full DB tests |
| E10 | 4 | T044 | Dart SDK/auth/sync | T043, T014 | 3 | IN PROGRESS | 99e2cb5 SDK scope/origin checks PASS | Connected UI/discovery and secure scope teardown |
| E10 | 4 | T045 | Dart SDK/auth/sync | T044, T040 | 3 | IN PROGRESS | f9f2b87 encrypted snapshot/lease candidate | Signed server package download, multi-page staging and disk-full |
| E10 | 4 | T046 | Dart SDK/auth/sync | T045, T017 | 3 | IN PROGRESS | d57a2a9 R029 delta transactional candidate | Runner, server transport and rebase |
| E10 | 4 | T047 | Dart SDK/auth/sync | T046 | 3 | IN PROGRESS | f9f2b87 outbox/receipt and 100 local recovery fixtures | Runner, conflict UI/manual rebase, real server unknown ACK |
| E10 | 4 | T048 | Dart SDK/auth/sync | T045, T043 | 3 | IN PROGRESS | d57a2a9 signed lease and connection closure candidate | OS key wrap, trusted clocks, device/reboot, server signing/revoke |
| E10 | 4 | T049 | Dart SDK/auth/sync | T044, T046 | 3 | IN PROGRESS | 99e2cb5 bounded SSE parser test PASS | Actual scoped stream/reconnect/revoke |
| E16 | 4 | T050 | Security/QA/release | T047, T048, T049, T041 | 3 | IN PROGRESS | d3f3128 isolated identity PG tests queued | A2 prior gates, PostGIS/Web concurrent edits, actual device |
| E11 | 5 | T051 | Plugin runtime | T007, T042 | 3 | WAITING | — | Await accepted dependencies |
| E11 | 5 | T052 | Plugin runtime | T051, T035 | 3 | WAITING | — | Await accepted dependencies |
| E11 | 5 | T053 | Plugin runtime | T052, T048 | 3 | WAITING | — | Await accepted dependencies |
| E11 | 5 | T054 | Plugin runtime | T051, T053 | 3 | WAITING | — | Await accepted dependencies |
| E11 | 5 | T055 | Plugin runtime | T052, T053 | 3 | WAITING | — | Await accepted dependencies |
| E12 | 5 | T056 | Reference plugins | T053, T038 | 3 | WAITING | — | Await accepted dependencies |
| E12 | 5 | T057 | Reference plugins | T056, T047 | 3 | WAITING | — | Await accepted dependencies |
| E12 | 5 | T058 | Reference plugins | T055, T054 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 5 | T059 | Security/QA/release | T057, T058, T054, T050 | 3 | WAITING | — | Await accepted dependencies |
| E13 | 6 | T060 | Processing | T029, T026, T017 | 3 | WAITING | — | Await accepted dependencies |
| E13 | 6 | T061 | Processing | T060 | 3 | WAITING | — | Await accepted dependencies |
| E13 | 6 | T062 | Processing | T061, T010 | 3 | WAITING | — | Await accepted dependencies |
| E13 | 6 | T063 | Processing | T021, T024 | 3 | WAITING | — | Await accepted dependencies |
| E13 | 6 | T064 | Processing | T030, T021, T005 | 3 | WAITING | — | Await accepted dependencies |
| E14 | 6 | T065 | Marketplace | T051, T043 | 3 | WAITING | — | Await accepted dependencies |
| E14 | 6 | T066 | Marketplace | T065, T048 | 3 | WAITING | — | Await accepted dependencies |
| E14 | 6 | T067 | Marketplace | T005, T065 | 3 | WAITING | — | Await accepted dependencies |
| E15 | 6 | T068 | Search/collaboration | T019, T036, T044 | 3 | WAITING | — | Await accepted dependencies |
| E15 | 6 | T069 | Search/collaboration | T049 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 6 | T070 | Security/QA/release | T062, T063, T064, T067, T068, T069 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T071 | Security/QA/release | T059, T070 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T072 | Security/QA/release | T059, T070 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T073 | Security/QA/release | T059, T070 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T074 | Security/QA/release | T071, T072, T073 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T075 | Security/QA/release | T074 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T076 | Security/QA/release | T075, T003 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T077 | Security/QA/release | T075 | 3 | WAITING | — | Await accepted dependencies |
| E16 | 7 | T078 | Security/QA/release | T076 | 3 | WAITING | — | Await accepted dependencies |

## Verified
- Matrix 78 parents and 238 leaves; 3 audit tests passed. Candidate Cargo graphs and metadata resolved for Android arm64 and iOS arm64; guard returned no forbidden packages for the five candidate roots. This is dependency resolution only, not cross-compilation or runtime proof.
- Artifact [15271903109](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46166872664/artifacts/15271903109), SHA-256 97e52c19ed60aac7c90b148c476374c1f14b1fdcb3654aea301ddac8c890244c. Size 179109 bytes. Artifact contents have not been independently reviewed.
- Existing unrelated workflows also report failure on the initial SPEC-only head. Investigate before full integrated acceptance; no check waived.
- VPS1 deploy-only, no verified coding agent process. GitHub-hosted runner used for audit. No production changes. No mobile toolchain/ABI/runtime or device qualification.
- Next after T001 acceptance: T002/T005/T006/T007; A0 remains open.

## W0 continued execution
- 08daa37 adds pinned candidate native and MapLibre qualification probes; initial job 46167398953 failed because sdkmanager was absent (ELF unit tests passed).
- c366b9d fixes runner SDK bootstrap. [Qualification run 46167470966](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46167470966) passed SDK/NDK setup for both jobs; native build and Flutter build in progress at snapshot.
- 07b6ae9 adds actual baseline auth and signature regression tests; [contract run 46167557596](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46167557596) queued at snapshot.
- Independent feasibility spikes allowed before A0; not a waiver of parent dependencies. No acceptance checkbox changed. No verified device, signed A0 review or release APK.

## Latest W0 evidence
- [Existing contract regression 46167557596](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46167557596) SUCCESS: mounted route inventory 1/1, credential isolation 2/2, extension validation 2/2. These are baseline contracts, not proof of mobile auth/lease or v2 runtime.
- Native compilation in run 46167470966 still active. MapLibre dependency resolution passed but analyzer failed for missing flutter_lints and deprecated Material import; ff52556 addresses both with explicit dependencies and new Material API. [Retry 46167650890](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46167650890) active.
- f3f032a implements real generated FRB qualification source with shared ovg-spatial transform, bounded batches, Rust and Dart tests; [FRB run 46167755969](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46167755969) queued. No bridge acceptance or native APK claimed.
- A0 remains blocked on native/bridge/device results, complete locked provenance and independent review. Physical Android 4KB/16KB and macOS device environment not verified. No task checkbox changed; qualification code and baseline tests are real partial progress.

## W1 source implementation (owner-requested early work)
See [W1 checkpoint](w1-implementation-checkpoint.md). W0 A0 not waived, no task formally accepted. Prior targeted native run passed five tests; latest strict XY and Flutter integration still need exact-candidate evidence.

## W2 source implementation
See [W2 checkpoint](w2-implementation-checkpoint.md). Targeted host native 25 tests PASS; prior gates and physical/render/format acceptance remain open. No mandatory checkbox changed.

## W3 source implementation
See [W3 checkpoint](w3-implementation-checkpoint.md). Focused host run passed 29 tests at b370310; later attachment/group/Dart tests pending. A1 and earlier gates remain open.

## W4 source implementation
See [W4 checkpoint](w4-implementation-checkpoint.md). Dart SDK targeted tests passed; encrypted host and identity integration pending. A0/A1/A2 remain open.
