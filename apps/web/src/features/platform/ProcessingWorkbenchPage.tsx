import {
  cancelProcessingJob,
  createProcessingJob,
  listProcessingAlgorithms,
  listProcessingJobs,
  listSpatialLayers,
  loadCsrfToken,
  retryProcessingJob
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { ProjectToolbar } from "./ProjectToolbar";

function processingStatusLabel(status: string): string {
  const labels: Record<string, string> = {
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

function algorithmLabel(id: string): string {
  const labels: Record<string, string> = {
    buffer: "Vùng đệm",
    clip: "Cắt lớp",
    dissolve: "Gộp vùng"
  };
  return labels[id] ?? id;
}

export function ProcessingWorkbenchPage() {
  const { organizationId = "", projectId = "" } = useParams();
  const qc = useQueryClient();
  const [algo, setAlgo] = useState<"buffer" | "clip" | "dissolve">("buffer");
  const [inputLayerId, setInputLayerId] = useState("");
  const [overlayLayerId, setOverlayLayerId] = useState("");
  const [distance, setDistance] = useState("500");
  const [groupField, setGroupField] = useState("");
  const [outputName, setOutputName] = useState("Lớp kết quả xử lý");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const layersQuery = useQuery({
    queryKey: ["r008", organizationId, projectId, "layers"],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const algosQuery = useQuery({
    queryKey: ["r008", organizationId, projectId, "algos"],
    queryFn: () => listProcessingAlgorithms(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const jobsQuery = useQuery({
    queryKey: ["r008", organizationId, projectId, "jobs"],
    queryFn: () => listProcessingJobs(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });

  const layers = layersQuery.data?.items ?? [];
  const activeInput = inputLayerId || layers[0]?.id || "";

  async function runProcessing(): Promise<void> {
    if (!activeInput) return;
    setError(null);
    setNotice(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const parameters =
        algo === "buffer"
          ? { distance_meters: Number(distance), quad_segs: 8 }
          : algo === "dissolve"
            ? { group_by_field: groupField.trim() || null }
            : {};
      const job = await createProcessingJob(
        apiClient,
        organizationId,
        projectId,
        {
          algorithm_id: algo,
          input_layer_id: activeInput,
          overlay_layer_id: algo === "clip" ? overlayLayerId || null : null,
          parameters,
          output_name: outputName,
          operation_id: `ui-proc-${Date.now()}`
        },
        csrf
      );
      setNotice(`Tác vụ ${job.id} hoàn tất (${processingStatusLabel(job.status)}, ${job.feature_count ?? 0} đối tượng).`);
      await qc.invalidateQueries({ queryKey: ["r008", organizationId, projectId] });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Xử lý không thành công.");
    }
  }

  return (
    <div className="ovg-suite" data-testid="processing-workbench">
      <ProjectToolbar
        organizationId={organizationId}
        projectId={projectId}
        active="processing"
        title="R008 Xử lý Không gian & Tác vụ bền vững"
      />
      <main className="ovg-suite-body">
        {notice && <div className="ovg-notice">{notice}</div>}
        {error && <div className="ovg-error">{error}</div>}
        <section className="ovg-card">
          <h2>Chạy thuật toán phân tích không gian ({algosQuery.data?.items.length ?? 3})</h2>
          <div className="ovg-form-row">
            <label>
              Thuật toán
              <select
                value={algo}
                onChange={(e) => setAlgo(e.target.value as "buffer" | "clip" | "dissolve")}
              >
                <option value="buffer">Vùng đệm (Buffer)</option>
                <option value="clip">Cắt lớp (Clip)</option>
                <option value="dissolve">Gộp vùng (Dissolve)</option>
              </select>
            </label>
            <label>
              Lớp đầu vào
              <select value={activeInput} onChange={(e) => setInputLayerId(e.target.value)}>
                {layers.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </select>
            </label>
            {algo === "clip" && (
              <label>
                Lớp phủ cắt (Overlay)
                <select value={overlayLayerId} onChange={(e) => setOverlayLayerId(e.target.value)}>
                  <option value="">Chọn lớp chồng phủ</option>
                  {layers.map((l) => (
                    <option key={l.id} value={l.id}>
                      {l.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {algo === "buffer" && (
              <label>
                Khoảng cách (mét)
                <input value={distance} onChange={(e) => setDistance(e.target.value)} />
              </label>
            )}
            {algo === "dissolve" && (
              <label>
                Trường gom nhóm (tùy chọn)
                <input value={groupField} onChange={(e) => setGroupField(e.target.value)} />
              </label>
            )}
            <label>
              Tên lớp kết quả mới
              <input value={outputName} onChange={(e) => setOutputName(e.target.value)} />
            </label>
            <button type="button" className="ovg-btn" disabled={!activeInput} onClick={() => void runProcessing()}>
              Chạy tác vụ
            </button>
          </div>
        </section>

        <section className="ovg-card">
          <h2>Lịch sử tác vụ xử lý</h2>
          <table className="ovg-table">
            <thead>
              <tr>
                <th>Thuật toán</th>
                <th>Lớp đầu ra</th>
                <th>Trạng thái</th>
                <th>Số đối tượng</th>
                <th>Bộ xử lý</th>
                <th>Hành động</th>
              </tr>
            </thead>
            <tbody>
              {(jobsQuery.data?.items ?? []).map((j) => (
                <tr key={j.id}>
                  <td>{algorithmLabel(j.algorithm_id)}</td>
                  <td>{j.output_name}</td>
                  <td>
                    <span className="ovg-badge">{processingStatusLabel(j.status)}</span>
                  </td>
                  <td>{j.feature_count ?? "—"}</td>
                  <td>{j.engine_version}</td>
                  <td>
                    {j.status === "queued" && (
                      <button
                        type="button"
                        className="ovg-btn ovg-btn-secondary"
                        onClick={async () => {
                          const csrf = await loadCsrfToken(apiClient);
                          await cancelProcessingJob(apiClient, organizationId, projectId, j.id, csrf);
                          await qc.invalidateQueries({ queryKey: ["r008", organizationId, projectId, "jobs"] });
                        }}
                      >
                        Cancel
                      </button>
                    )}
                    {(j.status === "failed" || j.status === "cancelled") && (
                      <button
                        type="button"
                        className="ovg-btn ovg-btn-secondary"
                        onClick={async () => {
                          const csrf = await loadCsrfToken(apiClient);
                          await retryProcessingJob(apiClient, organizationId, projectId, j.id, csrf);
                          await qc.invalidateQueries({ queryKey: ["r008", organizationId, projectId, "jobs"] });
                        }}
                      >
                        Retry
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      </main>
    </div>
  );
}
