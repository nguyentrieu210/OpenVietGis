# OpenVietGIS — Roadmap tổng thể

**Baseline**: 2026-11-18

**Định hướng**: [Product](docs/product.md)

**Nguyên tắc**: [Constitution](.specify/memory/constitution.md) · [ADR index](docs/adr/README.md)

**Cách lập**: [Spec Kit — spec of specs](https://github.com/github/spec-kit/blob/main/docs/how-to/spec-of-specs.md)

## Mục tiêu phát hành duy nhất: v1.0

**Quyết định 2026-11-24:** gộp toàn bộ phạm vi từng dự kiến cho v1.1 vào **v1.0 thống nhất**. Không duy trì một sản phẩm v1.0 rút gọn và một sản phẩm v1.1 song song. Desktop OAuth, đồng bộ QGIS/Web, offline, GIS nâng cao, CAD/3D, raster, AI/MCP, plugin, cộng tác, quản trị bản đồ nền và vận hành đều thuộc mục tiêu này.

Xem [Spec 030](specs/030-v1-unified-release/spec.md) · [Plan](specs/030-v1-unified-release/plan.md) · [Tasks](specs/030-v1-unified-release/tasks.md). **Đây là phạm vi phải hoàn thiện, không phải tuyên bố đã nghiệm thu.** Các bản đang chạy chỉ được thay thế sau khi ứng viên thống nhất vượt cổng release; giữ dữ liệu và bản rollback. Các ID lịch sử dưới đây vẫn dùng để truy vết kỹ thuật.

## Cách đọc

Đây là baseline phân rã sản phẩm thành các phần có đầu ra kiểm chứng được.
**Chỉ 001 và 002 đã có spec trong repo tại thời điểm ghi nhận.** Các nhãn 003–015
là thứ tự dự kiến cho nhóm năng lực, không khẳng định đã tạo 13 spec hay phải giữ
mỗi nhóm trong đúng một vòng triển khai.

- ID **R001–R015** là khóa truy vết ổn định, không đổi tên khi nhánh/spec được chia hoặc đổi số.
- 003 là ứng viên tiếp theo sau 002, không tự động được tạo bởi việc ghi roadmap.
- Số hàng không tạo phụ thuộc ngầm. Chỉ cột **Phụ thuộc** quyết định thứ tự bắt buộc.
- Nhóm quá lớn phải chia thành spec nhỏ hơn; giữ ID cha và bổ sung ID con cùng liên kết.
- Roadmap điều phối WHAT, ranh giới và phụ thuộc. Thiết kế kỹ thuật nằm trong plan/ADR.
- Tài liệu này không biến chức năng planned thành chức năng đang có.

**Trạng thái**: planned = chưa có spec chi tiết; in-progress = đang đặc tả/lập kế hoạch/
triển khai/kiểm chứng (ghi rõ giai đoạn); done = đã có bằng chứng nghiệm thu và convergence.
Mốc phát hành được đánh giá bằng kịch bản, không bằng số ô task hoặc tỷ lệ số spec.

## Các phần và phụ thuộc

| ID / nhãn dự kiến | Năng lực và đầu ra | Ranh giới phạm vi | Phụ thuộc | Trạng thái | Spec / bằng chứng |
| --- | --- | --- | --- | --- | --- |
| **R001 / 001** | Platform Foundation: khung chạy được, luồng persistence và quality gates | Dữ liệu mẫu dùng một lần; không GIS/tenant/business semantics | — | done | [Spec](specs/001-platform-foundation/spec.md) · [Kết quả](specs/001-platform-foundation/tasks.md#implementation-validation-results) · [Roadmap link](specs/001-platform-foundation/roadmap-reference.md) |
| **R002 / 002** | Identity, Organization & Tenancy: user/org, Workspace/Project, quyền, invite, service actor và bootstrap | GIS, public share, billing và triển khai enterprise thực tế nằm ngoài 002 | R001 | done (tích hợp nội bộ) — CI 44243951469 xanh, review/convergence 118/118; SC-013 hoãn đo thực tế đến trước nghiệm thu/phát hành bên ngoài | [Spec](specs/002-identity-organization-tenancy/spec.md) · [Roadmap link](specs/002-identity-organization-tenancy/roadmap-reference.md) |
| **R003 / 003** | Spatial Catalog & CRS: đăng ký nguồn/layer tenant-scoped, schema/extent/CRS/capability rõ, đọc được bộ dữ liệu tham chiếu | Layer khác SourceAsset/DB table; chưa làm bộ import đa định dạng hay editor hoàn chỉnh | R002 | done (internal integration) — 64/64 tasks, 9/9 measured SCs, independent review/convergence and full CI 44336830562; final exact CI required before main promotion | [Spec](specs/003-spatial-catalog-crs/spec.md), [Plan](specs/003-spatial-catalog-crs/plan.md), [Tasks](specs/003-spatial-catalog-crs/tasks.md) |
| **R004 / 004** | Map Workspace: mở Project, bản đồ nền, layer order/visibility, extent, chọn/xem feature và thuộc tính, lưu/mở lại cấu hình | Có đọc/query cơ bản và command/capability tối thiểu; query nâng cao ở R007, editor ở R006 | R003 | done (automated verified; T046/SC009 human observation open) | [Spec](specs/004-map-workspace/spec.md) |
| **R005 / 005** | Import & Export: đưa dữ liệu hỗ trợ vào Project và xuất kết quả để đối chiếu ở công cụ khác | Chốt ma trận định dạng/CRS; có provenance, change unit/revision, validation và recovery tối thiểu; CAD ở R012 | R003, R004 | done (automated verified; SC-007 2.38s/0.94s, GDAL 3.10.3; T010/SC-008 human observation open) | [Spec](specs/005-import-export/spec.md) |
| **R006 / 006** | Editing & Change History: vẽ/sửa hình học và thuộc tính, kiểm tra, commit/cancel, history và phát hiện stale revision | Mở rộng nền change của R003/R005; không giữ DB transaction theo phiên vẽ dài; CAD chính xác nâng cao ở R012 | R005 | done (automated verified) | [Spec](specs/006-editing-change-history/spec.md) |
| **R007 / 007** | Attribute & Spatial Query: lọc/tìm/selection nâng cao, kết hợp bảng và bản đồ, truy vấn không gian có semantics rõ | Chỉ các truy vấn và phép kết hợp được spec chọn; không trì hoãn read/select cơ bản của R004 | R004, R005 | done (automated verified) | [Spec](specs/007-attribute-spatial-query/spec.md) |
| **R008 / 008** | Processing & Jobs: thuật toán đăng ký, input/output, chạy và theo dõi tác vụ với kết quả kiểm chứng | Chọn ít thuật toán phục vụ workflow; job tối thiểu cần cho import có thể xuất hiện từ R005 | R005, R007 | done (automated verified — Buffer/Clip/Dissolve) | [Spec](specs/008-processing-jobs/spec.md) |
| **R009 / 009** | Cartography & Output: ký hiệu/nhãn, legend, bố cục và xuất bản đồ có tỷ lệ/CRS phù hợp | Style cơ bản thuộc phạm vi R004; không ép làm bộ layout ngang desktop ngay lần đầu | R004, R005 | done (automated verified — PNG/PDF 150/300 DPI) | [Spec](specs/009-cartography-output/spec.md) |
| **R010 / 010** | External Sources: raster, tile, dịch vụ GIS/OGC với capability và quyền rõ | Spec chọn từng adapter/protocol; không mặc định nguồn ngoài editable/offline; tile tối thiểu cần sớm không phải đợi nhóm này | R003, R004, R005 | done (automated verified — SSRF allowlist + read-only connectors) | [Spec](specs/010-external-sources/spec.md) |
| **R011 / 011** | Team Collaboration: nhiều người làm cùng Project với change/conflict và luồng phối hợp/duyệt được chọn | Tái dùng auth/grants R002; không đồng nghĩa phải có live cursors, CRDT hay public sharing | R002, R006, R007 | done (automated verified — reviews/comments/activity/rebase) | [Spec](specs/011-team-collaboration/spec.md) |
| **R012 / 012** | CAD Interoperability & Precision: trao đổi bản vẽ và thao tác chính xác phục vụ workflow GIS/CAD cụ thể | Chốt DXF/DWG read/write, entity mapping, đơn vị và mức bảo toàn; không hứa full AutoCAD hoặc DWG chỉ vì tên CAD | R005, R006, R007 | done (automated verified — ezdxf 2D DXF + COGO precision) | [Spec](specs/012-cad-precision/spec.md) |
| **R013 / 013** | Field & Offline: gói dữ liệu được phép, thao tác hiện trường, đồng bộ và xử lý xung đột/revocation | Scope offline phải giới hạn theo quyền/capability; đồng bộ ghi cần cơ chế conflict của Core | R005, R006, R011 | done (automated verified — GeoPackage bundle + outbox sync) | [Spec](specs/013-field-offline/spec.md) |
| **R014 / 014** | Module Framework & References: extension points/lifecycle, module nghiệp vụ mẫu tách Core | Ít nhất hai module khác nghiệp vụ kiểm chứng trước khi ổn định public SDK; không biến Số nhà thành Core | R006, R007 | done (automated verified — Core extension platform + 2 reference modules) | [Spec](specs/014-module-framework/spec.md) |
| **R015 / 015** | Production & Commercial Readiness: vận hành cấu hình phát hành, cloud/self-host parity; thương mại khi cần | Tách operations khỏi billing/entitlement; không bắt chờ CAD/field/modules để phát hành team GIS; không trì hoãn safety baseline | R002, R005, R006, R011 cho bản team có biên tập | done (automated verified — self-host stack + Prometheus + isolated backup/restore) | [Spec](specs/015-production-readiness/spec.md) |

Các phụ thuộc trên là baseline ở mức nhóm. Khi tạo spec nhỏ, phải ghi chính xác
contract/capability cần dùng, không chỉ viện dẫn cả một nhóm lớn. Nếu module tham
chiếu cần processing, thêm R008 vào phụ thuộc của slice đó trước khi mở spec.

## Audit độc lập R001–R015

[Spec 018 — Full Product Audit](specs/018-full-product-audit/spec.md) đánh giá toàn bộ các mục trên theo từng yêu cầu và bằng chứng chạy thực tế trên đúng candidate. Trạng thái `done` kế thừa trong bảng chưa phải kết luận của audit 018; không được xem các cờ capability tĩnh hoặc smoke test là nghiệm thu sản phẩm. Xem [ma trận sơ bộ](specs/018-full-product-audit/status-matrix.md) và [backlog nghi vấn](specs/018-full-product-audit/gaps.md). Không thay đổi phạm vi R001–R015 hoặc tự động phát hành.

**Trạng thái phát hành hiện tại — chưa nghiệm thu profile:** các nhãn `done` trong bảng là trạng thái triển khai lịch sử, không phải chứng nhận production của candidate hiện tại. [G006 release gate](specs/018-full-product-audit/remediation/G006-release-certification/evidence.md) tách 15 nhóm inventory khỏi số nhóm được nghiệm thu phát hành; [so sánh 016/017 và handoff nghiệm thu](specs/018-full-product-audit/remediation/release-convergence.md) ghi các gate còn mở. Không chuyển trạng thái M1/M2/M3 hoặc quảng bá capability release khi thiếu bằng chứng cùng candidate, profile và review.

## Mốc nghiệm thu

| Mốc | Phạm vi tối thiểu | Kịch bản phải chứng minh | Trạng thái |
| --- | --- | --- | --- |
| **M0 — Nền** | R001 | Bootstrap, web/API/persistence, guard kiến trúc và kiểm thử có bằng chứng; xem kết quả 001 | done theo evidence 001 |
| **M1 — Dùng thử GIS local** | R002–R005 | Đăng nhập/chọn đúng tổ chức; tạo Project; nhập dữ liệu thuộc ma trận hỗ trợ; xem layer, chọn feature và thuộc tính; lưu/mở lại; xuất và đối chiếu count/schema/CRS/geometry theo tolerance đã chốt; kiểm tra từ chối truy cập chéo tenant | planned |
| **M2 — Làm việc thực tế** | M1 + R006, R007, R009 | Hoàn thành một workflow tham chiếu bằng Core: sửa dữ liệu, xử lý stale revision, truy vấn, truy vết thay đổi và xuất đầu ra bản đồ; benchmark theo dataset/môi trường được ghi trước khi thực hiện | planned |
| **M3 — Phát hành cho team** | M1 + R006, R007, R011 + operations slice R015 | Hai thành viên có quyền khác nhau cùng làm Project; revocation và concurrent change xử lý đúng; triển khai/upgrade, backup/restore và recovery được diễn tập trên profile phát hành; capability M2 nào được phát hành cũng phải đạt gate M2 tương ứng | planned |
| **Nhánh nâng cao** | R008, R010, R012–R014 theo workflow; commercial slice R015 khi bán dịch vụ | Mỗi nhánh có spec và gate riêng; không dùng tên sản phẩm desktop để thay tiêu chí nghiệm thu | planned |

Mốc M3 không tự bao gồm mọi nhánh nâng cao và không bắt buộc billing. Bản phân tích
chuyên môn cần thêm R008; bản có CAD cần R012. Phải ghi rõ cấu hình capability thực
sự phát hành thay vì coi một mốc là tất cả khả năng dự kiến.

## Các yêu cầu phải xuất hiện sớm

1. **Tenant/actor/permission** theo R002 phải đi xuyên catalog, feature, import/export,
   render, job, artifact, cache, module và đồng bộ ngay khi các đường đó xuất hiện.
2. **CRS và correctness** bắt đầu ở R003, được kiểm chứng lại cho mỗi adapter/transform.
   Không tự chọn storage CRS hoặc tham số VN-2000 bằng roadmap này.
3. **Change/revision/provenance** phải có ngay tại material mutation đầu tiên;
   import không được chờ tới R006 mới có lịch sử/actor và recovery.
4. **Read/select/query và style cơ bản** nằm ở R003/R004; R007/R009 mở rộng khả năng này.
5. **Command/capability contracts** xuất hiện cùng công cụ đầu tiên ở R004;
   R014 mở rộng framework đăng ký module thay vì giới thiệu command contract quá muộn.
6. **Object storage/jobs/tile delivery** được đưa vào khi workflow đầu tiên thực sự cần,
   theo ADR và contract, không tạo service rỗng cũng không trì hoãn vì số roadmap.
7. **Migration, backup/recovery, log, secrets, resource limits và performance budgets**
   là nghĩa vụ của từng tính năng. R015 hoàn thiện vận hành/release, không bắt đầu
   toàn bộ công việc an toàn.

## Chia việc cho agent

- Tiếp tục R002 theo spec/plan hiện có; roadmap không thay đổi ngầm phạm vi 002.
- Trong lúc R002 chưa nghiệm thu, chỉ research/đặc tả phần phụ thuộc trên contract
  được ghi rõ là đã Accepted hay còn draft. Không tuyên bố downstream implementation
  đã ổn định trên một nền đang thay đổi.
- Khi prerequisite đã chốt và có commit nền phù hợp, tách worktree/nhánh riêng cho
  mỗi agent hoặc feature. Giữ active-feature state riêng; checkout nhánh không đủ
  để bảo đảm Spec Kit đang trỏ đúng feature.
- Chốt contract, error semantics và data ownership trước khi FE/BE triển khai độc lập.
- Có thể song song FE–BE–test của cùng slice sau contract; R006/R007/R009/R010 có
  thể chia các luồng sau R005 vì không có phụ thuộc bắt buộc giữa các nhóm đó.
- Thay đổi contract chung, schema hoặc manifest do một đầu mối điều phối.
  Một agent tích hợp đối chiếu diff, migrations, tests và traceability trước khi gộp.
- Chỉ gọi capability done sau Spec Kit convergence và review; planned/spec-ready
  không có nghĩa implementation đã hoàn tất.

## Truy vết và cập nhật

1. Trước khi mở spec, đọc [Product](docs/product.md), roadmap entry và ADR liên quan.
   Chốt ranh giới, prerequisites, benchmark và kịch bản nghiệm thu của slice.
2. Spec mới ghi trong Input/summary: **Parent roadmap: ROADMAP.md, entry Rxxx** và
   liên kết Markdown tới anchor tương ứng. Roadmap phải liên kết trở lại spec thật.
3. 001–002 có companion `roadmap-reference.md` để thêm truy vết mà không sửa nội dung
   spec/plan đang được thực hiện. Không viết lại yêu cầu lịch sử chỉ để khớp roadmap.
4. Cập nhật roadmap trước khi thay cách phân rã; đối chiếu spec/ADR/contract bị ảnh
   hưởng. Constitution và Accepted ADR chỉ được đổi theo quy trình governance.
5. Nếu tách một nhóm, giữ ID cha, thêm ID con, dependencies, scope, trạng thái và link.
   Nhãn số 003–015 có thể đổi; không tái sử dụng ID để gán sang năng lực khác.
6. Ghi trạng thái kèm giai đoạn và bằng chứng. Chỉ chuyển done khi các slice bắt buộc
   đã nghiệm thu; không tính số task thành phần trăm hoàn thành toàn sản phẩm.
7. Trước khi mở spec tiếp theo, resolve các quyết định đến hạn trong
   [Product decision register](docs/product.md#các-quyết-định-phải-được-chốt-ở-spec-liên-quan).
   Không coi những đề xuất định dạng/engine/CRS chưa chốt là quyết định đã Accepted.

## Mốc truy vết ổn định

### R001

[Platform Foundation](specs/001-platform-foundation/spec.md).
Bằng chứng và convergence: [tasks](specs/001-platform-foundation/tasks.md#implementation-validation-results).
Dữ liệu mẫu của 001 không trở thành Project, Layer hay mô hình nghiệp vụ thật.

### R002

[Identity, Organization & Tenancy](specs/002-identity-organization-tenancy/spec.md).
Đang triển khai P1 trên nhánh takeover/002-identity-tenancy; [checkpoint](specs/002-identity-organization-tenancy/quickstart.md#21-p1-independent-hardening-checkpoint--2026-11-18) ghi 173 kiểm thử Python, 66 integration + performance, P1 audit/log, CI chuẩn bị, backup/restore và các bản sửa frontend; chưa qua P1 browser gate hoặc convergence. Chưa tích hợp vào main. Phạm vi và tiến độ chi tiết do artifacts 002 quyết định.

### R003

Spatial Catalog & CRS: implemented and converged for internal integration on verified R002 main. All 64 tasks and 9 SCs passed: scoped source/Layer metadata and immutable history, pinned typed feature inspection, six CRS definitions/eight explicit control-verified operations, real RLS/lifecycle/credential boundaries, restart/atomic retry, 10k measured HTTP benchmark and real browser workflows. [Final evidence](specs/003-spatial-catalog-crs/quickstart.md#final-runtime-acceptance--2026-11-18), [measured report](specs/003-spatial-catalog-crs/evidence/ci-44336830562.json). Exact runtime ea18f13 CI 44336830562 all three jobs green, independent review/convergence no blockers; final documentation commit requires its own full green CI before main promotion. No deployment; R002 SC-013 remains mandatory before external release.

### R004

Map Workspace.

- **R004-A — Unified Spatial Workspace**: [Spec 022](specs/022-spatial-workspace/spec.md). In progress (Spec Kit; isolated candidate). Dependencies: R002, R003, R004, R006, R012; integrate after Spec 020 candidate is reviewed and committed. Shared project/session, capability-aware GIS/CAD/3D mode switching, safe draft handling and lazy engine lifecycle. No new CAD or 3D engine promised by this slice.
- **R012-A — CAD Bridge** (planned): authorized linking and explicit georeferencing of native CAD assets; depends on R004-A, R003, R005, R012.
- **R004-B — 3D Viewer** (planned): actual geographic 3D engine adapter, bounded first workflow using project data; depends on R004-A, R003 and R010. Terrain/model support follows measured acceptance; not part of R004-A.


### R005

Import & Export.

### R006

Editing & Change History.

### R007

Attribute & Spatial Query.

### R008

Processing & Jobs.

### R009

Cartography & Output.

### R010

External Sources.

Child [R026 — Integration Hub & Policy Gate](specs/026-integration-hub-policy/spec.md): project-scoped, read-only adapter catalog and deny-by-default action policy. Slice 1 only; desktop sync, credential binding and production deployment remain gated.

Child [R027 — Bidirectional Integration Sync](specs/027-bidirectional-integration-sync/spec.md): Spec Kit initialized (spec/research/plan/data model/API draft/tasks) for QGIS-first approved two-way synchronization, durable identity/cursor/inbox-outbox, ACL/CRS, conflicts and rollback. **Foundation Slice A implementation candidate exists on the feature branch** (disabled QGIS binding registry + pure three-way diff; see [evidence](specs/027-bidirectional-integration-sync/evidence.md)); no live write connector or automatic two-way sync implied. Depends on R026, R006, R002, R003, R010/R013. [Plan](specs/027-bidirectional-integration-sync/plan.md) | [Tasks](specs/027-bidirectional-integration-sync/tasks.md).

### R011

Team Collaboration.

- [R011-2 — Project Chat & Tasks](specs/024-team-chat-tasks/spec.md): project messages, assigned tasks and map pins; depends on R002/R011 and integrated GIS ACL candidate. Implementation candidate; release validation pending. [V4 feature edit leases](specs/024-team-chat-tasks/v4-feature-locks.md) extends R006 changeset conflict protection with renewable per-feature locks; [V5 feature discussion](specs/024-team-chat-tasks/v5-feature-discussion.md) adds comments, mentions and feature-linked tasks.

### R012

CAD Interoperability & Precision.

### R013

Field & Offline.

### R014

Module Framework & References.

Child [Spec 019 — House Numbering Plugin](specs/019-house-numbering-plugin/spec.md): isolated technical candidate implemented and automated-verified across dynamic module UI, PostGIS workflows, browser/mobile, security, 10k/100k performance and DB+artifact recovery. [Plan](specs/019-house-numbering-plugin/plan.md) | [Tasks](specs/019-house-numbering-plugin/tasks.md) | [Convergence evidence](specs/019-house-numbering-plugin/evidence/convergence.md).

Child [Spec 051 — Số nhà đúng nghiệp vụ Bình Hưng và thuận tay người dùng](specs/051-house-numbering-parity/spec.md): draft 2026-11-25, chờ chủ dự án duyệt; sửa nghiệp vụ và trải nghiệm của plugin 019 theo app Bình Hưng đang chạy. Chưa có plan/tasks.

Child [Spec 021 — AI Copilot / MCP](specs/021-ai-copilot/spec.md): isolated MCP adapter, live bridge and remote OAuth candidate, 27 automated tests plus Partner SDK/first-party browser acceptance (R002, R003, R004, R014); real Keycloak/Claude OAuth exchange, merged frontend integration, real-project verification and production release remain open. [Plan](specs/021-ai-copilot/plan.md) | [Tasks](specs/021-ai-copilot/tasks.md).

### R015

Production & Commercial Readiness.

Cross-cutting [Spec 031 — Product inventory & audit](specs/031-product-inventory-audit/spec.md): static baseline and remediation backlog for Spec 030 unified v1.0; [audit evidence](specs/031-product-inventory-audit/AUDIT.md). Runtime acceptance remains open; prioritize repository/release consolidation before new features.


Cross-cutting [Spec 032 — Repository and release consolidation](specs/032-release-consolidation/spec.md): isolated unified candidate, ref inventory and artifact integrity; [findings](specs/032-release-consolidation/AUDIT.md). Live promotion and full runtime certification remain gated.


Cross-cutting [Spec 033 — Core architecture standardization](specs/033-core-standardization/spec.md): enforce domain/adapter and package/app boundaries; isolate legacy workbench bridge, generic module discovery and shared spatial math. [Evidence](specs/033-core-standardization/AUDIT.md); real parity acceptance remains open.


Cross-cutting [Spec 034 — Spatial data and CRS standardization](specs/034-spatial-data-standardization/spec.md): unify CRS registry ownership and enforce coherent native QGIS snapshots; [evidence and limits](specs/034-spatial-data-standardization/AUDIT.md). Full geodetic and native runtime acceptance remains open.


Cross-cutting [Spec 035 — Unified API and SDK](specs/035-unified-api-sdk/spec.md): single API transport base, managed QGIS URL fix, shared scoped paths and normalized error parsing; [evidence](specs/035-unified-api-sdk/AUDIT.md). Full contract coverage and live acceptance remain open.


Cross-cutting [Spec 036 — Identity and access control](specs/036-identity-access-control/spec.md): managed-session ownership, management authority, epoch checks and scoped delta fail-closed behavior; [evidence and limitations](specs/036-identity-access-control/AUDIT.md). Full cross-client security acceptance remains open.


Cross-cutting [Spec 037 — Unified GIS/CAD/3D workspace](specs/037-unified-spatial-workspace/spec.md): independent authorized mode entry, adapter lifecycle isolation and browser regression; [evidence](specs/037-unified-spatial-workspace/AUDIT.md). Real CAD/3D acceptance remains open.


Cross-cutting [Spec 038 — Design system and workspace UX](specs/038-design-system-ux/spec.md): compact/full tool dock, discoverable ribbon search, consistent command feedback and keyboard/mode isolation; [evidence](specs/038-design-system-ux/AUDIT.md). Wider design-system and user acceptance remain open.


Cross-cutting [Spec 039 — Unified Plugin Framework](specs/039-unified-plugin-framework/spec.md): exact official manifest/runtime pinning, activation validation and Core fallback; [evidence and rollout limits](specs/039-unified-plugin-framework/AUDIT.md). Existing tenant manifest compatibility requires reviewed pre-deploy audit.


Cross-cutting [Spec 040 — Connector & Realtime](specs/040-connector-realtime/spec.md): [scope, evidence and open gates](specs/040-connector-realtime/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 041 — AI & MCP](specs/041-ai-mcp/spec.md): [scope, evidence and open gates](specs/041-ai-mcp/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 042 — Quality Gate](specs/042-quality-gate/spec.md): [scope, evidence and open gates](specs/042-quality-gate/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 043 — Performance](specs/043-performance/spec.md): [scope, evidence and open gates](specs/043-performance/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 044 — Security](specs/044-security/spec.md): [scope, evidence and open gates](specs/044-security/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 045 — Release & Deployment](specs/045-release-deployment/spec.md): [scope, evidence and open gates](specs/045-release-deployment/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 046 — Documentation & Governance](specs/046-docs-governance/spec.md): [scope, evidence and open gates](specs/046-docs-governance/AUDIT.md). No automatic live promotion.


Cross-cutting [Spec 047 — Stable Acceptance](specs/047-stable-acceptance/spec.md): [scope, evidence and open gates](specs/047-stable-acceptance/AUDIT.md). No automatic live promotion.
