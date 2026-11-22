import {
  createPrintLayout,
  exportPrintLayout,
  listPrintExports,
  listPrintLayouts,
  listSpatialLayers,
  loadCsrfToken,
  printExportDownloadUrl,
  upsertLayerStyle
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { ProjectToolbar } from "./ProjectToolbar";

export function CartographyWorkbenchPage() {
  const { organizationId = "", projectId = "" } = useParams();
  const qc = useQueryClient();
  const [layerId, setLayerId] = useState("");
  const [fillColor, setFillColor] = useState("#2563eb");
  const [strokeColor, setStrokeColor] = useState("#1e3a8a");
  const [layoutTitle, setLayoutTitle] = useState("Bản đồ in OpenVietGIS");
  const [paperSize, setPaperSize] = useState<"A4" | "A3">("A4");
  const [orientation, setOrientation] = useState<"portrait" | "landscape">("landscape");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const layersQuery = useQuery({
    queryKey: ["r009", organizationId, projectId, "layers"],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const layoutsQuery = useQuery({
    queryKey: ["r009", organizationId, projectId, "layouts"],
    queryFn: () => listPrintLayouts(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const exportsQuery = useQuery({
    queryKey: ["r009", organizationId, projectId, "exports"],
    queryFn: () => listPrintExports(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });

  const layers = layersQuery.data?.items ?? [];
  const activeLayerId = layerId || layers[0]?.id || "";

  async function saveStyle(): Promise<void> {
    if (!activeLayerId) return;
    setError(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const st = await upsertLayerStyle(
        apiClient,
        organizationId,
        projectId,
        activeLayerId,
        {
          mode: "single",
          style_spec: { fill_color: fillColor, stroke_color: strokeColor, opacity: 0.75 }
        },
        csrf
      );
      setNotice(`Đã lưu ký hiệu trung lập rev ${st.revision} with auto legend.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Không thể lưu kiểu hiển thị.");
    }
  }

  async function handleCreateLayout(): Promise<void> {
    if (!activeLayerId) return;
    setError(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const lo = await createPrintLayout(
        apiClient,
        organizationId,
        projectId,
        {
          name: layoutTitle,
          title: layoutTitle,
          paper_size: paperSize,
          orientation,
          crs_code: 4326,
          attribution: "Dữ liệu dự án OpenVietGIS",
          layer_ids: [activeLayerId]
        },
        csrf
      );
      setNotice(`Đã tạo bố cục in ${lo.paper_size} ${lo.orientation}.`);
      await qc.invalidateQueries({ queryKey: ["r009", organizationId, projectId, "layouts"] });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Không thể tạo bố cục in.");
    }
  }

  async function handleExport(layoutId: string, format: "png" | "pdf", dpi: 150 | 300): Promise<void> {
    setError(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const ex = await exportPrintLayout(
        apiClient,
        organizationId,
        projectId,
        layoutId,
        { output_format: format, dpi },
        csrf
      );
      setNotice(`Đã xuất tệp ${ex.output_format.toUpperCase()} (${ex.dpi} DPI, 1:${ex.scale_denominator}).`);
      await qc.invalidateQueries({ queryKey: ["r009", organizationId, projectId, "exports"] });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Xuất bản đồ không thành công.");
    }
  }

  return (
    <div className="ovg-suite" data-testid="cartography-workbench">
      <ProjectToolbar
        organizationId={organizationId}
        projectId={projectId}
        active="cartography"
        title="R009 Bản đồ học & Xuất in ấn"
      />
      <main className="ovg-suite-body">
        {notice && <div className="ovg-notice">{notice}</div>}
        {error && <div className="ovg-error">{error}</div>}

        <div className="ovg-grid-2">
          <section className="ovg-card">
            <h2>Ký hiệu lớp & Chú giải tự động</h2>
            <div className="ovg-form-row">
              <label>
                Layer
                <select value={activeLayerId} onChange={(e) => setLayerId(e.target.value)}>
                  {layers.map((l) => (
                    <option key={l.id} value={l.id}>
                      {l.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Màu tô
                <input type="color" value={fillColor} onChange={(e) => setFillColor(e.target.value)} />
              </label>
              <label>
                Màu viền
                <input
                  type="color"
                  value={strokeColor}
                  onChange={(e) => setStrokeColor(e.target.value)}
                />
              </label>
              <button type="button" className="ovg-btn" disabled={!activeLayerId} onClick={() => void saveStyle()}>
                Lưu ký hiệu
              </button>
            </div>
          </section>

          <section className="ovg-card">
            <h2>Bố cục in ấn (A4 / A3)</h2>
            <div className="ovg-form-row">
              <label>
                Tiêu đề bản đồ
                <input value={layoutTitle} onChange={(e) => setLayoutTitle(e.target.value)} />
              </label>
              <label>
                Khổ giấy
                <select value={paperSize} onChange={(e) => setPaperSize(e.target.value as "A4" | "A3")}>
                  <option value="A4">A4</option>
                  <option value="A3">A3</option>
                </select>
              </label>
              <label>
                Hướng giấy
                <select
                  value={orientation}
                  onChange={(e) => setOrientation(e.target.value as "portrait" | "landscape")}
                >
                  <option value="landscape">Ngang (Landscape)</option>
                  <option value="portrait">Dọc (Portrait)</option>
                </select>
              </label>
              <button type="button" className="ovg-btn" disabled={!activeLayerId} onClick={() => void handleCreateLayout()}>
                Tạo bố cục in
              </button>
            </div>
          </section>
        </div>

        <section className="ovg-card">
          <h2>Danh sách bố cục & Xuất bản</h2>
          <table className="ovg-table">
            <thead>
              <tr>
                <th>Tiêu đề</th>
                <th>Khổ giấy</th>
                <th>CRS</th>
                <th>Xuất bản</th>
              </tr>
            </thead>
            <tbody>
              {(layoutsQuery.data?.items ?? []).map((lo) => (
                <tr key={lo.id}>
                  <td>{lo.title}</td>
                  <td>
                    {lo.paper_size} {lo.orientation}
                  </td>
                  <td>EPSG:{lo.crs_code}</td>
                  <td style={{ display: "flex", gap: "0.4rem" }}>
                    <button
                      type="button"
                      className="ovg-btn ovg-btn-secondary"
                      onClick={() => void handleExport(lo.id, "png", 150)}
                    >
                      PNG 150 DPI
                    </button>
                    <button
                      type="button"
                      className="ovg-btn"
                      onClick={() => void handleExport(lo.id, "pdf", 300)}
                    >
                      PDF 300 DPI
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          <h3 style={{ marginTop: "1rem" }}>Tệp bản đồ đã xuất</h3>
          <ul>
            {(exportsQuery.data?.items ?? []).map((ex) => (
              <li key={ex.id}>
                <a
                  href={printExportDownloadUrl(organizationId, projectId, ex.id)}
                  target="_blank"
                  rel="noreferrer"
                >
                  Download {ex.output_format.toUpperCase()} ({ex.dpi} DPI, 1:{ex.scale_denominator})
                </a>{" "}
                — SHA-256 <code>{ex.sha256.slice(0, 12)}</code>
              </li>
            ))}
          </ul>
        </section>
      </main>
    </div>
  );
}
