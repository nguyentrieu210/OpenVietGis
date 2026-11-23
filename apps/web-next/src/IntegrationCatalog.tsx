import { useEffect, useMemo, useRef, useState } from "react";
import {
  ApiError,
  downloadIntegrationLayerExport,
  getIntegrationHubAdapterProfile,
  getIntegrationHubCatalog,
  type ActionDenialReason,
  type AdapterExecutionStatus,
  type AdapterLayerDescriptor,
  type ApiClient,
  type ConnectorLifecycle,
  type IntegrationAdapterProfileResponse,
  type IntegrationAdapterSummary,
  type IntegrationCatalogResponse,
  type IntegrationExportAdapterKey,
  type IntegrationExportFormat
} from "@openvietgis/client-sdk";
import "./integration-catalog.css";

interface IntegrationCatalogProps {
  client: ApiClient;
  org: string;
  projectId: string;
}

interface LayerExportStatus {
  kind: "success" | "error";
  filename?: string;
  crs?: string;
  featureCount?: number | null;
  shortSha256?: string | null;
  message: string;
}

const LIFECYCLE_LABELS: Record<ConnectorLifecycle, string> = {
  existing_read_only: "Có dịch vụ chỉ đọc",
  candidate: "Đang thử nghiệm",
  planned: "Chưa triển khai"
};

const EXECUTION_STATUS_LABELS: Record<AdapterExecutionStatus, string> = {
  existing_read_only_api: "Dịch vụ chỉ đọc hiện hữu",
  read_only_profile_ready: "Hồ sơ trao đổi chỉ đọc",
  candidate_external_branch: "Nhánh thử nghiệm độc lập",
  planned_not_implemented: "Chưa triển khai thực thi"
};

const DENIAL_REASON_LABELS: Record<ActionDenialReason, string> = {
  catalog_only: "Chỉ cho phép tra cứu danh mục / hồ sơ",
  read_only_export: "Cho phép tải xuất lớp chỉ đọc theo quyền và CRS",
  permission_denied: "Không đủ quyền dự án",
  unsupported_by_adapter: "Adapter không khai báo thao tác",
  not_implemented: "Chưa triển khai trong Integration Hub"
};

const EXPORT_FORMAT_LABELS: Record<IntegrationExportFormat, string> = {
  geopackage: "GeoPackage (.gpkg)",
  geojson: "GeoJSON (.geojson)",
  postgis_sql: "PostGIS SQL (.sql)"
};

const CATEGORY_LABELS: Record<string, string> = {
  gis: "GIS",
  cad: "CAD",
  cadastral: "Địa chính",
  database: "Cơ sở dữ liệu",
  tabular: "Bảng tính",
  survey_3d: "Trắc địa / 3D"
};

function isExportAdapterKey(key: string): key is IntegrationExportAdapterKey {
  return key === "qgis" || key === "postgis";
}

function formatLayerCrs(layer: AdapterLayerDescriptor): string {
  if (!layer.crs_explicit) {
    return "Chưa xác định CRS";
  }
  if (layer.stored_crs_code !== null) {
    if (
      layer.source_crs_code !== null &&
      layer.source_crs_code !== layer.stored_crs_code
    ) {
      return `EPSG:${layer.stored_crs_code} (nguồn EPSG:${layer.source_crs_code})`;
    }
    return `EPSG:${layer.stored_crs_code}`;
  }
  if (layer.source_crs_code !== null) {
    return `EPSG:${layer.source_crs_code}`;
  }
  return "Chưa xác định CRS";
}

function lifecycleBadgeClass(lifecycle: ConnectorLifecycle): string {
  if (lifecycle === "existing_read_only") {
    return "integration-badge is-existing";
  }
  if (lifecycle === "candidate") {
    return "integration-badge is-candidate";
  }
  return "integration-badge is-planned";
}

function formatExportErrorMessage(layerName: string, error: unknown): string {
  if (error instanceof ApiError) {
    if (error.status === 403) {
      return `Không có quyền xuất lớp "${layerName}" (HTTP 403). Kiểm tra quyền dự án hoặc phạm vi GIS (ACL).`;
    }
    if (error.status === 409) {
      return `Không thể xuất lớp "${layerName}" (HTTP 409): lớp chưa sẵn sàng, thiếu CRS tường minh hoặc vượt giới hạn xuất chỉ đọc.`;
    }
    if (error.status === 422) {
      return `Không thể xuất lớp "${layerName}" theo định dạng đã chọn (HTTP 422): tham số không hợp lệ hoặc GeoJSON yêu cầu EPSG:4326.`;
    }
    if (error.status === 404) {
      return `Không tìm thấy lớp "${layerName}" hoặc đầu cuối xuất chỉ đọc (HTTP 404).`;
    }
  }
  return `Không thể tải tệp xuất chỉ đọc cho lớp "${layerName}". Vui lòng thử lại.`;
}

