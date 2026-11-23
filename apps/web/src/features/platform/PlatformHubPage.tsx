import {
  activateProjectExtension,
  cogoTraverse,
  disableProjectExtension,
  createExternalConnection,
  getOpsHealthReady,
  getOpsReleaseCapabilities,
  getOrgEntitlement,
  listAllowlistedHosts,
  listCadConversions,
  listExtensionManifests,
  listExternalConnections,
  listOfficialExtensions,
  listProjectActivations,
  loadCsrfToken,
  registerAllowlistedHost,
  registerOfficialExtension
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { ProjectToolbar } from "./ProjectToolbar";
import {
  entitlementTierLabel,
  releaseProfileLabel,
  releaseStatusLabel
} from "./platform-release-state";
import { ExternalSourcePreview } from "./ExternalSourcePreview";
import { MicroStationBridgePanel } from "./MicroStationBridgePanel";
import { CollaborationWorkbenchPanel } from "./CollaborationWorkbenchPanel";
import { CadImportPanel } from "./CadImportPanel";
import { ExtensionWorkbenchPanel } from "./ExtensionWorkbenchPanel";

type HubTab = "external" | "collab" | "cad" | "field" | "extensions" | "ops";

function platformStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    loading: "Đang tải",
    ready: "Sẵn sàng",
    not_ready: "Chưa sẵn sàng",
    pending: "Đang chờ",
    approved: "Đã phê duyệt",
    rejected: "Đã từ chối",
    queued: "Đang chờ",
    active: "Đang xử lý",
    succeeded: "Thành công",
    completed: "Hoàn tất",
    failed: "Thất bại",
    cancelled: "Đã hủy",
    canceled: "Đã hủy"
  };
  return labels[status] ?? status;
}

function releaseCapabilityLabel(id: string, fallback: string): string {
  const labels: Record<string, string> = {
    R001: "Nền tảng hệ thống",
    R002: "Danh tính, tổ chức & phân vùng dữ liệu",
    R003: "Danh mục dữ liệu không gian & hệ tọa độ",
    R004: "Không gian bản đồ",
    R005: "Nhập / Xuất & tác vụ bền vững",
    R006: "Biên tập & lịch sử thay đổi",
    R007: "Truy vấn thuộc tính & không gian",
    R008: "Xử lý không gian & tác vụ",
    R009: "Bản đồ trình bày & xuất bản in",
    R010: "Nguồn dữ liệu ngoài",
    R011: "Cộng tác nhóm & duyệt thay đổi",
    R012: "CAD 2D & COGO chính xác",
    R013: "Hiện trường, ngoại tuyến & đồng bộ",
    R014: "Nền tảng mô-đun mở rộng",
    R015: "Vận hành sản xuất & tự triển khai"
  };
  return labels[id] ?? fallback;
}

function cadDirectionLabel(direction: string): string {
  const labels: Record<string, string> = {
    import: "Nhập",
    export: "Xuất",
    dxf_to_layer: "DXF → Lớp",
    layer_to_dxf: "Lớp → DXF"
  };
  return labels[direction] ?? direction.toUpperCase();
}

