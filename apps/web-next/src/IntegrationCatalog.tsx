import { useEffect, useMemo, useRef, useState } from "react";
import {
  ApiError,
  downloadIntegrationLayerExport,
  getIntegrationHubAdapterProfile,
  getIntegrationHubCatalog,
  loadCsrfToken,
  previewQgisGpkgSync,
  pushQgisGpkgSync,
  type ActionDenialReason,
  type AdapterExecutionStatus,
  type AdapterLayerDescriptor,
  type ApiClient,
  type ConnectorLifecycle,
  type IntegrationAdapterProfileResponse,
  type IntegrationAdapterSummary,
  type IntegrationCatalogResponse,
  type IntegrationExportAdapterKey,
  type IntegrationExportFormat,
  type IntegrationSyncPreviewResponse,
  type IntegrationSyncPushResponse
} from "@openvietgis/client-sdk";
import { QgisBindingManager } from "./QgisBindingManager";
import { zipFiles } from "./zip";
import "./integration-catalog.css";

interface IntegrationCatalogProps {
  client: ApiClient;
  org: string;
  projectId: string;
}

type IntegrationTask = "export" | "sync" | "service" | "details" | "bindings";

const CONNECTOR_HELP: Record<string, string> = {
  qgis: "Đồng bộ 2 chiều Realtime (R029) hoặc xuất GeoPackage về QGIS Desktop.",
  postgis: "Xuất SQL hoặc GeoPackage sang cơ sở dữ liệu PostgreSQL / PostGIS.",
  autocad: "Xuất bản vẽ DXF 2D chuẩn hệ tọa độ phẳng để mở trong AutoCAD.",
  ogc_services: "Kết nối dịch vụ bản đồ WMS, WFS, COG và XYZ từ nguồn ngoài.",
  microstation_v8i: "Cầu nối DGN V8i dành cho máy trạm Windows có MicroStation.",
  arcgis: "Kết nối ArcGIS chuyên dụng (đang phát triển).",
  mapinfo: "Trao đổi dữ liệu MapInfo TAB/MIF/MID (đang phát triển).",
  cadastral_vn: "Liên thông gCadas, FAMIS, TMV.Map (đang phát triển).",
  ogc_publisher: "Xuất bản dịch vụ OGC (đang phát triển).",
  google_earth: "Xuất/nhập Google Earth KML/KMZ (đang phát triển).",
  excel: "Liên thông bảng tính Excel / CSV (đang phát triển).",
  survey_3d: "Dữ liệu đo đạc RTK, UAV, BIM & 3D (đang phát triển)."
};

function AdapterLogo({ adapterKey }: { adapterKey: string }) {
  switch (adapterKey) {
    case "qgis":
      return (
        <span className="integration-logo is-qgis" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <circle cx="19" cy="19" r="12" stroke="#589632" strokeWidth="5.2" />
            <path d="M21 21L33 33" stroke="#93B023" strokeWidth="5.2" strokeLinecap="round" />
            <path d="M20 20L27 27" stroke="#EE7913" strokeWidth="3.2" strokeLinecap="round" />
          </svg>
        </span>
      );
    case "postgis":
      return (
        <span className="integration-logo is-postgis" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <ellipse cx="20" cy="11" rx="11" ry="4.5" fill="#336791" />
            <path d="M9 11V29C9 31.5 13.9 33.5 20 33.5C26.1 33.5 31 31.5 31 29V11" stroke="#336791" strokeWidth="2.8" fill="#E0F2FE" />
            <path d="M9 20C9 22.5 13.9 24.5 20 24.5C26.1 24.5 31 22.5 31 20" stroke="#0284C7" strokeWidth="2.4" />
            <circle cx="28" cy="27" r="5" fill="#0284C7" stroke="#FFFFFF" strokeWidth="1.8" />
          </svg>
        </span>
      );
    case "autocad":
      return (
        <span className="integration-logo is-autocad" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <path d="M20 6L32 33H25.8L23.4 27.2H16.6L14.2 33H8L20 6Z" fill="#E51937" />
            <path d="M20 14.5L17.9 22.5H22.1L20 14.5Z" fill="#FFFFFF" />
            <path d="M20 6L26 20L20 15L14 20L20 6Z" fill="#B91C1C" opacity="0.45" />
          </svg>
        </span>
      );
    case "ogc_services":
    case "ogc_publisher":
      return (
        <span className="integration-logo is-ogc" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <circle cx="20" cy="20" r="13" stroke="#0F766E" strokeWidth="2.6" fill="#CCFBF1" />
            <ellipse cx="20" cy="20" rx="6.5" ry="13" stroke="#0EA5E9" strokeWidth="2.2" />
            <path d="M7.5 20H32.5M10 13.5H30M10 26.5H30" stroke="#0F766E" strokeWidth="2" />
          </svg>
        </span>
      );
    case "microstation_v8i":
      return (
        <span className="integration-logo is-v8i" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <polygon points="20,6 33,13.5 33,26.5 20,34 7,26.5 7,13.5" fill="#E0F2FE" stroke="#0072CE" strokeWidth="2.6" />
            <path d="M13 25V15L20 20L27 15V25" stroke="#0072CE" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </span>
      );
    case "excel":
      return (
        <span className="integration-logo is-excel" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <rect x="7" y="8" width="26" height="24" rx="4" fill="#DCFCE7" stroke="#1D6F42" strokeWidth="2.4" />
            <path d="M15 14L25 26M25 14L15 26" stroke="#1D6F42" strokeWidth="3" strokeLinecap="round" />
          </svg>
        </span>
      );
    default:
      return (
        <span className="integration-logo is-generic" aria-hidden="true">
          <svg viewBox="0 0 40 40" width="28" height="28" fill="none">
            <polygon points="20,7 34,15 20,23 6,15" fill="#DBEAFE" stroke="#2563EB" strokeWidth="2.4" />
            <path d="M6 21L20 29L34 21M6 27L20 35L34 27" stroke="#2563EB" strokeWidth="2.2" strokeLinecap="round" />
          </svg>
        </span>
      );
  }
}