function triggerBrowserBlobDownload(blob: Blob, filename: string): void {
  const objectUrl = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = objectUrl;
  anchor.download = filename;
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  try {
    anchor.click();
  } finally {
    anchor.remove();
    URL.revokeObjectURL(objectUrl);
  }
}

export function IntegrationCatalog({
  client,
  org,
  projectId
}: IntegrationCatalogProps) {
  const [catalog, setCatalog] = useState<IntegrationCatalogResponse | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState("");

  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [profiles, setProfiles] = useState<
    Record<string, IntegrationAdapterProfileResponse>
  >({});
  const [profileLoadingKey, setProfileLoadingKey] = useState<string | null>(null);
  const [profileError, setProfileError] = useState<Record<string, string>>({});
  const [exportingKey, setExportingKey] = useState<string | null>(null);
  const [exportStatusByAdapter, setExportStatusByAdapter] = useState<
    Record<string, LayerExportStatus>
  >({});
  const scopeGeneration = useRef(0);

  useEffect(() => {
    let alive = true;
    scopeGeneration.current += 1;

    setCatalog(null);
    setError("");
    setLoading(true);
    setExpandedKey(null);
    setProfiles({});
    setProfileLoadingKey(null);
    setProfileError({});
    setExportingKey(null);
    setExportStatusByAdapter({});

    void getIntegrationHubCatalog(client, org, projectId)
      .then((value) => {
        if (alive) {
          setCatalog(value);
        }
      })
      .catch(() => {
        if (alive) {
          setError("Không thể đọc danh mục. Hãy kiểm tra quyền xem dự án.");
        }
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [client, org, projectId]);

  const filteredAdapters = useMemo(() => {
    if (!catalog) {
      return [];
    }
    const normalized = query.trim().toLocaleLowerCase("vi");
    if (!normalized) {
      return catalog.adapters;
    }
    return catalog.adapters.filter((adapter) => {
      const searchable = [
        adapter.name,
        adapter.key,
        adapter.category,
        CATEGORY_LABELS[adapter.category] ?? "",
        ...adapter.protocols
      ]
        .join(" ")
        .toLocaleLowerCase("vi");
      return searchable.includes(normalized);
    });
  }, [catalog, query]);

  const inspectAdapterProfile = (adapter: IntegrationAdapterSummary) => {
    if (expandedKey === adapter.key) {
      setExpandedKey(null);
      return;
    }

    setExpandedKey(adapter.key);
    if (profiles[adapter.key]) {
      return;
    }

    const requestGeneration = scopeGeneration.current;
    setProfileLoadingKey(adapter.key);
    setProfileError((prev) => {
      const next = { ...prev };
      delete next[adapter.key];
      return next;
    });

    void getIntegrationHubAdapterProfile(client, org, projectId, adapter.key)
      .then((profile) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setProfiles((prev) => ({ ...prev, [adapter.key]: profile }));
      })
      .catch(() => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setProfileError((prev) => ({
          ...prev,
          [adapter.key]:
            "Không thể đọc hồ sơ kết nối chỉ đọc cho adapter này."
        }));
      })
      .finally(() => {
        if (scopeGeneration.current === requestGeneration) {
          setProfileLoadingKey((current) =>
            current === adapter.key ? null : current
          );
        }
      });
  };

  const handleReadOnlyExport = (
    adapterKey: IntegrationExportAdapterKey,
    layer: AdapterLayerDescriptor,
    format: IntegrationExportFormat
  ) => {
    const busyToken = `${adapterKey}:${layer.layer_id}:${format}`;
    const requestGeneration = scopeGeneration.current;
    setExportingKey(busyToken);
    setExportStatusByAdapter((prev) => {
      const next = { ...prev };
      delete next[adapterKey];
      return next;
    });

    void downloadIntegrationLayerExport(
      client,
      org,
      projectId,
      adapterKey,
      layer.layer_id,
      format
    )
      .then((result) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        triggerBrowserBlobDownload(result.blob, result.filename);

        const resolvedCrs = result.crs ?? formatLayerCrs(layer);
        const shortSha256 = result.sha256 ? result.sha256.slice(0, 12) : null;
        const summaryParts = [
          `Tệp: ${result.filename}`,
          `CRS: ${resolvedCrs}`
        ];
        if (result.featureCount !== null) {
          summaryParts.push(`Đối tượng: ${result.featureCount}`);
        }
        if (shortSha256) {
          summaryParts.push(`SHA-256: ${shortSha256}`);
        }

        setExportStatusByAdapter((prev) => ({
          ...prev,
          [adapterKey]: {
            kind: "success",
            filename: result.filename,
            crs: resolvedCrs,
            featureCount: result.featureCount,
            shortSha256,
            message: `Đã xuất bản sao chỉ đọc · ${summaryParts.join(" · ")}`
          }
        }));
      })
      .catch((downloadError: unknown) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setExportStatusByAdapter((prev) => ({
          ...prev,
          [adapterKey]: {
            kind: "error",
            message: formatExportErrorMessage(layer.name, downloadError)
          }
        }));
      })
      .finally(() => {
        if (scopeGeneration.current === requestGeneration) {
          setExportingKey((current) =>
            current === busyToken ? null : current
          );
        }
      });
  };

  return (
    <section className="integration-catalog" aria-label="Tích hợp phần mềm">
      <details>
        <summary>
          <strong>Tích hợp phần mềm</strong>
          <span>
            {catalog
              ? `${catalog.adapters.length} nhóm kết nối · chính sách ghi mặc định: ${catalog.default_write_policy}`
              : "GIS · CAD · Địa chính · 3D"}
          </span>
        </summary>

        <p className="live-note">
          Danh mục năng lực kỹ thuật chỉ đọc của OpenVietGIS. Quyền dự án không tự
          động cấp quyền truy cập tệp, cơ sở dữ liệu hoặc thực thi đồng bộ ở phần
          mềm bên ngoài.
        </p>

        {catalog && (
          <div className="integration-policy-bar">
            <span className="integration-policy-pill is-deny">
              Chính sách ghi mặc định: Từ chối ({catalog.default_write_policy})
            </span>
            <span className="integration-policy-pill is-readonly">
              Chế độ Hub: Chỉ đọc ({catalog.mode})
            </span>
            {catalog.scoped_access && (
              <span className="integration-policy-pill is-scoped" role="status">
                Phạm vi GIS bị giới hạn (scoped_access): chỉ hiển thị lớp và quyền
                trong phạm vi được cấp
              </span>
            )}
          </div>
        )}

        {loading && <p role="status">Đang kiểm tra danh mục…</p>}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}

        {catalog && (
          <>
            <label className="integration-search">
              Tìm phần mềm hoặc định dạng
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="QGIS, PostGIS, AutoCAD, DGN, WFS…"
              />
            </label>

            <div className="integration-list">
              {filteredAdapters.map((adapter) => {
                const discoverAction = adapter.actions.find(
                  (item) => item.action === "catalog.discover"
                );
                const pushAction = adapter.actions.find(
                  (item) => item.action === "sync.push"
                );
                const isExpanded = expandedKey === adapter.key;
                const profile = profiles[adapter.key] ?? null;
                const isProfileLoading = profileLoadingKey === adapter.key;
                const adapterError = profileError[adapter.key] ?? "";
                const detailPanelId = `integration-profile-${adapter.key}`;
                const exportAdapterKey = isExportAdapterKey(adapter.key)
                  ? adapter.key
                  : null;
                const adapterExportStatus =
                  exportStatusByAdapter[adapter.key] ?? null;

                return (
                  <article
                    key={adapter.key}
                    className={`integration-item${isExpanded ? " is-expanded" : ""}`}
                    data-adapter-key={adapter.key}
                  >
                    <div className="integration-item-header">
                      <b>{adapter.name}</b>
                      <span className={lifecycleBadgeClass(adapter.lifecycle)}>
                        {LIFECYCLE_LABELS[adapter.lifecycle]}
                      </span>
                    </div>

                    <div className="integration-meta">
                      <span>
                        {CATEGORY_LABELS[adapter.category] ?? adapter.category}
                      </span>
                      <span>{adapter.protocols.join(" · ")}</span>
                    </div>

                    <p>{adapter.note}</p>

                    <small>
                      Quyền xem danh mục:{" "}
                      {discoverAction?.allowed ? "Được phép" : "Không được cấp"}.
                      {" "}Đồng bộ ghi:{" "}
                      {pushAction?.executable_in_hub
                        ? "Có API"
                        : "Từ chối mặc định / chưa hỗ trợ"}
                      .
                    </small>

                    {adapter.existing_api && (
                      <small>
                        Dịch vụ hiện hữu: {adapter.existing_api} (vẫn áp dụng phân
                        quyền riêng).
                      </small>
                    )}

                    <div className="integration-item-footer">
                      <button
                        type="button"
                        className="integration-inspect-toggle"
                        aria-expanded={isExpanded}
                        aria-controls={detailPanelId}
                        onClick={() => inspectAdapterProfile(adapter)}
                      >
                        {isExpanded
                          ? "Ẩn hồ sơ kết nối"
                          : "Xem hồ sơ kết nối"}
                      </button>
                    </div>

                    {isExpanded && (
                      <section
                        id={detailPanelId}
                        className="integration-profile"
                        aria-label={`Hồ sơ chỉ đọc cho ${adapter.name}`}
                      >
                        {isProfileLoading && (
                          <p className="integration-profile-status" role="status">
                            Đang tải hồ sơ kết nối chỉ đọc…
                          </p>
                        )}

                        {adapterError && (
                          <p className="error" role="alert">
                            {adapterError}
                          </p>
                        )}

                        {profile && (
                          <div className="integration-profile-body">
                            <div className="integration-profile-summary">
                              <span className="integration-badge is-status">
                                {EXECUTION_STATUS_LABELS[profile.execution_status]}
                              </span>
                              <span className="integration-policy-pill is-deny">
                                Ghi mặc định: {profile.default_write_policy}
                              </span>
                              {profile.scoped_access && (
                                <span className="integration-policy-pill is-scoped">
                                  Tài khoản giới hạn phạm vi GIS
                                </span>
                              )}
                            </div>

                            {profile.connection_profile ? (
                              <div className="integration-profile-block">
                                <h4>Hồ sơ trao đổi chỉ đọc</h4>
                                <dl className="integration-kv">
                                  <div>
                                    <dt>Chế độ truy cập</dt>
                                    <dd>
                                      Trao đổi chỉ đọc (
                                      <code>
                                        {profile.connection_profile.access_mode}
                                      </code>
                                      )
                                    </dd>
                                  </div>
                                  <div>
                                    <dt>Thông tin đăng nhập CSDL trực tiếp</dt>
                                    <dd>
                                      {profile.connection_profile
                                        .supports_direct_db_credentials
                                        ? "Có"
                                        : "Không hỗ trợ (false)"}
                                    </dd>
                                  </div>
                                  <div>
                                    <dt>ACL theo từng lớp</dt>
                                    <dd>
                                      {profile.connection_profile
                                        .requires_per_layer_acl
                                        ? "Bắt buộc kiểm tra"
                                        : "Không yêu cầu"}
                                    </dd>
                                  </div>
                                  <div>
                                    <dt>Chính sách hệ tọa độ (CRS)</dt>
                                    <dd>
                                      Chỉ dùng CRS tường minh, không suy đoán (
                                      <code>
                                        {profile.connection_profile.crs_policy}
                                      </code>
                                      )
                                    </dd>
                                  </div>
                                  <div>
                                    <dt>Giao thức hỗ trợ</dt>
                                    <dd>
                                      {profile.connection_profile.supported_protocols.join(
                                        ", "
                                      )}
                                    </dd>
                                  </div>
                                  {profile.connection_profile.export_formats &&
                                    profile.connection_profile.export_formats
                                      .length > 0 && (
                                      <div>
                                        <dt>Định dạng xuất chỉ đọc</dt>
                                        <dd>
                                          {profile.connection_profile.export_formats
                                            .map(
                                              (fmt) =>
                                                EXPORT_FORMAT_LABELS[fmt] ?? fmt
                                            )
                                            .join(", ")}
                                        </dd>
                                      </div>
                                    )}
                                </dl>

                                {profile.connection_profile.restrictions.length >
                                  0 && (
                                  <div className="integration-subblock">
                                    <h5>Giới hạn bảo mật</h5>
                                    <ul className="integration-restrictions">
                                      {profile.connection_profile.restrictions.map(
                                        (restriction) => (
                                          <li key={restriction}>{restriction}</li>
                                        )
                                      )}
                                    </ul>
                                  </div>
                                )}

                                {profile.connection_profile.existing_endpoints
                                  .length > 0 && (
                                  <div className="integration-subblock">
                                    <h5>API chỉ đọc hiện hữu</h5>
                                    <ul className="integration-endpoints">
                                      {profile.connection_profile.existing_endpoints.map(
                                        (endpoint) => (
                                          <li
                                            key={`${endpoint.method}-${endpoint.name}`}
                                          >
                                            <span className="integration-method-badge">
                                              {endpoint.method}
                                            </span>
                                            <code>{endpoint.path_template}</code>
                                            <small>
                                              ({endpoint.name} · quyền:{" "}
                                              {endpoint.required_permission})
                                            </small>
                                          </li>
                                        )
                                      )}
                                    </ul>
                                  </div>
                                )}
                              </div>
                            ) : (
                              <p className="integration-profile-empty">
                                Adapter này chưa có hồ sơ kết nối thực thi trong
                                Integration Hub; mọi thao tác ghi hoặc đồng bộ đều
                                bị từ chối mặc định.
                              </p>
                            )}

                            <div className="integration-profile-block">
                              <h4>
                                Lớp dữ liệu khả dụng trong dự án (
                                {profile.layers.length})
                              </h4>
                              {profile.layers.length === 0 ? (
                                <p className="integration-profile-empty">
                                  Không có lớp dữ liệu hoạt động nào hiển thị trong
                                  phạm vi quyền hiện tại.
                                </p>
                              ) : (
                                <div className="integration-table-wrap">
                                  <table className="integration-table">
                                    <thead>
                                      <tr>
                                        <th scope="col">Tên lớp</th>
                                        <th scope="col">Hình học</th>
                                        <th scope="col">Hệ tọa độ (CRS)</th>
                                        <th scope="col">Kiểm định</th>
                                        <th scope="col">Quyền xuất</th>
                                        {exportAdapterKey && (
                                          <th scope="col">Xuất chỉ đọc</th>
                                        )}
                                      </tr>
                                    </thead>
                                    <tbody>
                                      {profile.layers.map((layer) => {
                                        const gpkgBusy =
                                          exportingKey ===
                                          `${adapter.key}:${layer.layer_id}:geopackage`;
                                        const geojsonBusy =
                                          exportingKey ===
                                          `${adapter.key}:${layer.layer_id}:geojson`;
                                        const sqlBusy =
                                          exportingKey ===
                                          `${adapter.key}:${layer.layer_id}:postgis_sql`;

                                        return (
                                          <tr key={layer.layer_id}>
                                            <td>
                                              <strong>{layer.name}</strong>
                                              <small className="integration-cell-sub">
                                                rev {layer.layer_revision}
                                              </small>
                                            </td>
                                            <td>
                                              {layer.geometry_family} (
                                              {layer.dimensions})
                                            </td>
                                            <td>
                                              <span
                                                className={
                                                  layer.crs_explicit
                                                    ? "integration-crs-explicit"
                                                    : "integration-crs-missing"
                                                }
                                              >
                                                {formatLayerCrs(layer)}
                                              </span>
                                            </td>
                                            <td>
                                              <code>
                                                {layer.validation_state}
                                              </code>
                                            </td>
                                            <td>
                                              <span
                                                className={
                                                  layer.export_permitted
                                                    ? "integration-flag is-allowed"
                                                    : "integration-flag is-denied"
                                                }
                                              >
                                                {layer.export_permitted
                                                  ? "Được phép xuất"
                                                  : "Không được xuất"}
                                              </span>
                                            </td>
                                            {exportAdapterKey && (
                                              <td>
                                                {layer.export_permitted ? (
                                                  <div className="integration-export-actions">
                                                    {exportAdapterKey ===
                                                      "qgis" && (
                                                      <>
                                                        <button
                                                          type="button"
                                                          className="integration-export-btn"
                                                          disabled={
                                                            exportingKey !==
                                                            null
                                                          }
                                                          onClick={() =>
                                                            handleReadOnlyExport(
                                                              "qgis",
                                                              layer,
                                                              "geopackage"
                                                            )
                                                          }
                                                        >
                                                          {gpkgBusy
                                                            ? "Đang tải .gpkg…"
                                                            : "Tải .gpkg"}
                                                        </button>
                                                        {layer.stored_crs_code ===
                                                          4326 && (
                                                          <button
                                                            type="button"
                                                            className="integration-export-btn"
                                                            disabled={
                                                              exportingKey !==
                                                              null
                                                            }
                                                            onClick={() =>
                                                              handleReadOnlyExport(
                                                                "qgis",
                                                                layer,
                                                                "geojson"
                                                              )
                                                            }
                                                          >
                                                            {geojsonBusy
                                                              ? "Đang tải .geojson…"
                                                              : "Tải .geojson"}
                                                          </button>
                                                        )}
                                                      </>
                                                    )}
                                                    {exportAdapterKey ===
                                                      "postgis" && (
                                                      <>
                                                        <button
                                                          type="button"
                                                          className="integration-export-btn"
                                                          disabled={
                                                            exportingKey !==
                                                            null
                                                          }
                                                          onClick={() =>
                                                            handleReadOnlyExport(
                                                              "postgis",
                                                              layer,
                                                              "postgis_sql"
                                                            )
                                                          }
                                                        >
                                                          {sqlBusy
                                                            ? "Đang tải .sql…"
                                                            : "Tải .sql"}
                                                        </button>
                                                        <button
                                                          type="button"
                                                          className="integration-export-btn"
                                                          disabled={
                                                            exportingKey !==
                                                            null
                                                          }
                                                          onClick={() =>
                                                            handleReadOnlyExport(
                                                              "postgis",
                                                              layer,
                                                              "geopackage"
                                                            )
                                                          }
                                                        >
                                                          {gpkgBusy
                                                            ? "Đang tải .gpkg…"
                                                            : "Tải .gpkg"}
                                                        </button>
                                                      </>
                                                    )}
                                                  </div>
                                                ) : (
                                                  <span className="integration-export-restricted">
                                                    Bị hạn chế / chưa đủ CRS
                                                  </span>
                                                )}
                                              </td>
                                            )}
                                          </tr>
                                        );
                                      })}
                                    </tbody>
                                  </table>
                                </div>
                              )}

                              {adapterExportStatus && (
                                <p
                                  className={
                                    adapterExportStatus.kind === "error"
                                      ? "integration-export-status is-error"
                                      : "integration-export-status"
                                  }
                                  role={
                                    adapterExportStatus.kind === "error"
                                      ? "alert"
                                      : "status"
                                  }
                                >
                                  {adapterExportStatus.message}
                                </p>
                              )}
                            </div>

                            <div className="integration-profile-block">
                              <h4>Ma trận chính sách thao tác</h4>
                              <div className="integration-table-wrap">
                                <table className="integration-table">
                                  <thead>
                                    <tr>
                                      <th scope="col">Thao tác</th>
                                      <th scope="col">Quyền yêu cầu</th>
                                      <th scope="col">Theo vai trò</th>
                                      <th scope="col">Thực thi Hub</th>
                                      <th scope="col">Kết quả</th>
                                      <th scope="col">Lý do</th>
                                    </tr>
                                  </thead>
                                  <tbody>
                                    {profile.adapter.actions.map((evaluation) => (
                                      <tr key={evaluation.action}>
                                        <td>
                                          <code>{evaluation.action}</code>
                                        </td>
                                        <td>
                                          <code>
                                            {evaluation.required_permission}
                                          </code>
                                        </td>
                                        <td>
                                          {evaluation.eligible_by_role
                                            ? "Có"
                                            : "Không"}
                                        </td>
                                        <td>
                                          {evaluation.executable_in_hub
                                            ? "Có"
                                            : "Không"}
                                        </td>
                                        <td>
                                          <span
                                            className={
                                              evaluation.allowed
                                                ? "integration-flag is-allowed"
                                                : "integration-flag is-denied"
                                            }
                                          >
                                            {evaluation.allowed
                                              ? "Cho phép"
                                              : "Từ chối"}
                                          </span>
                                        </td>
                                        <td>
                                          <span>
                                            {
                                              DENIAL_REASON_LABELS[
                                                evaluation.reason
                                              ]
                                            }
                                          </span>
                                          <small className="integration-cell-sub">
                                            <code>{evaluation.reason}</code>
                                          </small>
                                        </td>
                                      </tr>
                                    ))}
                                  </tbody>
                                </table>
                              </div>
                            </div>
                          </div>
                        )}
                      </section>
                    )}
                  </article>
                );
              })}
            </div>

            {!filteredAdapters.length && (
              <p className="live-note">Không có kết nối khớp từ khóa.</p>
            )}
          </>
        )}
      </details>
    </section>
  );
}