export function PlatformHubPage({ embeddedTab }: { embeddedTab?: HubTab } = {}) {
  const { organizationId = "", projectId = "" } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const qc = useQueryClient();
  const [localTab, setLocalTab] = useState<HubTab>(embeddedTab ?? "cad");
  useEffect(() => {
    if (embeddedTab) setLocalTab(embeddedTab);
  }, [embeddedTab]);
  const requestedTab = searchParams.get("tab") as HubTab | null;
  const validTabs: HubTab[] = ["cad", "external", "field", "collab", "extensions", "ops"];
  const tab: HubTab = embeddedTab
    ? localTab
    : requestedTab && validTabs.includes(requestedTab)
      ? requestedTab
      : "cad";
  function setTab(next: HubTab) {
    if (embeddedTab) {
      setLocalTab(next);
    } else {
      setSearchParams({ tab: next });
    }
  }
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // R010 state
  const [extHost, setExtHost] = useState("tiles.example.org");
  const [extUrl, setExtUrl] = useState("https://tiles.example.org/wms");
  const [extProtocol, setExtProtocol] = useState<"wms" | "wfs_ogc_features" | "wmts_xyz" | "cog_geotiff">("wms");
  const [extLayer, setExtLayer] = useState("default");
  const [extAttribution, setExtAttribution] = useState("External Provider");
  const [extLicense, setExtLicense] = useState("Provider License Terms");

  // R012 COGO state
  const [originX, setOriginX] = useState("500000");
  const [originY, setOriginY] = useState("2300000");
  const [dist, setDist] = useState("100");
  const [azimuth, setAzimuth] = useState("90");
  const [cogoOut, setCogoOut] = useState<string | null>(null);

  const allowlistQ = useQuery({
    queryKey: ["hub", organizationId, "allowlist"],
    queryFn: () => listAllowlistedHosts(apiClient, organizationId),
    enabled: Boolean(organizationId)
  });
  const extConnsQ = useQuery({
    queryKey: ["hub", organizationId, projectId, "ext"],
    queryFn: () => listExternalConnections(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const cadQ = useQuery({
    queryKey: ["hub", organizationId, projectId, "cad"],
    queryFn: () => listCadConversions(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const extManifestsQ = useQuery({
    queryKey: ["hub", organizationId, "manifests"],
    queryFn: () => listExtensionManifests(apiClient, organizationId),
    enabled: Boolean(organizationId)
  });
  const officialExtensionsQ = useQuery({
    queryKey: ["hub", organizationId, "official-extensions"],
    queryFn: () => listOfficialExtensions(apiClient, organizationId),
    enabled: Boolean(organizationId)
  });
  const extensionActivationsQ = useQuery({
    queryKey: ["hub", organizationId, projectId, "extension-activations"],
    queryFn: () => listProjectActivations(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const readyQ = useQuery({
    queryKey: ["ops", "ready"],
    queryFn: () => getOpsHealthReady(apiClient)
  });
  const capsQ = useQuery({
    queryKey: ["ops", "caps"],
    queryFn: () => getOpsReleaseCapabilities(apiClient)
  });
  const entQ = useQuery({
    queryKey: ["ops", organizationId, "entitlement"],
    queryFn: () => getOrgEntitlement(apiClient, organizationId),
    enabled: Boolean(organizationId)
  });

  return (
    <div className="ovg-suite" data-testid="platform-hub">
      <ProjectToolbar
        organizationId={organizationId}
        projectId={projectId}
        active="hub"
        title="Trung tâm Nền tảng GIS & Vận hành"
      />
      <main className="ovg-suite-body">
        {!embeddedTab && (
        <div className="ovg-tabs" role="tablist">
          {(
            [
              ["cad", "AutoCAD 2D (DXF) & COGO"],
              ["external", "Tích hợp Bên thứ 3 (WMS / WMTS / COG / WFS)"],
              ["extensions", "Quản lý Plugin & Số nhà"],
              ["field", "Thực địa & Ngoại tuyến (Offline)"],
              ["collab", "Cộng tác nhóm & Duyệt"],
              ["ops", "Vận hành & Sẵn sàng"]
            ] as Array<[HubTab, string]>
          ).map(([k, label]) => (
            <button
              key={k}
              type="button"
              role="tab"
              aria-selected={tab === k}
              className={`ovg-tab ${tab === k ? "active" : ""}`}
              onClick={() => setTab(k)}
            >
              {label}
            </button>
          ))}
        </div>
        )}

        {notice && <div className="ovg-notice">{notice}</div>}
        {error && <div className="ovg-error">{error}</div>}

        {tab === "ops" && (
          <section className="ovg-card" data-testid="ops-readiness-panel">
            <h2>Kiểm tra phụ thuộc & Nghiệm thu phát hành</h2>
            <p>
              <strong>Trạng thái phụ thuộc:</strong> {readyQ.isError ? "Không xác nhận được" : readyQ.data?.status ? platformStatusLabel(readyQ.data.status) : "Đang tải"} ·{" "}
              <strong>PostGIS:</strong> {readyQ.data?.postgis_version ?? "—"}
            </p>
            <p>
              <strong>Hồ sơ phát hành:</strong> {releaseProfileLabel(capsQ.data)}.
              {" "}Phụ thuộc hoạt động không đồng nghĩa với nghiệm thu sản phẩm.
            </p>
            <p>
              <strong>Gói cấu hình:</strong> {entitlementTierLabel(entQ.data?.plan_tier)} · Chỉ là thông tin cấu hình, không thực thi hạn mức hoặc cấp quyền truy cập.
            </p>
            <table className="ovg-table">
              <thead>
                <tr>
                  <th>Năng lực</th>
                  <th>Giai đoạn</th>
                  <th>Nghiệm thu phát hành</th>
                </tr>
              </thead>
              <tbody>
                {(capsQ.data?.items ?? []).map((c) => (
                  <tr key={c.id}>
                    <td>{releaseCapabilityLabel(c.id, c.name)}</td>
                    <td>{c.milestone}</td>
                    <td>
                      <span className="ovg-badge">{releaseStatusLabel(c, capsQ.data)}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {tab === "external" && <MicroStationBridgePanel />}

        {tab === "external" && (
          <section className="ovg-card">
            <h2>Nguồn dữ liệu ngoài (Danh sách cho phép chống SSRF)</h2>
            <div className="ovg-form-row">
              <label>
                Tên miền được phép
                <input value={extHost} onChange={(e) => setExtHost(e.target.value)} />
              </label>
              <button
                type="button"
                className="ovg-btn"
                onClick={async () => {
                  setError(null);
                  try {
                    const csrf = await loadCsrfToken(apiClient);
                    const al = (allowlistQ.data?.items ?? []).find(
                      (host) => host.hostname === extHost.trim().toLowerCase()
                    ) ?? await registerAllowlistedHost(
                      apiClient,
                      organizationId,
                      { hostname: extHost },
                      csrf
                    );
                    await createExternalConnection(
                      apiClient,
                      organizationId,
                      projectId,
                      {
                        allowlist_id: al.id,
                        name: `${extProtocol.toUpperCase()} ${extHost}`,
                        protocol: extProtocol,
                        endpoint_url: extUrl,
                        layer_identifier: extLayer,
                        attribution: extAttribution,
                        license_text: extLicense
                      },
                      csrf
                    );
                    setNotice(`Đã đăng ký cấu hình ${extProtocol.toUpperCase()} cho ${extHost}; chưa xác minh kết nối. Xem mẫu được phép bên dưới.`);
                    await qc.invalidateQueries({ queryKey: ["hub", organizationId] });
                  } catch (err) {
                    setError(err instanceof Error ? err.message : "Không thành công.");
                  }
                }}
              >
                Thêm vào danh sách cho phép & kết nối
              </button>
              <label style={{ flex: 1 }}>
                URL điểm cuối
                <input value={extUrl} onChange={(e) => setExtUrl(e.target.value)} />
              </label>
            </div>
            <div className="ovg-form-row">
              <label>
                Giao thức xem mẫu
                <select
                  value={extProtocol}
                  onChange={(e) => {
                    const value = e.target.value;
                    if (value === "wms" || value === "wfs_ogc_features" ||
                        value === "wmts_xyz" || value === "cog_geotiff") setExtProtocol(value);
                  }}
                >
                  <option value="wms">WMS 1.1.1</option>
                  <option value="wfs_ogc_features">WFS 1.0.0</option>
                  <option value="wmts_xyz">WMTS / XYZ (một tile)</option>
                  <option value="cog_geotiff">GeoTIFF (mẫu range giới hạn)</option>
                </select>
              </label>
              <label>
                Tên lớp / Feature type
                <input value={extLayer} onChange={(e) => setExtLayer(e.target.value)} />
              </label>
              <label>
                Ghi công
                <input value={extAttribution} onChange={(e) => setExtAttribution(e.target.value)} />
              </label>
              <label>
                Quyền sử dụng
                <input value={extLicense} onChange={(e) => setExtLicense(e.target.value)} />
              </label>
            </div>
            <p>Máy chủ trong danh sách cho phép: {(allowlistQ.data?.items ?? []).map((h) => h.hostname).join(", ") || "Không có"}</p>
            <ul>
              {(extConnsQ.data?.items ?? []).map((c) => (
                <li key={c.id}>
                  <strong>{c.name}</strong> ({c.protocol}) — {c.endpoint_url}
                  <ExternalSourcePreview
                    key={`${organizationId}:${projectId}:${c.id}:${c.cache_generation}:${c.lifecycle}`}
                    connection={c}
                    organizationId={organizationId}
                    projectId={projectId}
                  />
                </li>
              ))}
            </ul>
          </section>
        )}

        {tab === "collab" && <CollaborationWorkbenchPanel organizationId={organizationId} projectId={projectId} />}

        {tab === "cad" && (
          <section className="ovg-card">
            <h2>Trao đổi CAD 2D DXF & Tính thử COGO mặt phẳng</h2>
            <CadImportPanel key={`${organizationId}/${projectId}`} organizationId={organizationId} projectId={projectId} onImported={() => { void cadQ.refetch(); }} />
            <div className="ovg-form-row">
              <label>
                Tọa độ gốc X
                <input value={originX} onChange={(e) => setOriginX(e.target.value)} />
              </label>
              <label>
                Tọa độ gốc Y
                <input value={originY} onChange={(e) => setOriginY(e.target.value)} />
              </label>
              <label>
                Khoảng cách (đơn vị tọa độ nhập)
                <input value={dist} onChange={(e) => setDist(e.target.value)} />
              </label>
              <label>
                Góc phương vị (độ)
                <input value={azimuth} onChange={(e) => setAzimuth(e.target.value)} />
              </label>
              <button
                type="button"
                className="ovg-btn"
                onClick={async () => {
                  setError(null);
                  setCogoOut(null);
                  try {
                    const csrf = await loadCsrfToken(apiClient);
                    const res = await cogoTraverse(
                      apiClient,
                      organizationId,
                      projectId,
                      {
                        origin: [Number(originX), Number(originY)],
                        distance: Number(dist),
                        azimuth_degrees: Number(azimuth)
                      },
                      csrf
                    );
                    if (
                      res.coordinate_reference !== "unreferenced_planar_input" ||
                      res.units !== "input_coordinate_units" ||
                      res.survey_grade !== false || res.published !== false
                    ) {
                      throw new Error("Máy chủ chưa xác nhận đây là kết quả tính thử mặt phẳng.");
                    }
                    setCogoOut(
                      `Tọa độ điểm tính được: (${res.endpoint[0].toFixed(4)}, ${res.endpoint[1].toFixed(4)}) · tính thử, chưa gắn CRS`
                    );
                  } catch (err) {
                    setError(err instanceof Error ? err.message : "Tính toán COGO không thành công.");
                  }
                }}
              >
                Tính tọa độ điểm cuối
              </button>
            </div>
            {cogoOut && <p className="ovg-notice">{cogoOut}</p>}
            <h3>Chuyển đổi CAD ({cadQ.data?.items.length ?? 0})</h3>
            <ul>
              {(cadQ.data?.items ?? []).map((c) => (
                <li key={c.id}>
                  {cadDirectionLabel(c.direction)} {c.filename} ({c.cad_format}, EPSG:{c.crs_code}) —{" "}
                  {platformStatusLabel(c.status)}
                </li>
              ))}
            </ul>
          </section>
        )}

        {tab === "field" && (
          <section className="ovg-card">
            <h2>Không gian hiện trường ngoại tuyến</h2>
            <Link
              className="ovg-btn"
              to={`/offline-field/${encodeURIComponent(organizationId)}/${encodeURIComponent(projectId)}`}
            >
              Mở không gian hiện trường
            </Link>
          </section>
        )}

        {tab === "extensions" && (
          <>
            <section className="ovg-card">
              <h2>Module chính thức (Official Modules)</h2>
              <table className="ovg-table">
                <thead>
                  <tr>
                    <th>Module official</th><th>Phiên bản</th><th>Trạng thái</th><th>Thao tác</th>
                  </tr>
                </thead>
                <tbody>
                  {(officialExtensionsQ.data?.items ?? []).map((official) => {
                    const manifest = (extManifestsQ.data?.items ?? []).find(
                      (item) => item.extension_key === official.extension_key && item.version === official.version
                    );
                    const activation = (extensionActivationsQ.data?.items ?? []).find(
                      (item) => item.extension_key === official.extension_key
                    );
                    const contribution = official.ui_contributions.find(
                      (item) => item.slot === "project_route"
                    );
                    const suffix = typeof contribution?.route_suffix === "string"
                      ? contribution.route_suffix
                      : `/extensions/${official.extension_key}`;
                    return (
                      <tr key={official.extension_key}>
                        <td><strong>{official.display_name}</strong><br /><small>{official.description}</small></td>
                        <td><code>{official.version}</code></td>
                        <td>{activation?.status ?? (manifest ? "registered" : "available")}</td>
                        <td>
                          <div className="ovg-form-row">
                            {!manifest && (
                              <button className="ovg-btn" type="button" onClick={async () => {
                                setError(null);
                                try {
                                  const csrf = await loadCsrfToken(apiClient);
                                  await registerOfficialExtension(apiClient, organizationId, official.extension_key, csrf);
                                  setNotice(`Đã đăng ký ${official.display_name}.`);
                                  await Promise.all([
                                    qc.invalidateQueries({ queryKey: ["hub", organizationId] }),
                                    qc.invalidateQueries({ queryKey: ["extensions", organizationId, "manifests"] })
                                  ]);
                                } catch (err) { setError(err instanceof Error ? err.message : "Đăng ký thất bại."); }
                              }}>Đăng ký</button>
                            )}
                            {manifest && activation?.status !== "active" && (
                              <button className="ovg-btn" type="button" onClick={async () => {
                                setError(null);
                                try {
                                  const csrf = await loadCsrfToken(apiClient);
                                  await activateProjectExtension(apiClient, organizationId, projectId, manifest.id, csrf);
                                  setNotice(`Đã kích hoạt ${official.display_name}.`);
                                  await Promise.all([
                                    qc.invalidateQueries({ queryKey: ["hub", organizationId, projectId] }),
                                    qc.invalidateQueries({ queryKey: ["extensions", organizationId, projectId, "activations"] })
                                  ]);
                                } catch (err) { setError(err instanceof Error ? err.message : "Kích hoạt thất bại."); }
                              }}>Kích hoạt</button>
                            )}
                            {activation?.status === "active" && (
                              <>
                                <Link className="ovg-btn" to={`/organizations/${organizationId}/projects/${projectId}${suffix}`}>Mở module</Link>
                                <button className="ovg-btn ovg-btn-secondary" type="button" onClick={async () => {
                                  setError(null);
                                  try {
                                    const csrf = await loadCsrfToken(apiClient);
                                    await disableProjectExtension(apiClient, organizationId, projectId, official.extension_key, csrf);
                                    setNotice(`Đã tắt ${official.display_name}; dữ liệu được giữ nguyên.`);
                                    await Promise.all([
                                      qc.invalidateQueries({ queryKey: ["hub", organizationId, projectId] }),
                                      qc.invalidateQueries({ queryKey: ["extensions", organizationId, projectId, "activations"] })
                                    ]);
                                  } catch (err) { setError(err instanceof Error ? err.message : "Tắt module thất bại."); }
                                }}>Tắt</button>
                              </>
                            )}
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </section>
            <ExtensionWorkbenchPanel organizationId={organizationId} projectId={projectId} />
          </>
        )}
      </main>
    </div>
  );
}