interface LayerExportStatus {
  kind: "success" | "error";
  filename?: string;
  crs?: string;
  featureCount?: number | null;
  shortSha256?: string | null;
  attributeLoss?: boolean;
  message: string;
}

const LIFECYCLE_LABELS: Record<ConnectorLifecycle, string> = {
  existing_read_only: "Có thể sử dụng",
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
  controlled_changeset_sync: "Đồng bộ qua Changeset có kiểm soát",
  permission_denied: "Không đủ quyền dự án",
  unsupported_by_adapter: "Adapter không khai báo thao tác",
  not_implemented: "Chưa triển khai trong Integration Hub"
};

const EXPORT_FORMAT_LABELS: Record<IntegrationExportFormat, string> = {
  geopackage: "GeoPackage (.gpkg)",
  geojson: "GeoJSON (.geojson)",
  postgis_sql: "PostGIS SQL (.sql)",
  dxf: "AutoCAD DXF (.dxf)"
};
const EXPORT_FORMAT_EXT: Record<IntegrationExportFormat, string> = { geopackage: "gpkg", geojson: "geojson", postgis_sql: "sql", dxf: "dxf" };

const CATEGORY_LABELS: Record<string, string> = {
  gis: "GIS",
  cad: "CAD",
  cadastral: "Địa chính",
  database: "Cơ sở dữ liệu",
  tabular: "Bảng tính",
  survey_3d: "Trắc địa / 3D"
};

function isExportAdapterKey(key: string): key is IntegrationExportAdapterKey {
  return key === "qgis" || key === "postgis" || key === "autocad";
}

function isAutocadMetricCrs(storedCrsCode: number | null): boolean {
  return storedCrsCode === 5899 || storedCrsCode === 3857;
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
      return `Không thể xuất lớp "${layerName}" theo định dạng đã chọn (HTTP 422): tham số không hợp lệ, GeoJSON yêu cầu EPSG:4326 hoặc DXF yêu cầu hệ tọa độ phẳng mét (EPSG:5899/3857).`;
    }
    if (error.status === 404) {
      return `Không tìm thấy lớp "${layerName}" hoặc đầu cuối xuất chỉ đọc (HTTP 404).`;
    }
  }
  return `Không thể tải tệp xuất chỉ đọc cho lớp "${layerName}". Vui lòng thử lại.`;
}

function formatSyncErrorMessage(
  step: "preview" | "push",
  layerName: string,
  error: unknown
): string {
  const stepLabel =
    step === "preview"
      ? "xem trước thay đổi (Diff)"
      : "xác nhận đẩy Changeset";
  if (error instanceof ApiError) {
    if (error.status === 403) {
      return `Không có quyền ${stepLabel} cho lớp "${layerName}" (HTTP 403). Kiểm tra quyền project.edit hoặc phạm vi GIS (ACL).`;
    }
    if (error.status === 409) {
      return `Xung đột phiên bản hoặc mã kiểm tra diff khi ${stepLabel} cho lớp "${layerName}" (HTTP 409). Hãy tải lại hồ sơ và xem trước lại.`;
    }
    if (error.status === 413) {
      return `Tệp GeoPackage vượt giới hạn kích thước tối đa 16 MiB khi ${stepLabel} cho lớp "${layerName}" (HTTP 413).`;
    }
    if (error.status === 422) {
      return `Tệp GeoPackage không hợp lệ hoặc sai hệ tọa độ CRS khi ${stepLabel} cho lớp "${layerName}" (HTTP 422).`;
    }
    if (error.status === 404) {
      return `Không tìm thấy lớp "${layerName}" hoặc đầu cuối đồng bộ (HTTP 404).`;
    }
  }
  return `Không thể ${stepLabel} cho lớp "${layerName}". Vui lòng kiểm tra tệp .gpkg và thử lại.`;
}

