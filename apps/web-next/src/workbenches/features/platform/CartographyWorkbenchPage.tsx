import {
  createPrintLayout,
  exportPrintLayout,
  listPrintExports,
  listPrintLayouts,
  listSpatialLayers,
  loadCsrfToken,
  printExportDownloadUrl,
  previewPrintExport,
  type PrintExportPreview,
  upsertLayerStyle
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
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
  const [pending, setPending] = useState<{ org: string; project: string; layoutId: string; format: "png" | "pdf"; dpi: 150 | 300; preview: PrintExportPreview } | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  const activePending = pending?.org === organizationId && pending.project === projectId ? pending : null;

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
      setNotice(`Đã lưu ký hiệu trung lập phiên bản ${st.revision} kèm chú giải tự động.`);
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
      setNotice(`Đã tạo bố cục in ${lo.paper_size} ${lo.orientation === "landscape" ? "ngang" : "dọc"}.`);
      await qc.invalidateQueries({ queryKey: ["r009", organizationId, projectId, "layouts"] });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Không thể tạo bố cục in.");
    }
  }

  async function handleExportPreview(layoutId: string, format: "png" | "pdf", dpi: 150 | 300): Promise<void> {
    const request = ++generation.current;
    setError(null);
    setNotice(null);
    setPending(null);
    setBusy(true);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const preview = await previewPrintExport(apiClient, organizationId, projectId, layoutId, { output_format: format, dpi }, csrf);
      if (request === generation.current) setPending({ org: organizationId, project: projectId, layoutId, format, dpi, preview });
    } catch (err) {
      if (request === generation.current) setError(err instanceof Error ? err.message : "Không thể chuẩn bị bản in.");
    } finally {
      if (request === generation.current) setBusy(false);
    }
  }

  async function confirmExport(): Promise<void> {
    if (!activePending || busy) return;
    const selection = activePending;
    const request = ++generation.current;
    setBusy(true);
    setError(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const ex = await exportPrintLayout(
        apiClient, organizationId, projectId, selection.layoutId,
        { output_format: selection.format, dpi: selection.dpi, expected_revision_fingerprint: selection.preview.revision_fingerprint },
        csrf
      );
      if (request === generation.current) {
        setPending(null);
        setNotice(`Đã xuất tệp ${ex.output_format.toUpperCase()} (${ex.dpi} DPI, 1:${ex.scale_denominator}).`);
      }
      await qc.invalidateQueries({ queryKey: ["r009", organizationId, projectId, "exports"] });
    } catch (err) {
      if (request === generation.current) {
        setPending(null);
        setError(err instanceof Error ? `${err.message} — Hãy chuẩn bị bản in lại.` : "Xuất bản đồ không thành công.");
      }
    } finally {
      if (request === generation.current) setBusy(false);
    }
  }

  return (
    <div className="ovg-suite" data-testid="cartography-workbench">
      <ProjectToolbar
        organizationId={organizationId}
        projectId={projectId}
        active="cartography"
        title="Bản đồ học & Xuất in ấn"
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
                      disabled={busy}
                      onClick={() => void handleExportPreview(lo.id, "png", 150)}
                    >
                      PNG 150 DPI
                    </button>
                    <button
                      type="button"
                      className="ovg-btn"
                      disabled={busy}
                      onClick={() => void handleExportPreview(lo.id, "pdf", 300)}
                    >
                      PDF 300 DPI
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {activePending && (
            <section className="ovg-card" data-testid="print-revision-preview">
              <h3>Xác nhận bản in đã ghim phiên bản</h3>
              <p>{activePending.format.toUpperCase()} · {activePending.dpi} DPI · EPSG:{activePending.preview.revision_manifest.crs_code} · {activePending.preview.revision_manifest.pixel_width} × {activePending.preview.revision_manifest.pixel_height} px</p>
              <p>Tỷ lệ 1:{activePending.preview.revision_manifest.scale_denominator} (xấp xỉ) · {activePending.preview.revision_manifest.layers.length} lớp</p>
              <p>Phạm vi in thực tế: {Object.entries(activePending.preview.revision_manifest.effective_extent).map(([axis, value]) => `${axis}: ${value.toFixed(5)}`).join(" · ")}</p>
              <p>Phiên bản <code>{activePending.preview.revision_fingerprint.slice(0, 12)}</code>. Nếu dữ liệu hoặc ký hiệu thay đổi, yêu cầu sẽ bị từ chối.</p>
              <button type="button" className="ovg-btn" disabled={busy} onClick={() => void confirmExport()}>Xác nhận xuất {activePending.format.toUpperCase()}</button>
              <button type="button" className="ovg-btn ovg-btn-secondary" disabled={busy} onClick={() => setPending(null)}>Hủy chuẩn bị</button>
            </section>
          )}
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
