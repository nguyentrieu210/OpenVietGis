# OpenVietGIS

**Nền tảng WebGIS mã nguồn công khai, hướng tới kết nối GIS, CAD, dữ liệu không gian, cộng tác và AI trên một không gian làm việc thống nhất.**

[**Trải nghiệm bản demo trực tuyến**](https://openvietgis.cloud) · [**Mã nguồn GitHub**](https://github.com/nguyentrieu210/OpenVietGis) · [**Roadmap**](ROADMAP.md) · [**Định hướng sản phẩm**](docs/product.md)

> **Trạng thái:** Đang phát triển và mở cho cộng đồng trải nghiệm, góp ý, kiểm thử. Demo là môi trường dùng thử, không phải chứng nhận mọi tính năng đã sẵn sàng cho sản xuất. Không tải dữ liệu nhạy cảm lên bản demo.

## OpenVietGIS là gì?

OpenVietGIS là dự án xây dựng **không gian làm việc GIS trên trình duyệt**, lấy cảm hứng từ các luồng công việc hữu ích của QGIS, ArcGIS, MapInfo và AutoCAD nhưng không đặt mục tiêu sao chép toàn bộ các phần mềm này. Dự án được xây dựng với sự hỗ trợ của AI (AI-assisted development / Vibe Coding), đồng thời chú trọng kiến trúc, kiểm thử và sự đóng góp của cộng đồng.

Các nhóm năng lực đang phát triển và kiểm chứng:

- **WebGIS:** dự án, nguồn dữ liệu, lớp bản đồ, thuộc tính, truy vấn và biên tập.
- **Bản đồ chuyên đề:** ký hiệu, nhãn, bố cục in và xuất kết quả.
- **Xử lý dữ liệu không gian:** CRS, nhập/xuất, phân tích và các luồng xử lý.
- **CAD và các nguồn bên ngoài:** trao đổi dữ liệu, kết nối hệ thống và khả năng mở rộng.
- **Cộng tác:** tổ chức, workspace, phân quyền, phân tách dữ liệu và lịch sử thao tác.
- **Plugin, API và AI:** các hướng tích hợp/mở rộng đang được phát triển, thử nghiệm và đánh giá.

**Lưu ý:** Danh sách trên mô tả *phạm vi phát triển*, không có nghĩa mọi nhóm tính năng đều đã hoàn tất hoặc được nghiệm thu. Xem [roadmap](ROADMAP.md), [đặc tả](specs/) và [bằng chứng kiểm thử](specs/018-full-product-audit/remediation/final-next-integration-evidence.md) để xác minh từng năng lực.

## Thử demo và cùng đóng góp

1. Mở **https://openvietgis.cloud** để trải nghiệm phiên bản đang triển khai.
2. Nếu gặp lỗi, có đề xuất cải tiến hoặc muốn phát triển thêm plugin, hãy [tạo Issue](https://github.com/nguyentrieu210/OpenVietGis/issues).
3. Muốn đóng góp mã nguồn, hãy Fork repo, tạo nhánh riêng và gửi Pull Request kèm mô tả thay đổi, kiểm thử và ảnh minh họa nếu liên quan giao diện.
4. Trước khi sửa Core, vui lòng đọc [kiến trúc](docs/architecture.md), [quy tắc dự án](.specify/memory/constitution.md) và [roadmap](ROADMAP.md).

Rất hoan nghênh cộng đồng **Vibe Coding, GIS, trắc địa, địa chính, CAD, frontend/backend và AI Agent** tham gia góp ý và phát triển cùng nhau. Mã nguồn đang được công khai; điều khoản cấp phép sử dụng và phân phối cần được xác định rõ trong tệp LICENSE trước khi sử dụng lại theo một giấy phép mã nguồn mở cụ thể.

---

## Technical documentation (English)

OpenVietGIS is a platform-first spatial workspace for the web. The original Spec 001 built the
architecture runway and a disposable non-GIS vertical slice; later feature slices extend that
foundation.

## Product direction and roadmap

- [Product direction and release outcomes](docs/product.md)
- [Product roadmap, dependencies, milestones, and feature status](ROADMAP.md)
- Foundation: [Spec 001](specs/001-platform-foundation/spec.md), roadmap entry [R001](ROADMAP.md#r001)
- Identity/organization/tenancy: [Spec 002](specs/002-identity-organization-tenancy/spec.md), roadmap entry [R002](ROADMAP.md#r002)

The original roadmap baseline began with 001 and 002. This candidate also contains R003–R017
specifications and implementation slices, plus the R018 product audit/remediation. Consult the
[requirement inventory](specs/018-full-product-audit/evidence/requirements.md),
[reconciliation index](specs/018-full-product-audit/remediation/requirement-evidence-reconciliation.md)
and [integrated evidence](specs/018-full-product-audit/remediation/final-next-integration-evidence.md)
for tested capability boundaries and remaining gates. Task checkboxes and source presence alone
do not establish full requirement acceptance or production release certification. Before opening
another feature, read the product direction and roadmap entry and maintain both directions of
the roadmap/spec links.

## Governance and architecture

- Constitution: `.specify/memory/constitution.md`
- ADR index: `docs/adr/README.md`
- Architecture boundaries: `docs/architecture.md`
- Accepted foundation specification and evidence: `specs/001-platform-foundation/`

Core/platform code must never depend on a vertical domain module. The foundation must build and run
when `modules/` is empty or absent.

## Supported development baseline

- Node.js 26 LTS
- pnpm 12.21 (installed explicitly; Node 26 no longer bundles Corepack)
- Python 3.13 managed by `uv`
- Docker with Compose
- PostgreSQL 18 + PostGIS 3.7 through the development compose profile

The current local machine may use another Node version for exploratory work, but accepted CI uses
Node 26 and is the authoritative runtime gate.

## Bootstrap

From the repository root, copy the non-secret environment template to `.env` only when there
is no existing local configuration. On PowerShell use `Copy-Item .env.example .env`; on Bash
use `cp .env.example .env`. Then run:

```sh
npm install --global pnpm@12.21.0
pnpm install --frozen-lockfile
uv python install 3.13
uv sync --locked --project apps/api
docker compose -f infra/dev/compose.yaml up -d postgres postgres-test
uv run --project apps/api alembic -c apps/api/alembic.ini upgrade head
pnpm exec playwright install chromium
```

No production credential or production data is required.

## Run locally

API:

```powershell
pnpm dev:api
```

Web shell in another terminal:

```powershell
pnpm dev:web
```

Open `http://127.0.0.1:5173`.

The shell exposes real identity, Organization, Workspace and Project flows, with scoped access,
invitation, service identity, and audit contracts. Foundation readiness remains at
`/api/v1/health/ready`. The disposable Spec 001 sample route, SDK, UI and table are retired by
migration `0005_retire_foundation_sample`; existing sample-only records are intentionally removed.
The historical Spec 001 evidence remains unchanged. Create/read/restart and browser persistence
regressions now exercise an authenticated tenant Project instead.

The current candidate also contains the scoped spatial catalog, map, editing, query, processing,
cartography, external sources, collaboration, CAD, offline and module framework workflows.
Supported formats, capabilities, constraints and acceptance evidence are in their individual
specifications and R018 remediation evidence; the candidate is not certified for external release.
Core does not depend on the reference vertical modules under `modules/`.

## Identity and tenancy development profile

Spec 002 adds a real OIDC reference profile for development and E2E validation. Keycloak is a
reference provider only; OpenVietGIS product identity remains provider-neutral.

Start PostgreSQL plus the synthetic Keycloak realm:

```powershell
pnpm dev:identity
```

Reference endpoints and identities:

- Keycloak: `http://127.0.0.1:18080`
- Realm: `openvietgis`
- Synthetic users: `owner-a`, `member-a`, `member-b`
- All fixture credentials are development/test-only and live in
  `infra/dev/keycloak/realm-openvietgis.json`.
- Copy the current `.env.example` to ignored `.env` before running the API.

The application uses OIDC only for human authentication. Organization membership, scoped
authorization, tenant isolation, service principals, and audit semantics are owned by OpenVietGIS.
Provider tokens must not be stored in browser local/session storage.

The complete security/tenancy acceptance flow is maintained in
`specs/002-identity-organization-tenancy/quickstart.md`.

## Contract generation

Spec 002 OpenAPI contracts generate TypeScript types into the client SDK:

```powershell
pnpm contracts:generate
pnpm validate:contracts
```

Generated files are committed so contract drift is visible in code review.

## Baseline validation

With the development and dedicated test PostGIS services running:

```powershell
pnpm validate
```

The command covers:
1. TypeScript/JavaScript static quality and unit tests.
2. Python static quality, unit tests, and contract checks.
3. Architecture dependency rules.
4. Documentation navigation.
5. Real PostgreSQL/PostGIS migration and integration tests.
6. Browser E2E through the web/API/persistence path.

CI runs the same logical validation path on Node 26 with real PostGIS.

Integration and browser tests use the dedicated local `openvietgis_test` database on port
`54330`. They ignore the application `OPENVIETGIS_DATABASE_URL`; an optional exported
`OPENVIETGIS_TEST_DATABASE_URL` must still name `openvietgis_test` on localhost/127.0.0.1.
URL query parameters and other drivers are rejected before migrations or child processes run.
Destructive migration-cycle tests create fresh disposable loopback PostGIS databases rather than
downgrading shared test history; never store working data in any test database. Development
application data remains on port `54329`.

## Safe development reset

```powershell
pnpm dev:reset
```

The reset command refuses:
- environments other than `development` or `test`;
- non-local database hosts;
- database names other than exactly `openvietgis`, `openvietgis_dev`, or `openvietgis_test`.

It attempts to downgrade and reapply migrations for the configured local development/test
database. A downgrade can intentionally refuse to discard retained extension histories (migration
0019); that refusal is a safety guard, not a reset mechanism to bypass. Never point development or
migration configuration at a production database.

## Acceptance measurements

Run the scenarios in `specs/001-platform-foundation/quickstart.md`.

Two measurements require explicit evidence:
- clean supported-environment bootstrap: at most 20 minutes excluding separately recorded download time;
- repository navigation: an unfamiliar reviewer must locate governance/architecture/spec artifacts
  in under 10 minutes without assistance.

## Troubleshooting

### Node engine warning

If pnpm reports an unsupported engine, install Node.js 26 LTS. CI does not accept older Node
versions even if local exploratory commands happen to work.

### Port already in use

Development defaults:
- web: `5173`
- API: `8000`
- PostGIS development host port: `54329`
- PostGIS dedicated test host port: `54330`

Stop the conflicting local process or intentionally change the development-only configuration.

### PostGIS unavailable

Check:

```powershell
docker compose -f infra/dev/compose.yaml ps
```

Start it with:

```powershell
docker compose -f infra/dev/compose.yaml up -d postgres postgres-test
```

### Schema not migrated

Run:

```powershell
pnpm db:migrate
```

### Missing configuration

Copy `.env.example` to the ignored local `.env`. The API intentionally fails fast when required
configuration is absent.