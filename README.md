# OpenVietGIS

**Nền tảng WebGIS mã nguồn mở dành cho bản đồ, dữ liệu không gian, cộng tác và tích hợp GIS/CAD/AI.**

[Trải nghiệm demo](https://openvietgis.cloud) · [GitHub](https://github.com/nguyentrieu210/OpenVietGis) · [Roadmap](ROADMAP.md) · [Đóng góp](CONTRIBUTING.md) · [Giấy phép AGPL](LICENSE)

> [!IMPORTANT]
> **Trạng thái: public development candidate — chưa được nghiệm thu để tuyên bố sẵn sàng production.** Demo phục vụ trải nghiệm và phản hồi, không phải chứng nhận mọi tính năng đã hoàn thiện. Không tải dữ liệu địa chính thật, dữ liệu cá nhân, thông tin nhạy cảm hoặc bí mật lên demo. Không thử ghi/xóa, quét bảo mật hoặc pentest production khi chưa được ủy quyền.

## OpenVietGIS là gì?

OpenVietGIS là **dự án phần mềm GIS độc lập**, hướng đến không gian làm việc chung trên trình duyệt: tổ chức dự án, nhập và xem dữ liệu không gian, quản lý layer, biên tập và phân tích, trình bày bản đồ, chia sẻ kết quả và kết nối những công cụ khác trong phạm vi quyền được cấp.

Dự án học hỏi các **workflow** hữu ích từ QGIS, MapInfo, ArcGIS và AutoCAD, nhưng **không phải sản phẩm trực thuộc, bản sao hay bản thay thế đã được chứng nhận** của bất kỳ nhà cung cấp nào. Tên và nhãn hiệu của bên thứ ba thuộc chủ sở hữu tương ứng.

Dự án có sử dụng **AI-assisted development/Vibe Coding** trong quá trình phát triển. AI hỗ trợ triển khai, không thay thế trách nhiệm kiểm thử, kiểm soát chất lượng, bảo mật và tuân thủ pháp luật.

### Các nhóm năng lực đang phát triển

| Lĩnh vực | Hướng năng lực | Giới hạn công bố |
| --- | --- | --- |
| Workspace & bản đồ | Organization, Workspace, Project, layer, bản đồ nền, thuộc tính, truy vấn | Mức hỗ trợ tùy bản build và quyền |
| Dữ liệu GIS | CRS, nhập/xuất, GeoJSON, GeoPackage, Shapefile, PostGIS theo ma trận hỗ trợ | Không mặc định hỗ trợ mọi định dạng, CRS hoặc bảo toàn hoàn toàn dữ liệu |
| Biên tập | Sửa hình học/thuộc tính, revision, history, conflict | Chỉ nguồn có quyền ghi và adapter hỗ trợ |
| Cartography & processing | Ký hiệu, nhãn, layout, PDF/PNG, tác vụ phân tích | Độ chính xác, output và hiệu năng cần kiểm chứng bằng dataset cụ thể |
| Cộng tác | Phân quyền, project-scoped data, kiểm toán, phối hợp | Authorization phải được thực thi ở backend, không chỉ UI |
| Plugin & tích hợp | GIS/OGC/CAD, API, dữ liệu ngoài, AI/MCP, cầu nối client | Không đồng nghĩa đã kết nối được mọi phần mềm hay có quyền truy cập hệ thống khác |
| Hiện trường & đồng bộ | Offline, hàng đợi thay đổi, xử lý xung đột, hướng client realtime | Không mặc định đã hỗ trợ realtime hai chiều cho mọi nguồn |

**Đây là phạm vi sản phẩm, không phải danh sách đã nghiệm thu.** Muốn xác minh, xem [ROADMAP.md](ROADMAP.md), [đặc tả](specs/), [audit R018](specs/018-full-product-audit/) và bằng chứng theo **commit SHA + môi trường + quyền + dữ liệu**. Một nút trên giao diện, một test xanh hay một spec có trạng thái done không đủ để khẳng định sẵn sàng production. Các hướng như [AI Copilot](specs/021-ai-copilot/), [đồng bộ tích hợp hai chiều](specs/027-bidirectional-integration-sync/) và [GIS client quản lý realtime](specs/029-managed-realtime-gis-client/) phải được đánh giá theo từng bản phát hành.

## Ranh giới pháp lý: phần mềm GIS khác dữ liệu địa chính chính thức

**OpenVietGIS là công cụ phần mềm, không phải cơ sở dữ liệu đất đai nhà nước và không trao quyền truy cập/sửa đổi dữ liệu mà người sử dụng chưa được cấp.**

| Công cụ có thể hỗ trợ | Không được tự suy là có quyền hoặc giá trị pháp lý |
| --- | --- |
| Xem, phân tích, nhập/xuất và biên tập dữ liệu được phép sử dụng | Truy cập, cập nhật cơ sở dữ liệu địa chính chính thức |
| Tạo bản đồ chuyên đề, bản đồ nội bộ, sản phẩm phân tích | Xác lập quyền sử dụng đất, chứng nhận địa giới hoặc thay thế hồ sơ pháp lý |
| Kết nối qua API, plugin, OGC, CAD, AI/MCP | Bẻ khóa phần mềm, vượt xác thực, trích xuất dữ liệu hạn chế |
| Sinh báo cáo/dự thảo và hỗ trợ người dùng ra quyết định | Tự phê duyệt hồ sơ, cấp số nhà hoặc quyết định hành chính |

### Nguyên tắc sử dụng có trách nhiệm

1. **Quyền dữ liệu:** Chỉ thu thập, truy cập, nhập, sao chép, sửa, chia sẻ hoặc xuất bản khi có quyền hợp lệ. Dữ liệu có thể xem trên Internet không mặc nhiên được phép tái phân phối.
2. **Địa chính, bản đồ chuyên ngành:** Không công khai hay chỉnh sửa dữ liệu chính thức của cơ quan, tổ chức nếu không có thẩm quyền. Hồ sơ hành chính, bản đồ địa chính và tài liệu do công cụ xuất ra không tự động có giá trị chứng thực.
3. **Dữ liệu cá nhân/bảo mật:** Không đưa thông tin chủ đất, số giấy tờ, địa chỉ nhạy cảm, tọa độ gắn với cá nhân, dữ liệu nội bộ hay bí mật nhà nước lên demo công khai. Bên xử lý dữ liệu cần đánh giá căn cứ, biện pháp bảo vệ và nghĩa vụ pháp lý áp dụng.
4. **Chủ quyền, lãnh thổ:** Khi công bố bản đồ, cần kiểm tra nội dung thể hiện chủ quyền, biên giới, địa giới và dữ liệu nền theo quy định áp dụng.
5. **Dịch vụ có điều kiện:** Phát triển công cụ phần mềm không đồng nhất với việc trực tiếp kinh doanh dịch vụ đo đạc/thành lập bản đồ thuộc danh mục phải đáp ứng điều kiện hay giấy phép chuyên ngành.
6. **Phân quyền và AI:** Plugin, MCP, tác nhân AI và đồng bộ hai chiều phải tôn trọng xác thực, ủy quyền, revision, lịch sử thao tác và ranh giới tenant. Không sử dụng chúng để vượt quyền của chủ nguồn dữ liệu.
7. **Bản quyền bên thứ ba:** Tôn trọng điều khoản của SDK, dữ liệu bản đồ nền, ảnh, logo, font, phần mềm được tích hợp và các nhãn hiệu GIS/CAD.

Văn bản tham khảo: [Luật Đo đạc và bản đồ 27/2018/QH14](https://vanban.chinhphu.vn/?classid=1&docid=206110&pageid=27160) (đọc cùng sửa đổi, văn bản hiện hành); [Luật Bảo vệ dữ liệu cá nhân 91/2025/QH15](https://chinhphu.vn/?docid=214590&pageid=27160) (hiệu lực 01/01/2026). **Mục này không phải tư vấn pháp lý, chứng nhận tuân thủ hoặc giấy phép hoạt động.** Việc triển khai cho cơ quan/doanh nghiệp phải được rà soát theo nghiệp vụ, quyền dữ liệu và quy định hiện hành.

## Mã nguồn mở – Phát triển cùng cộng đồng

OpenVietGIS được xây dựng với tinh thần mã nguồn mở, tự do sáng tạo, chia sẻ tri thức và cùng nhau phát triển công nghệ GIS.

Chúng tôi chào đón mọi cá nhân, tổ chức và doanh nghiệp tham gia hệ sinh thái OpenVietGIS:

- Tự do sử dụng cho mục đích cá nhân, giáo dục, nghiên cứu và thương mại.
- Tự triển khai trên máy chủ và hạ tầng của mình.
- Tự do nghiên cứu, sửa đổi, fork, phân phối và phát triển phiên bản riêng theo giấy phép.
- Phát triển plugin, tiện ích mở rộng và tích hợp với các nền tảng khác.
- Cung cấp dịch vụ triển khai, tư vấn, đào tạo, bảo trì và các dịch vụ thương mại phù hợp với giấy phép.
- Đóng góp ý tưởng, mã nguồn và cùng xây dựng hệ sinh thái GIS ngày càng hoàn thiện.

### Cam kết với cộng đồng

- **Open Source:** Duy trì tính mở của mã nguồn Core.
- **Open Ecosystem:** Khuyến khích phát triển và mở rộng hệ sinh thái.
- **Data Ownership:** Tôn trọng quyền làm chủ dữ liệu của người dùng.
- **No Vendor Lock-in:** Giảm sự phụ thuộc vào một nhà cung cấp duy nhất.
- **Community Driven:** Khuyến khích cộng đồng tham gia định hướng và phát triển sản phẩm.
- **Commercial Friendly:** Khuyến khích các mô hình kinh doanh và dịch vụ phù hợp với giấy phép mã nguồn mở.

### Giấy phép

OpenVietGIS Core được phát hành theo giấy phép **[AGPL-3.0-or-later](LICENSE)**, nhằm duy trì tính mở của mã nguồn và bảo vệ quyền lợi lâu dài của cộng đồng.

Quyền sử dụng hợp pháp đối với các phiên bản đã phát hành không bị tự động thu hồi khi định hướng kinh doanh thay đổi. Các phiên bản sửa đổi thuộc phạm vi AGPL được cung cấp cho người dùng tương tác qua mạng phải cung cấp mã nguồn tương ứng theo giấy phép; điều này không đồng nghĩa với việc công khai dữ liệu người dùng hay bí mật vận hành.

Các SDK, plugin, connector, dữ liệu, bản đồ nền và thành phần độc lập có thể áp dụng giấy phép riêng phù hợp với kiến trúc và mục đích sử dụng. Người đóng góp cần có quyền đối với nội dung cung cấp; Pull Request không mặc nhiên chuyển nhượng toàn bộ bản quyền.

Chi tiết điều khoản được quy định tại [LICENSE](LICENSE) và giấy phép của từng thành phần.

### Tầm nhìn

Lấy cảm hứng từ tinh thần phát triển cộng đồng của Linux, OpenVietGIS hướng đến một nền tảng GIS mở, linh hoạt, có khả năng kết nối và phát triển không giới hạn bởi một cá nhân hay tổ chức duy nhất. OpenVietGIS là dự án độc lập, không trực thuộc hay được chứng thực bởi Linux Foundation hoặc các nhà cung cấp GIS/CAD khác.

Chúng tôi tin rằng công nghệ càng mở, cộng đồng càng mạnh và giá trị tạo ra càng lớn.

**Open Source. Open Ecosystem. Open Future.**

**Không chỉ là một phần mềm GIS, mà là một hệ sinh thái GIS do cộng đồng cùng xây dựng.**

## Trạng thái sản phẩm, nghiệm thu và phát hành

Hãy phân biệt: **đã đặc tả** ≠ **đã có mã** ≠ **đã kiểm thử** ≠ **đã nghiệm thu** ≠ **sẵn sàng production**.

- Lịch sử và phạm vi sản phẩm: [Product direction](docs/product.md), [ROADMAP](ROADMAP.md).
- Ma trận và chương trình QA cộng đồng: [Community QA](docs/community/README.md), [Acceptance matrix](docs/community/ACCEPTANCE_MATRIX.md), [Test runbook](docs/community/TEST_RUNBOOK.md).
- Cổng phát hành: [Release policy](docs/community/RELEASE_POLICY.md).
- Audit tích hợp: [Spec 018](specs/018-full-product-audit/), [bằng chứng kiểm thử](specs/018-full-product-audit/remediation/final-next-integration-evidence.md).

Người thử nghiệm nên ghi **commit, phiên bản frontend/backend, profile, tập dữ liệu giả, CRS, vai trò, kỳ vọng, kết quả và bằng chứng đã ẩn thông tin nhạy cảm**. Không sử dụng production làm phòng thí nghiệm.

## Demo, Donate và tham gia cộng đồng

- **Demo:** [https://openvietgis.cloud](https://openvietgis.cloud). Chỉ dùng dữ liệu thử nghiệm được phép và thao tác trong phạm vi quyền được cấp.
- **Báo lỗi, đề xuất:** [GitHub Issues](https://github.com/nguyentrieu210/OpenVietGis/issues).
- **Đóng góp:** [CONTRIBUTING.md](CONTRIBUTING.md).
- **Ủng hộ dự án:** dùng mục **Donate** trên trang demo (có mã VietQR do nhóm phát triển công bố). Ủng hộ là tự nguyện, không tự tạo quyền sở hữu, quyền ưu tiên phát triển hoặc quyền truy cập hệ thống; không mặc định là khoản được khấu trừ thuế.

### Nhóm dự án được giới thiệu

- **Nguyễn Trọng Triệu:** phát triển nền tảng OpenVietGIS.
- **GS.TS. Trình Công Tư:** Co-founder (theo thông tin giới thiệu của nhóm dự án).
- **Lê Minh:** Co-founder (theo thông tin giới thiệu của nhóm dự án).

Danh sách này là thông tin giới thiệu, **không xác nhận tư cách pháp nhân, chức vụ đại diện theo pháp luật hay thẩm quyền của cơ quan nhà nước**.

## Kiến trúc và điều hướng tài liệu

| Tài liệu | Nơi xem |
| --- | --- |
| Định hướng, lộ trình | [docs/product.md](docs/product.md), [ROADMAP.md](ROADMAP.md) |
| Quy tắc dự án | [.specify/memory/constitution.md](.specify/memory/constitution.md) |
| Kiến trúc và ranh giới Core | [docs/architecture.md](docs/architecture.md) |
| Quyết định kỹ thuật | [docs/adr/README.md](docs/adr/README.md) |
| Spec nền tảng | [specs/001-platform-foundation/](specs/001-platform-foundation/) |
| Danh mục yêu cầu/các giai đoạn | [specs/](specs/) |
| Tích hợp bên ngoài | [docs/developer-integrations.md](docs/developer-integrations.md) |
| Hướng dẫn đóng góp | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Bảo mật | [SECURITY.md](SECURITY.md) |

**Nguyên tắc:** Core/platform không được phụ thuộc ngược vào logic ngành dọc. Plugin, module, API và bridge phải tuân thủ quyền, tenancy, schema, audit và hợp đồng Core.

## Yêu cầu môi trường

| Thành phần | Baseline CI |
| --- | --- |
| Node.js | 26 LTS (`>=26 <27`) |
| pnpm | 12.21.0 |
| Python | 3.13 qua `uv` |
| Database | PostgreSQL 18 + PostGIS 3.7 theo Compose |
| Hạ tầng | Docker + Compose |
| Kiểm thử trình duyệt | Playwright Chromium |

### Bootstrap

Từ thư mục gốc repository, chạy:

```bash
git clone https://github.com/nguyentrieu210/OpenVietGis.git
cd OpenVietGis
npm install --global pnpm@12.21.0
pnpm install --frozen-lockfile
uv python install 3.13
uv sync --locked --project apps/api
```

Nếu chưa có `.env`, sao chép từ `.env.example`. Bash: `cp .env.example .env`; PowerShell: `Copy-Item .env.example .env`. **Không commit `.env`, token hoặc dữ liệu thật.**

```bash
docker compose -f infra/dev/compose.yaml up -d postgres postgres-test
uv run --project apps/api alembic -c apps/api/alembic.ini upgrade head
pnpm exec playwright install chromium
```

Môi trường phát triển không cần credential hay dữ liệu production. Để áp dụng thêm migration module theo repo, xem `pnpm db:migrate` trong `package.json`.

### Run locally

Khởi động API và web ở hai terminal:

```bash
pnpm dev:api
```

```bash
pnpm dev:web
```

Mặc định: web `http://127.0.0.1:5173`; API `http://127.0.0.1:8000`; readiness endpoint `/api/v1/health/ready`.

Repo có **`apps/web`** (được chạy bởi `pnpm dev:web`) và **`apps/web-next`** (giao diện đang phát triển). Để chạy riêng web-next, **dừng web đang chiếm cổng 5173**, rồi dùng:

```bash
pnpm --filter @openvietgis/web-next dev
```

Không mặc định các frontend hoặc bản demo luôn cùng commit, feature flags và cấu hình backend.

### OIDC development profile

```bash
pnpm dev:identity
```

Keycloak tham chiếu tại `http://127.0.0.1:18080`, realm `openvietgis`; các tài khoản **giả lập** `owner-a`, `member-a`, `member-b` có trong `infra/dev/keycloak/realm-openvietgis.json`. Đây là provider phục vụ thử nghiệm; quyền theo Organization/Workspace/Project vẫn thuộc OpenVietGIS. Không lưu provider token trong local/session storage của trình duyệt. Xem [Spec 002 quickstart](specs/002-identity-organization-tenancy/quickstart.md).

## Hợp đồng API và kiểm thử

```bash
pnpm contracts:generate
pnpm validate:contracts
pnpm validate:ts
pnpm validate:py
pnpm validate:arch
pnpm validate:docs
```

Bộ kiểm thử tổng hợp khi môi trường PostGIS/test đã sẵn sàng:

```bash
pnpm validate
```

Nó bao gồm các bước static/unit, Python/TypeScript, architecture, contract, docs, integration trên PostGIS và E2E theo script trong `package.json`. **Kết quả xanh trong một profile không tương đương chứng nhận phát hành.**

- CSDL phát triển mặc định: `openvietgis`, cổng **54329** trên localhost.
- CSDL test tách biệt: `openvietgis_test`, cổng **54330** trên localhost; các kiểm thử có guard chống trỏ nhầm dữ liệu dùng chung.
- Kiểm thử migration/destructive dùng **fixture/DB có thể xóa**; không lưu dữ liệu quan trọng trong cơ sở dữ liệu test.
- Không sửa guard môi trường, không cấp token production cho kiểm thử.

### Safe development reset

```bash
pnpm dev:reset
```

Lệnh này chỉ dành cho local **development/test**, có kiểm tra môi trường, host và tên cơ sở dữ liệu cho phép. Một số migration có thể từ chối downgrade nhằm bảo vệ dữ liệu; **không lách cơ chế an toàn**. Không trỏ lệnh reset, migration-cycle hay integration test vào production.

### Troubleshooting

| Sự cố | Hướng kiểm tra |
| --- | --- |
| Node engine không tương thích | Dùng Node 26 LTS và pnpm 12.21.0 |
| Cổng 5173/8000 đã dùng | Dừng tiến trình dev cũ hoặc đổi cấu hình local có chủ đích |
| PostGIS không hoạt động | `docker compose -f infra/dev/compose.yaml ps` và bật `postgres postgres-test` |
| Schema chưa migration | `pnpm db:migrate` với DB **local** |
| API báo thiếu cấu hình | Tạo `.env` từ `.env.example` và điền config local |
| Lỗi đăng nhập | Kiểm tra Keycloak, OIDC redirect/origin và thời gian hệ thống |
| Không thấy tính năng | Kiểm tra URL, frontend, quyền, plugin, migration, commit |
| Demo không trùng `main` | Kiểm tra release SHA thực tế; không coi GitHub `main` là bản live |

[Spec 001](specs/001-platform-foundation/) định nghĩa hai phép đo: bootstrap trên môi trường hỗ trợ trong **≤20 phút** (không tính thời gian tải ngoài được ghi riêng), và khả năng người mới tìm tài liệu nền trong **<10 phút**. Đây là **tiêu chí nghiệm thu**, không phải cam kết mọi thiết bị hiện đều đạt.

## Đóng góp, bảo mật và quản trị

1. Đọc [CONTRIBUTING](CONTRIBUTING.md), Issue, spec và ADR liên quan.
2. Mở Issue có bước tái hiện, commit/phiên bản, vai trò, dữ liệu giả và kết quả mong đợi/thực tế.
3. Fork hoặc tạo nhánh từ `main`; PR giới hạn một phạm vi, có kiểm thử và tài liệu.
4. Không sử dụng demo/production để kiểm thử hành vi ghi/xóa hoặc pentest nếu chưa được ủy quyền riêng.
5. Gửi báo cáo bảo mật theo [SECURITY.md](SECURITY.md), không đăng công khai secrets, dữ liệu người dùng hoặc hướng dẫn khai thác lỗ hổng chưa được khắc phục.

Maintainer và reviewer quyết định phạm vi release dựa trên bằng chứng cùng SHA, profile, backup/restore, rollback, review và rủi ro tồn đọng. Star/fork, số commit hoặc hiệu ứng giao diện **không phải** tiêu chí chứng nhận sản phẩm.

---

## English summary

**OpenVietGIS is an independent open-source WebGIS project** exploring spatial data editing, cartography, collaboration, plugins, interoperability and GIS/CAD/AI workflows.

**Status:** public development candidate; **not production-certified, not an official cadastral or government information system, and not a certified replacement for QGIS/ArcGIS/MapInfo/AutoCAD**. The project declares **AGPL-3.0-or-later** in [LICENSE](LICENSE), subject to applicable component and third-party licenses.

Use only data you are authorized to access, edit or publish. Do not upload personal, classified, restricted or official cadastral data to the public demo. All GIS plugins, AI agents and integrations must respect permissions. No production testing or pentesting without explicit authorization.

[Demo](https://openvietgis.cloud) · [Issues](https://github.com/nguyentrieu210/OpenVietGis/issues) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [License](LICENSE)


### Ứng viên ổn định hóa

Xem [Candidate runbook](docs/community/CANDIDATE_RUNBOOK.md) và Spec 031–047 trong [ROADMAP](ROADMAP.md). Kết quả kiểm thử local không đồng nghĩa đã phát hành live hoặc chứng nhận production.