function generateSyncOperationId(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  return `qgis-sync-${Date.now()}-${Math.random().toString(16).slice(2, 10)}`;
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

// Every format the server writes, whichever connector tab is open; the server endpoint is chosen by format.
const BATCH_FORMATS: IntegrationExportFormat[] = ["geopackage", "geojson", "dxf", "postgis_sql"];
const BATCH_ADAPTER: Record<IntegrationExportFormat, IntegrationExportAdapterKey> = { geopackage: "qgis", geojson: "qgis", dxf: "autocad", postgis_sql: "postgis" };
const BATCH_DEFAULT: Record<IntegrationExportAdapterKey, IntegrationExportFormat> = { qgis: "geopackage", postgis: "postgis_sql", autocad: "dxf" };

// Mirrors the server: GeoJSON is written in EPSG:4326 (3857 is converted), DXF in metres (4326 is converted to 3857).
function batchBlockReason(layer: AdapterLayerDescriptor, format: IntegrationExportFormat): string | null {
  if (!layer.export_permitted) return "Không được phép xuất";
  if (!layer.crs_explicit) return "Chưa xác định hệ tọa độ";
  if (format === "dxf" && !isAutocadMetricCrs(layer.stored_crs_code) && layer.stored_crs_code !== 4326) return "DXF cần hệ tọa độ mét hoặc EPSG:4326";
  if (format === "geojson" && layer.stored_crs_code !== 4326 && layer.stored_crs_code !== 3857) return "GeoJSON cần EPSG:4326 hoặc 3857";
  return null;
}

function BatchExport({ client, org, projectId, adapterKey, layers }: IntegrationCatalogProps & { adapterKey: IntegrationExportAdapterKey; layers: AdapterLayerDescriptor[] }) {
  const [format, setFormat] = useState<IntegrationExportFormat>(BATCH_DEFAULT[adapterKey]);
  // Track what the user unticked, so every exportable layer starts selected and survives format changes.
  const [unticked, setUnticked] = useState<Set<string>>(() => new Set());
  const [progress, setProgress] = useState<string | null>(null);
  const [result, setResult] = useState<{ ok: number; failed: string[] } | null>(null);
  const run = useRef(0);
  useEffect(() => () => { run.current++; }, []);
  const eligible = layers.filter((layer) => !batchBlockReason(layer, format));
  const chosen = eligible.filter((layer) => !unticked.has(layer.layer_id));
  const busy = progress !== null;

  const start = async () => {
    const id = ++run.current;
    const failed: string[] = [];
    const files: { name: string; blob: Blob }[] = [];
    setResult(null);
    for (const [index, layer] of chosen.entries()) {
      setProgress(`Đang chuẩn bị ${index + 1}/${chosen.length}: ${layer.name}`);
      try {
        const file = await downloadIntegrationLayerExport(client, org, projectId, BATCH_ADAPTER[format], layer.layer_id, format);
        if (run.current !== id) return;
        files.push({ name: file.filename, blob: file.blob });
      } catch (exportError: unknown) {
        if (run.current !== id) return;
        failed.push(formatExportErrorMessage(layer.name, exportError));
      }
    }
    if (files.length === 1) {
      triggerBrowserBlobDownload(files[0]!.blob, files[0]!.name);
    } else if (files.length > 1) {
      setProgress("Đang nén ZIP…");
      const stamp = new Date().toISOString().slice(0, 16).replace(/[-:T]/g, "");
      triggerBrowserBlobDownload(await zipFiles(files), `openvietgis-${files.length}-lop-${EXPORT_FORMAT_EXT[format]}-${stamp}.zip`);
    }
    setProgress(null);
    setResult({ ok: files.length, failed });
  };

  return (
    <section className="integration-batch-export" aria-label="Xuất dữ liệu">
      <div className="integration-batch-head">
        <h3>Xuất dữ liệu</h3>
        <label>
          Định dạng{" "}
          <select value={format} disabled={busy} onChange={(event) => setFormat(event.target.value as IntegrationExportFormat)}>
            {BATCH_FORMATS.map((option) => <option key={option} value={option}>{EXPORT_FORMAT_LABELS[option]}</option>)}
          </select>
        </label>
        <label>
          <input
            type="checkbox"
            checked={eligible.length > 0 && chosen.length === eligible.length}
            disabled={busy || eligible.length === 0}
            onChange={(event) => setUnticked(new Set(event.target.checked ? [] : eligible.map((layer) => layer.layer_id)))}
          />{" "}
          Chọn tất cả ({eligible.length})
        </label>
        <button type="button" className="integration-batch-download" disabled={busy || chosen.length === 0} onClick={() => { void start(); }}>
          {busy ? "Đang tải…" : chosen.length > 1 ? `⬇ Tải ${chosen.length} lớp (.zip)` : chosen.length === 1 ? "⬇ Tải 1 lớp" : "Chọn lớp để tải"}
        </button>
      </div>
      {layers.length === 0 ? (
        <p className="integration-profile-empty">Không có lớp dữ liệu nào trong phạm vi quyền hiện tại.</p>
      ) : (
        <ul className="integration-batch-list">
          {layers.map((layer) => {
            const reason = batchBlockReason(layer, format);
            return (
              <li key={layer.layer_id}>
                <label className={reason ? "is-blocked" : undefined}>
                  <input
                    type="checkbox"
                    disabled={busy || reason !== null}
                    checked={reason === null && !unticked.has(layer.layer_id)}
                    onChange={(event) => setUnticked((prev) => {
                      const next = new Set(prev);
                      if (event.target.checked) next.delete(layer.layer_id); else next.add(layer.layer_id);
                      return next;
                    })}
                  />
                  <span>
                    <strong>{layer.name}</strong>
                    <small>{reason ?? formatLayerCrs(layer)}</small>
                  </span>
                </label>
              </li>
            );
          })}
        </ul>
      )}
      {progress && <p className="integration-profile-status" role="status">{progress}</p>}
      {result && (
        <div role="status" className={result.failed.length ? "integration-batch-result has-errors" : "integration-batch-result"}>
          <p>{result.ok > 1 ? `Đã nén ${result.ok} lớp vào một tệp ZIP` : `Đã tải ${result.ok} lớp`}{result.failed.length ? `, lỗi ${result.failed.length} lớp:` : "."}</p>
          {result.failed.length > 0 && <ul>{result.failed.map((message) => <li key={message}>{message}</li>)}</ul>}
        </div>
      )}
    </section>
  );
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
  const [categoryFilter, setCategoryFilter] = useState<string>("ready");
  const [activeTask, setActiveTask] = useState<IntegrationTask | null>(null);

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

  const [syncLayerId, setSyncLayerId] = useState<string | null>(null);
  const [syncFile, setSyncFile] = useState<File | null>(null);
  const [syncPreviewLoading, setSyncPreviewLoading] = useState(false);
  const [syncPushLoading, setSyncPushLoading] = useState(false);
  const [syncPreview, setSyncPreview] =
    useState<IntegrationSyncPreviewResponse | null>(null);
  const [syncPushResult, setSyncPushResult] =
    useState<IntegrationSyncPushResponse | null>(null);
  const [syncError, setSyncError] = useState("");

  const scopeGeneration = useRef(0);

  const resetSyncPanelState = () => {
    setSyncLayerId(null);
    setSyncFile(null);
    setSyncPreviewLoading(false);
    setSyncPushLoading(false);
    setSyncPreview(null);
    setSyncPushResult(null);
    setSyncError("");
  };

  useEffect(() => {
    let alive = true;
    scopeGeneration.current += 1;

    setCatalog(null);
    setError("");
    setLoading(true);
    setExpandedKey(null);
    setActiveTask(null);
    setQuery("");
    setCategoryFilter("ready");
    setProfiles({});
    setProfileLoadingKey(null);
    setProfileError({});
    setExportingKey(null);
    setExportStatusByAdapter({});
    resetSyncPanelState();

    void getIntegrationHubCatalog(client, org, projectId)
      .then((value) => {
        if (alive) {
          setCatalog(value);
          // Auto-select QGIS two-way sync immediately so the user lands directly on the connection panel.
          const qgisAdapter = value.adapters.find((a) => a.key === "qgis");
          if (qgisAdapter) {
            setExpandedKey("qgis");
            setActiveTask("bindings");
            void getIntegrationHubAdapterProfile(client, org, projectId, "qgis")
              .then((profile) => {
                if (alive) {
                  setProfiles((prev) => ({ ...prev, qgis: profile }));
                }
              })
              .catch(() => {});
          }
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
    const lifecycleRank: Record<ConnectorLifecycle, number> = {
      existing_read_only: 0,
      candidate: 1,
      planned: 2
    };
    const keyPriority: Record<string, number> = {
      qgis: 0,
      postgis: 1,
      autocad: 2,
      ogc_services: 3
    };
    const sorted = [...catalog.adapters].sort((a, b) => {
      const rankDiff = lifecycleRank[a.lifecycle] - lifecycleRank[b.lifecycle];
      if (rankDiff !== 0) {
        return rankDiff;
      }
      return (keyPriority[a.key] ?? 99) - (keyPriority[b.key] ?? 99);
    });
    const byCategory = sorted.filter((adapter) => {
      if (categoryFilter === "all") {
        return true;
      }
      if (categoryFilter === "ready") {
        return adapter.lifecycle === "existing_read_only";
      }
      return adapter.category === categoryFilter;
    });
    const normalized = query.trim().toLocaleLowerCase("vi");
    if (!normalized) {
      return byCategory;
    }
    return byCategory.filter((adapter) => {
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
  }, [catalog, categoryFilter, query]);

  const visibleAdapters = expandedKey
    ? catalog?.adapters.filter((adapter) => adapter.key === expandedKey) ?? []
    : filteredAdapters;

  const inspectAdapterProfile = (adapter: IntegrationAdapterSummary) => {
    if (expandedKey === adapter.key) {
      setActiveTask(null);
      setExpandedKey(null);
      if (adapter.key === "qgis") {
        resetSyncPanelState();
      }
      return;
    }

    if (expandedKey === "qgis" && adapter.key !== "qgis") {
      resetSyncPanelState();
    }

    // Open directly into the primary workflow without forcing an extra Step 2 click:
    // QGIS -> Two-Way Realtime Sync ("bindings"); Export-only -> "export"; OGC -> "service".
    if (adapter.key === "qgis") {
      setActiveTask("bindings");
    } else if (adapter.lifecycle === "existing_read_only" && isExportAdapterKey(adapter.key)) {
      setActiveTask("export");
    } else if (adapter.key === "ogc_services") {
      setActiveTask("service");
    } else {
      setActiveTask(null);
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
        if (result.attributeLoss) {
          summaryParts.push(
            "Lưu ý: chỉ xuất hình học (không bảo toàn thuộc tính)"
          );
        }

        setExportStatusByAdapter((prev) => ({
          ...prev,
          [adapterKey]: {
            kind: "success",
            filename: result.filename,
            crs: resolvedCrs,
            featureCount: result.featureCount,
            shortSha256,
            attributeLoss: result.attributeLoss,
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

  const toggleSyncLayerPanel = (layerId: string) => {
    if (syncLayerId === layerId) {
      resetSyncPanelState();
      return;
    }
    setSyncLayerId(layerId);
    setSyncFile(null);
    setSyncPreview(null);
    setSyncPushResult(null);
    setSyncError("");
  };

  const handleSyncPreview = (layer: AdapterLayerDescriptor) => {
    if (!syncFile) {
      return;
    }
    const requestGeneration = scopeGeneration.current;
    setSyncPreviewLoading(true);
    setSyncPreview(null);
    setSyncPushResult(null);
    setSyncError("");

    void (async () => {
      const csrfToken = await loadCsrfToken(client);
      return previewQgisGpkgSync(client, org, projectId, layer.layer_id, {
        file: syncFile,
        filename: syncFile.name,
        baseContentRevision: layer.content_revision,
        baseSchemaRevision: layer.schema_revision,
        csrfToken
      });
    })()
      .then((preview) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setSyncPreview(preview);
      })
      .catch((previewError: unknown) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setSyncError(formatSyncErrorMessage("preview", layer.name, previewError));
      })
      .finally(() => {
        if (scopeGeneration.current === requestGeneration) {
          setSyncPreviewLoading(false);
        }
      });
  };

  const handleSyncPush = (layer: AdapterLayerDescriptor) => {
    if (!syncFile || !syncPreview || !syncPreview.can_commit) {
      return;
    }
    const requestGeneration = scopeGeneration.current;
    const expectedDiffSha256 = syncPreview.diff_sha256;
    const operationId = generateSyncOperationId();

    setSyncPushLoading(true);
    setSyncError("");

    void (async () => {
      const csrfToken = await loadCsrfToken(client);
      const pushResult = await pushQgisGpkgSync(
        client,
        org,
        projectId,
        layer.layer_id,
        {
          file: syncFile,
          filename: syncFile.name,
          baseContentRevision: layer.content_revision,
          baseSchemaRevision: layer.schema_revision,
          operationId,
          expectedDiffSha256,
          summary: `QGIS GeoPackage sync (${syncFile.name})`,
          csrfToken
        }
      );
      let refreshedProfile: IntegrationAdapterProfileResponse | null;
      try {
        refreshedProfile = await getIntegrationHubAdapterProfile(
          client,
          org,
          projectId,
          "qgis"
        );
      } catch {
        refreshedProfile = null;
      }
      return { pushResult, refreshedProfile };
    })()
      .then(({ pushResult, refreshedProfile }) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setSyncPushResult(pushResult);
        setSyncPreview(null);
        if (refreshedProfile) {
          setProfiles((prev) => ({ ...prev, qgis: refreshedProfile }));
        }
      })
      .catch((pushError: unknown) => {
        if (scopeGeneration.current !== requestGeneration) {
          return;
        }
        setSyncError(formatSyncErrorMessage("push", layer.name, pushError));
      })
      .finally(() => {
        if (scopeGeneration.current === requestGeneration) {
          setSyncPushLoading(false);
        }
      });
  };

  return (
    <section className="integration-catalog" aria-label="Tích hợp phần mềm">
      <div className="integration-guided-shell is-minimal">
        <header className="integration-topbar">
          {catalog && (
            <div className="integration-logo-switcher" role="tablist" aria-label="Chọn nhanh phần mềm">
              {catalog.adapters
                .filter((a) => a.lifecycle === "existing_read_only")
                .sort((a, b) => {
                  const order: Record<string, number> = { qgis: 0, postgis: 1, autocad: 2, ogc_services: 3 };
                  return (order[a.key] ?? 9) - (order[b.key] ?? 9);
                })
                .map((adapter) => {
                  const active = expandedKey === adapter.key;
                  return (
                    <button
                      key={adapter.key}
                      type="button"
                      role="tab"
                      aria-selected={active}
                      className={"integration-logo-tab" + (active ? " is-active" : "")}
                      onClick={() => {
                        if (expandedKey !== adapter.key) {
                          inspectAdapterProfile(adapter);
                        }
                      }}
                    >
                      <AdapterLogo adapterKey={adapter.key} />
                      <span>{adapter.name}</span>
                    </button>
                  );
                })}
              <button
                type="button"
                className={"integration-logo-tab is-more" + (!expandedKey ? " is-active" : "")}
                onClick={() => { setExpandedKey(null); setActiveTask(null); resetSyncPanelState(); }}
              >
                <span>Tất cả ({catalog.adapters.length})</span>
              </button>
            </div>
          )}
        </header>

        {loading && <p role="status">Đang kiểm tra danh mục…</p>}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}

        {catalog && (
          <>
            {!expandedKey && (
              <div className="integration-compact-toolbar">
                <div className="integration-compact-step">
                  <span className="integration-step-label">BƯỚC 1</span>
                  <h3>Chọn phần mềm</h3>
                </div>
                <div className="integration-compact-controls">
                  <select
                    aria-label="Lọc nhóm phần mềm"
                    value={categoryFilter}
                    onChange={(event) => setCategoryFilter(event.target.value)}
                  >
                    <option value="ready">
                      ★ Sẵn sàng ({catalog.adapters.filter((a) => a.lifecycle === "existing_read_only").length})
                    </option>
                    <option value="all">Tất cả ({catalog.adapters.length})</option>
                    <option value="gis">GIS (QGIS · OGC)</option>
                    <option value="cad">CAD (AutoCAD · V8i)</option>
                    <option value="database">CSDL (PostGIS)</option>
                    <option value="cadastral">Địa chính</option>
                    <option value="survey_3d">Trắc địa / 3D</option>
                    <option value="tabular">Bảng tính</option>
                  </select>
                  <input
                    type="search"
                    aria-label="Tìm phần mềm hoặc định dạng"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="Tìm QGIS, PostGIS, AutoCAD, WFS…"
                  />
                </div>
              </div>
            )}

            <div className={"integration-list" + (expandedKey ? " is-focused" : "")}>
              {visibleAdapters.map((adapter) => {
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

                const profilePushAction =
                  profile?.adapter.actions.find(
                    (item) => item.action === "sync.push"
                  ) ?? pushAction;
                const qgisSyncPushAllowed =
                  adapter.key === "qgis" && profilePushAction?.allowed === true;
                const selectedSyncLayer =
                  qgisSyncPushAllowed && profile && syncLayerId
                    ? profile.layers.find(
                        (item) =>
                          item.layer_id === syncLayerId && item.crs_explicit
                      ) ?? null
                    : null;

                return (
                  <article
                    key={adapter.key}
                    className={`integration-item${isExpanded ? " is-expanded" : ""}${
                      adapter.lifecycle === "existing_read_only"
                        ? " is-ready-card"
                        : ""
                    }`}
                    data-adapter-key={adapter.key}
                  >
                    {!isExpanded && (
                      <>
                        <div className="integration-item-header">
                          <div className="integration-item-title">
                            <AdapterLogo adapterKey={adapter.key} />
                            <div>
                              <b>{adapter.name}</b>
                              <div className="integration-meta">
                                <span>{adapter.protocols.join(" · ")}</span>
                              </div>
                            </div>
                          </div>
                          <span className={lifecycleBadgeClass(adapter.lifecycle)}>
                            {LIFECYCLE_LABELS[adapter.lifecycle]}
                          </span>
                        </div>

                        <p>{CONNECTOR_HELP[adapter.key] ?? adapter.note}</p>

                        <div className="integration-item-footer">
                          <button
                            type="button"
                            className="integration-inspect-toggle"
                            aria-expanded={isExpanded}
                            aria-controls={detailPanelId}
                            onClick={() => inspectAdapterProfile(adapter)}
                          >
                            {adapter.lifecycle === "existing_read_only"
                              ? "Sử dụng kết nối này"
                              : "Xem tình trạng phát triển"}
                          </button>
                        </div>
                      </>
                    )}

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
                            <details className="integration-export-disclosure" open>
                              <summary>{`Thao tác với ${adapter.name}`}</summary>
                              <section className="integration-task-intro" aria-label="Chọn thao tác">
                                <span className="integration-step-label">BƯỚC 2</span>
                                <h3>Bạn muốn làm gì với {adapter.name}?</h3>
                                {adapter.lifecycle !== "existing_read_only" ? (
                                  <p className="integration-unavailable" role="status">Kết nối này chưa trao đổi dữ liệu trực tiếp trên live. Tên phần mềm trong danh mục không có nghĩa là đã liên thông.</p>
                                ) : (
                                  <div className="integration-task-grid">
                                    {isExportAdapterKey(adapter.key) && (
                                      <button type="button" className={"integration-task-option" + (activeTask === "export" ? " is-active" : "")} aria-pressed={activeTask === "export"} onClick={() => { setActiveTask("export"); resetSyncPanelState(); }}>
                                        <span className="integration-task-icon">↓</span>
                                        <strong>{adapter.key === "autocad" ? "Tải bản vẽ DXF" : "Tải dữ liệu về máy"}</strong>
                                        <small>{adapter.key === "autocad" ? "Xuất DXF 2D để mở bằng AutoCAD" : adapter.key === "postgis" ? "Tải SQL hoặc GeoPackage để nhập vào PostGIS riêng" : "Chức năng tải xuất (.gpkg / .geojson) — nên dùng ở mục Export"}</small>
                                      </button>
                                    )}
                                    {adapter.key === "qgis" && (
                                      <button type="button" className={"integration-task-option" + (activeTask === "sync" ? " is-active" : "")} aria-pressed={activeTask === "sync"} disabled={!profile.adapter.actions.some((action) => action.action === "sync.push" && action.allowed)} onClick={() => { setActiveTask("sync"); resetSyncPanelState(); }}>
                                        <span className="integration-task-icon">⇄</span>
                                        <strong>Gửi chỉnh sửa trở lại</strong>
                                        <small>{profile.adapter.actions.some((action) => action.action === "sync.push" && action.allowed) ? "Đẩy tệp .gpkg thủ công (xem trước diff rồi xác nhận)" : "Cần quyền chỉnh sửa dự án (project.edit)"}</small>
                                      </button>
                                    )}
                                    {adapter.key === "qgis" && (
                                      <button
                                        type="button"
                                        className={"integration-task-option" + (activeTask === "bindings" ? " is-active" : "")}
                                        aria-pressed={activeTask === "bindings"}
                                        onClick={() => { setActiveTask("bindings"); resetSyncPanelState(); }}
                                      >
                                        <span className="integration-task-icon">⌘</span>
                                        <strong>Đồng bộ 2 chiều QGIS (R029)</strong>
                                        <small>Kết nối realtime & mã ghép nối ở ngay phía dưới.</small>
                                      </button>
                                    )}
                                    {adapter.key === "ogc_services" && (
                                      <button type="button" className={"integration-task-option" + (activeTask === "service" ? " is-active" : "")} aria-pressed={activeTask === "service"} onClick={() => setActiveTask("service")}>
                                        <span className="integration-task-icon">◎</span>
                                        <strong>Xem kết nối dịch vụ</strong>
                                        <small>WMS/WFS/COG/XYZ dùng chức năng nguồn ngoài</small>
                                      </button>
                                    )}
                                  </div>
                                )}
                                <button type="button" className="integration-advanced-trigger" aria-pressed={activeTask === "details"} onClick={() => { setActiveTask(activeTask === "details" ? null : "details"); resetSyncPanelState(); }}>
                                  {activeTask === "details" ? "Ẩn thông tin kỹ thuật" : "Thông số kỹ thuật và phân quyền"}
                                </button>
                                {activeTask === "service" && <p className="integration-user-note">Để dùng WMS/WFS/COG/XYZ, mở <b>Nguồn dữ liệu ngoài (R010)</b> của dự án. Trình kết nối không tự đăng nhập hoặc đồng bộ với máy chủ ngoài.</p>}
                                {activeTask === "sync" && <p className="integration-user-note">Tải .gpkg bản gốc, chỉnh sửa một bản sao trong QGIS, sau đó chọn lớp đích và xem trước thay đổi. Chỉ đẩy dữ liệu nếu có quyền sửa lớp và dự án.</p>}
                                {activeTask === "export" && <p className="integration-user-note">Chọn lớp dữ liệu và bấm Tải xuống. Tệp sẽ lưu về máy; dữ liệu gốc trên OpenVietGIS không thay đổi.</p>}
                              </section>
                            </details>

                            {adapter.key === "qgis" && (
                              <QgisBindingManager client={client} org={org} projectId={projectId} />
                            )}

                            {exportAdapterKey && adapter.lifecycle === "existing_read_only" && (
                              <BatchExport
                                key={`${projectId}:${exportAdapterKey}`}
                                client={client}
                                org={org}
                                projectId={projectId}
                                adapterKey={exportAdapterKey}
                                layers={profile.layers}
                              />
                            )}
                            {activeTask === "details" && <div className="integration-profile-summary">
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
                            </div>}

                            {activeTask === "details" && (profile.connection_profile ? (
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
                                  {profile.connection_profile
                                    .sync_push_enabled && (
                                    <div>
                                      <dt>Đồng bộ Changeset có kiểm soát</dt>
                                      <dd>
                                        Hỗ trợ quy trình 2 bước (Diff → Xác nhận)
                                        {profile.connection_profile
                                          .deletions_blocked_by_default !== false
                                          ? " · Xóa bị chặn mặc định"
                                          : ""}
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
                                    <h5>API trao đổi hiện hữu</h5>
                                    <ul className="integration-endpoints">
                                      {profile.connection_profile.existing_endpoints.map(
                                        (endpoint) => (
                                          <li
                                            key={`${endpoint.method}-${endpoint.name}`}
                                          >
                                            <span
                                              className={
                                                endpoint.method === "POST"
                                                  ? "integration-method-badge is-post"
                                                  : "integration-method-badge"
                                              }
                                            >
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
                            ))}

                            {(activeTask === "export" || activeTask === "sync") && (
                            <div className="integration-profile-block integration-data-step">
                              <span className="integration-step-label">BƯỚC 3</span>
                              <h4>
                                Chọn lớp dữ liệu trong dự án (
                                {profile.layers.length})
                              </h4>
                              {profile.layers.length === 0 ? (
                                <p className="integration-profile-empty">
                                  Không có lớp dữ liệu hoạt động nào hiển thị trong
                                  phạm vi quyền hiện tại.
                                </p>
                              ) : (
                                <div className="integration-table-wrap">
                                  <table className="integration-table integration-layer-table">
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
                                        const dxfBusy =
                                          exportingKey ===
                                          `${adapter.key}:${layer.layer_id}:dxf`;
                                        const canOpenLayerSync =
                                          qgisSyncPushAllowed &&
                                          layer.crs_explicit;
                                        const isSyncSelected =
                                          syncLayerId === layer.layer_id;

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
                                                {layer.export_permitted ||
                                                canOpenLayerSync ? (
                                                  <div className="integration-export-actions">
                                                    {exportAdapterKey ===
                                                      "qgis" && (
                                                      <>
                                                        {(activeTask === "export" || activeTask === "sync") && layer.export_permitted && (
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
                                                                : activeTask === "sync" ? "Tải .gpkg bản gốc" : "Tải .gpkg"}
                                                            </button>
                                                            {activeTask === "export" && layer.stored_crs_code ===
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
                                                        {activeTask === "sync" && canOpenLayerSync && (
                                                          <button
                                                            type="button"
                                                            className={`integration-sync-toggle-btn${isSyncSelected ? " is-active" : ""}`}
                                                            aria-expanded={
                                                              isSyncSelected
                                                            }
                                                            onClick={() =>
                                                              toggleSyncLayerPanel(
                                                                layer.layer_id
                                                              )
                                                            }
                                                          >
                                                            {isSyncSelected
                                                              ? "Đóng đồng bộ .gpkg"
                                                              : "Đồng bộ .gpkg (2 bước)"}
                                                          </button>
                                                        )}
                                                      </>
                                                    )}
                                                    {exportAdapterKey ===
                                                      "postgis" &&
                                                      layer.export_permitted && (
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
                                                    {exportAdapterKey ===
                                                      "autocad" &&
                                                      layer.export_permitted &&
                                                      (isAutocadMetricCrs(
                                                        layer.stored_crs_code
                                                      ) ? (
                                                        <button
                                                          type="button"
                                                          className="integration-export-btn"
                                                          disabled={
                                                            exportingKey !== null
                                                          }
                                                          onClick={() =>
                                                            handleReadOnlyExport(
                                                              "autocad",
                                                              layer,
                                                              "dxf"
                                                            )
                                                          }
                                                        >
                                                          {dxfBusy
                                                            ? "Đang tải .dxf…"
                                                            : "Tải .dxf"}
                                                        </button>
                                                      ) : (
                                                        <span className="integration-export-restricted">
                                                          Cần hệ tọa độ phẳng mét
                                                          (EPSG:5899/3857)
                                                        </span>
                                                      ))}
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

                              {selectedSyncLayer && (
                                <section
                                  className="integration-sync-panel"
                                  aria-label={`Đồng bộ .gpkg (2 bước) cho lớp ${selectedSyncLayer.name}`}
                                >
                                  <div className="integration-sync-header">
                                    <h5>
                                      Đồng bộ .gpkg (2 bước) ·{" "}
                                      {selectedSyncLayer.name}
                                    </h5>
                                    <span className="integration-policy-pill is-scoped">
                                      Xóa bị chặn mặc định
                                    </span>
                                  </div>

                                  <p className="integration-sync-note">
                                    Phiên bản gốc đang ghim:{" "}
                                    <code
                                      title={selectedSyncLayer.content_revision}
                                    >
                                      {selectedSyncLayer.content_revision.slice(
                                        0,
                                        12
                                      )}
                                    </code>{" "}
                                    · lược đồ:{" "}
                                    <code
                                      title={selectedSyncLayer.schema_revision}
                                    >
                                      {selectedSyncLayer.schema_revision.slice(
                                        0,
                                        12
                                      )}
                                    </code>{" "}
                                    · CRS: {formatLayerCrs(selectedSyncLayer)}.
                                    Bước 2 chỉ mở sau khi bước 1 xác nhận diff
                                    hợp lệ.
                                  </p>

                                  <label className="integration-sync-file">
                                    Tệp GeoPackage (.gpkg) đã chỉnh sửa trong
                                    QGIS
                                    <input
                                      type="file"
                                      accept=".gpkg"
                                      disabled={
                                        syncPreviewLoading || syncPushLoading
                                      }
                                      onChange={(event) => {
                                        const picked =
                                          event.target.files?.[0] ?? null;
                                        setSyncFile(picked);
                                        setSyncPreview(null);
                                        setSyncPushResult(null);
                                        setSyncError("");
                                      }}
                                    />
                                  </label>

                                  <div className="integration-sync-actions">
                                    <button
                                      type="button"
                                      className="integration-export-btn"
                                      disabled={
                                        !syncFile ||
                                        syncPreviewLoading ||
                                        syncPushLoading
                                      }
                                      onClick={() =>
                                        handleSyncPreview(selectedSyncLayer)
                                      }
                                    >
                                      {syncPreviewLoading
                                        ? "Đang phân tích Diff…"
                                        : "1. Xem trước thay đổi (Diff)"}
                                    </button>
                                    <button
                                      type="button"
                                      className="integration-sync-commit-btn"
                                      disabled={
                                        !syncFile ||
                                        !syncPreview ||
                                        !syncPreview.can_commit ||
                                        syncPreviewLoading ||
                                        syncPushLoading
                                      }
                                      onClick={() =>
                                        handleSyncPush(selectedSyncLayer)
                                      }
                                    >
                                      {syncPushLoading
                                        ? "Đang đẩy Changeset…"
                                        : "2. Xác nhận đẩy Changeset"}
                                    </button>
                                  </div>

                                  {syncError && (
                                    <p
                                      className="integration-export-status is-error"
                                      role="alert"
                                    >
                                      {syncError}
                                    </p>
                                  )}

                                  {syncPreview && (
                                    <div
                                      className="integration-sync-preview"
                                      role="status"
                                    >
                                      <dl className="integration-sync-metrics">
                                        <div>
                                          <dt>Tạo mới</dt>
                                          <dd>{syncPreview.create_count}</dd>
                                        </div>
                                        <div>
                                          <dt>Cập nhật</dt>
                                          <dd>{syncPreview.update_count}</dd>
                                        </div>
                                        <div>
                                          <dt>Không đổi</dt>
                                          <dd>{syncPreview.unchanged_count}</dd>
                                        </div>
                                        <div>
                                          <dt>
                                            Đối tượng máy chủ giữ nguyên (chặn
                                            xóa mặc định)
                                          </dt>
                                          <dd>
                                            {
                                              syncPreview.omitted_server_feature_count
                                            }
                                          </dd>
                                        </div>
                                        <div>
                                          <dt>Trạng thái phiên bản</dt>
                                          <dd>
                                            {syncPreview.stale_revision ? (
                                              <span className="integration-flag is-denied">
                                                Cũ (stale_revision)
                                              </span>
                                            ) : (
                                              <span className="integration-flag is-allowed">
                                                Hợp lệ
                                              </span>
                                            )}
                                          </dd>
                                        </div>
                                        <div>
                                          <dt>Mã diff (SHA-256)</dt>
                                          <dd>
                                            <code
                                              title={syncPreview.diff_sha256}
                                            >
                                              {syncPreview.diff_sha256.slice(
                                                0,
                                                12
                                              )}
                                            </code>
                                          </dd>
                                        </div>
                                      </dl>
                                      <p className="integration-sync-hint">
                                        {syncPreview.can_commit
                                          ? "Diff hợp lệ. Xóa đối tượng bị chặn mặc định; chỉ các đối tượng tạo mới và cập nhật sẽ được áp dụng khi bạn nhấn Bước 2."
                                          : "Không thể đẩy Changeset (can_commit: false): phiên bản lớp đã cũ hoặc không có thay đổi hợp lệ để áp dụng."}
                                      </p>
                                    </div>
                                  )}

                                  {syncPushResult && (
                                    <div
                                      className="integration-export-status"
                                      role="status"
                                    >
                                      Đã đẩy Changeset có kiểm soát · Changeset:{" "}
                                      <code>{syncPushResult.changeset_id}</code>{" "}
                                      · Phiên bản nội dung mới:{" "}
                                      <code
                                        title={
                                          syncPushResult.resulting_content_revision
                                        }
                                      >
                                        {syncPushResult.resulting_content_revision.slice(
                                          0,
                                          12
                                        )}
                                      </code>{" "}
                                      · Số thao tác:{" "}
                                      <strong>
                                        {syncPushResult.operation_count}
                                      </strong>
                                      .
                                    </div>
                                  )}
                                </section>
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
                            )}

                            {activeTask === "details" && <div className="integration-profile-block">
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
                            </div>}
                          </div>
                        )}
                      </section>
                    )}
                  </article>
                );
              })}
            </div>

            {!visibleAdapters.length && (
              <p className="live-note">Không có kết nối khớp từ khóa.</p>
            )}
          </>
        )}
      </div>
    </section>
  );
}
