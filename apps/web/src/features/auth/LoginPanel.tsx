import {
  listIdentityProviders,
  type IdentityProviderSummary
} from "@openvietgis/client-sdk";
import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../../app/api";
import "./auth.css";

function loginUrl(provider: IdentityProviderSummary): string {
  const returnTo = encodeURIComponent(window.location.pathname || "/");
  return (
    "/api/v1/auth/login/" +
    encodeURIComponent(provider.id) +
    "?return_to=" +
    returnTo
  );
}

export function LoginPanel() {
  const providers = useQuery({
    queryKey: ["identity-providers"],
    queryFn: () => listIdentityProviders(apiClient)
  });

  return (
    <main className="ovg-landing">
      <header className="ovg-landing-header">
        <a className="ovg-brand" href="#top" aria-label="OpenVietGIS">
          <span className="ovg-brand-mark">OVG</span>
          <span>
            <strong>OpenVietGIS</strong>
            <small>Nền tảng GIS web mã nguồn mở</small>
          </span>
        </a>
        <nav aria-label="Điều hướng trang giới thiệu">
          <a href="#features">Tính năng</a>
          <a href="#workflow">Quy trình</a>
          <a href="#login">Đăng nhập</a>
        </nav>
      </header>

      <section id="top" className="ovg-hero">
        <div className="ovg-hero-copy">
          <span className="ovg-eyebrow">GIS WEB · POSTGIS · BẢN ĐỒ VIỆT NAM</span>
          <h1>Quản lý dữ liệu không gian trên một giao diện hiện đại, thuần Việt.</h1>
          <p className="ovg-hero-lead">
            OpenVietGIS giúp tổ chức quản lý lớp dữ liệu, bản đồ nền vệ tinh, biên tập,
            truy vấn, xử lý không gian, nhập/xuất và cộng tác trong cùng một hệ thống.
          </p>
          <div className="ovg-hero-actions">
            <a className="ovg-primary-link" href="#login">Bắt đầu sử dụng</a>
            <a className="ovg-secondary-link" href="#features">Xem tính năng</a>
          </div>
          <div className="ovg-proof-row">
            <span><b>PostGIS</b><small>Dữ liệu không gian chuẩn</small></span>
            <span><b>Google / Esri</b><small>Bản đồ nền vệ tinh</small></span>
            <span><b>GeoPackage</b><small>Nhập / xuất dữ liệu</small></span>
          </div>
        </div>

        <div className="ovg-map-preview" aria-label="Minh họa không gian bản đồ OpenVietGIS">
          <div className="ovg-map-toolbar">
            <span>OpenVietGIS</span>
            <span className="ovg-map-status">● Trực tuyến</span>
          </div>
          <div className="ovg-map-stage-preview">
            <div className="ovg-map-grid" />
            <div className="ovg-map-road ovg-road-a" />
            <div className="ovg-map-road ovg-road-b" />
            <div className="ovg-map-shape ovg-shape-a" />
            <div className="ovg-map-shape ovg-shape-b" />
            <div className="ovg-map-pin ovg-pin-a"><i /></div>
            <div className="ovg-map-pin ovg-pin-b"><i /></div>
            <aside className="ovg-map-layer-card">
              <strong>Lớp dữ liệu</strong>
              <span>☑ Ranh giới khu vực</span>
              <span>☑ Điểm công trình</span>
              <span>☑ Tuyến giao thông</span>
            </aside>
            <div className="ovg-map-coordinates">EPSG:4326 · 10.7769, 106.7009</div>
          </div>
        </div>
      </section>

      <section id="features" className="ovg-feature-section">
        <div className="ovg-section-heading">
          <span className="ovg-eyebrow">NĂNG LỰC CỐT LÕI</span>
          <h2>Một workspace GIS cho toàn bộ vòng đời dữ liệu bản đồ.</h2>
        </div>
        <div className="ovg-feature-grid">
          <article>
            <span className="ovg-feature-icon">01</span>
            <h3>Quản lý lớp dữ liệu</h3>
            <p>Tổ chức nguồn dữ liệu, lớp hiển thị, CRS, thuộc tính và phiên bản một cách rõ ràng.</p>
          </article>
          <article>
            <span className="ovg-feature-icon">02</span>
            <h3>Bản đồ nền vệ tinh</h3>
            <p>Chuyển nhanh giữa Google Hybrid, Google Satellite, Esri, OSM, địa hình và nền tối.</p>
          </article>
          <article>
            <span className="ovg-feature-icon">03</span>
            <h3>Biên tập & truy vấn</h3>
            <p>Chỉnh sửa đối tượng, lọc thuộc tính, truy vấn không gian và theo dõi lịch sử thay đổi.</p>
          </article>
          <article>
            <span className="ovg-feature-icon">04</span>
            <h3>Nhập / xuất chuẩn GIS</h3>
            <p>Hỗ trợ GeoPackage, GeoJSON, Shapefile ZIP và chuyển đổi hệ tọa độ phổ biến.</p>
          </article>
        </div>
      </section>

      <section id="workflow" className="ovg-workflow">
        <div>
          <span className="ovg-eyebrow">QUY TRÌNH</span>
          <h2>Từ dữ liệu thô đến bản đồ dùng chung.</h2>
        </div>
        <ol>
          <li><b>01</b><span><strong>Nhập dữ liệu</strong><small>GeoPackage, GeoJSON, Shapefile</small></span></li>
          <li><b>02</b><span><strong>Kiểm tra & chuẩn hóa</strong><small>CRS, schema, phạm vi</small></span></li>
          <li><b>03</b><span><strong>Hiển thị & phân tích</strong><small>Bản đồ, truy vấn, xử lý</small></span></li>
          <li><b>04</b><span><strong>Chia sẻ & vận hành</strong><small>Phân quyền, lịch sử, xuất bản</small></span></li>
        </ol>
      </section>

      <section id="login" className="ovg-login-section">
        <div className="ovg-login-copy">
          <span className="ovg-eyebrow">KHÔNG GIAN LÀM VIỆC</span>
          <h2>Đăng nhập để tiếp tục.</h2>
          <p>
            Tài khoản được xác thực qua hệ thống định danh của OpenVietGIS. Sau khi đăng nhập,
            bạn sẽ được đưa về đúng tổ chức và dự án có quyền truy cập.
          </p>
          <ul>
            <li>Phân quyền theo tổ chức, dự án và tài nguyên</li>
            <li>Phiên đăng nhập bảo mật</li>
            <li>Lịch sử thao tác và thay đổi có thể kiểm tra</li>
          </ul>
        </div>

        <div className="ovg-login-card">
          <span className="ovg-login-logo">OVG</span>
          <h2 id="login-title">Đăng nhập OpenVietGIS</h2>
          <p>Sử dụng tài khoản được cấp để truy cập hệ thống.</p>

          {providers.isLoading ? <div className="ovg-login-state">Đang tải phương thức đăng nhập…</div> : null}
          {providers.error ? <div className="ovg-login-error" role="alert">Không thể tải phương thức đăng nhập.</div> : null}

          <div className="ovg-provider-list">
            {providers.data?.map((provider) => (
              <a className="ovg-provider-button" key={provider.id} href={loginUrl(provider)}>
                <span className="ovg-provider-icon">→</span>
                <span>Đăng nhập với {provider.display_name}</span>
              </a>
            ))}
          </div>

          <small className="ovg-login-note">
            Bằng việc đăng nhập, bạn xác nhận sử dụng hệ thống theo quyền đã được cấp.
          </small>
        </div>
      </section>

      <footer className="ovg-landing-footer">
        <div className="ovg-brand">
          <span className="ovg-brand-mark">OVG</span>
          <span><strong>OpenVietGIS</strong><small>Nền tảng GIS web</small></span>
        </div>
        <span>© 2026 OpenVietGIS · Xây dựng cho dữ liệu không gian Việt Nam</span>
      </footer>
    </main>
  );
}
