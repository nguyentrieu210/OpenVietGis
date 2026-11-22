import {
  createSavedQuery,
  executeLayerQuery,
  listSavedQueries,
  listSpatialLayers,
  loadCsrfToken,
  type LayerQueryResponse
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { ProjectToolbar } from "./ProjectToolbar";

export function QueryWorkbenchPage() {
  const { organizationId = "", projectId = "" } = useParams();
  const qc = useQueryClient();
  const [layerId, setLayerId] = useState("");
  const [field, setField] = useState("");
  const [op, setOp] = useState("eq");
  const [val, setVal] = useState("");
  const [refLayerId, setRefLayerId] = useState("");
  const [predicate, setPredicate] = useState("intersects");
  const [queryName, setQueryName] = useState("Bộ lọc đã lưu");
  const [result, setResult] = useState<LayerQueryResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const layersQuery = useQuery({
    queryKey: ["r007", organizationId, projectId, "layers"],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });
  const savedQuery = useQuery({
    queryKey: ["r007", organizationId, projectId, "saved"],
    queryFn: () => listSavedQueries(apiClient, organizationId, projectId),
    enabled: Boolean(organizationId && projectId)
  });

  const layers = layersQuery.data?.items ?? [];
  const activeLayerId = layerId || layers[0]?.id || "";

  function buildBody(): Record<string, unknown> {
    const body: Record<string, unknown> = { limit: 100, offset: 0 };
    if (field.trim() && val.trim()) {
      body.attribute_filter = { field: field.trim(), op, value: val.trim() };
    }
    if (refLayerId) {
      body.spatial_filter = { predicate, reference_layer_id: refLayerId };
    }
    return body;
  }

  async function runQuery(): Promise<void> {
    if (!activeLayerId) return;
    setError(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const res = await executeLayerQuery(
        apiClient,
        organizationId,
        projectId,
        activeLayerId,
        buildBody(),
        csrf
      );
      setResult(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Thực thi truy vấn không thành công.");
    }
  }

  async function saveFilter(): Promise<void> {
    if (!activeLayerId) return;
    setError(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      await createSavedQuery(
        apiClient,
        organizationId,
        projectId,
        { layer_id: activeLayerId, name: queryName, ...buildBody() },
        csrf
      );
      await qc.invalidateQueries({ queryKey: ["r007", organizationId, projectId, "saved"] });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Không thể lưu truy vấn.");
    }
  }

  return (
    <div className="ovg-suite" data-testid="query-workbench">
      <ProjectToolbar
        organizationId={organizationId}
        projectId={projectId}
        active="queries"
        title="R007 Truy vấn Thuộc tính & Không gian"
      />
      <main className="ovg-suite-body">
        {error && <div className="ovg-error">{error}</div>}
        <section className="ovg-card">
          <h2>Bộ lọc Thuộc tính có kiểu & Truy vấn Không gian</h2>
          <div className="ovg-form-row">
            <label>
              Lớp dữ liệu mục tiêu
              <select value={activeLayerId} onChange={(e) => setLayerId(e.target.value)}>
                {layers.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Trường thuộc tính
              <input
                placeholder="optional field_id"
                value={field}
                onChange={(e) => setField(e.target.value)}
              />
            </label>
            <label>
              Toán tử
              <select value={op} onChange={(e) => setOp(e.target.value)}>
                <option value="eq">=</option>
                <option value="neq">!=</option>
                <option value="gt">&gt;</option>
                <option value="gte">&gt;=</option>
                <option value="lt">&lt;</option>
                <option value="lte">&lt;=</option>
                <option value="contains">contains</option>
              </select>
            </label>
            <label>
              Giá trị
              <input value={val} onChange={(e) => setVal(e.target.value)} />
            </label>
          </div>
          <div className="ovg-form-row">
            <label>
              Quan hệ không gian
              <select value={predicate} onChange={(e) => setPredicate(e.target.value)}>
                <option value="intersects">Giao nhau (Intersects)</option>
                <option value="within">Nằm trong (Within)</option>
                <option value="contains">Chứa (Contains)</option>
              </select>
            </label>
            <label>
              Lớp tham chiếu (tùy chọn)
              <select value={refLayerId} onChange={(e) => setRefLayerId(e.target.value)}>
                <option value="">Không chọn</option>
                {layers.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Tên truy vấn lưu
              <input value={queryName} onChange={(e) => setQueryName(e.target.value)} />
            </label>
            <button type="button" className="ovg-btn" disabled={!activeLayerId} onClick={() => void runQuery()}>
              Chạy truy vấn
            </button>
            <button
              type="button"
              className="ovg-btn ovg-btn-secondary"
              onClick={() => void saveFilter()}
            >
              Lưu truy vấn
            </button>
          </div>
        </section>

        {result && (
          <section className="ovg-card">
            <h3>
              Kết quả: {result.total_count} đối tượng phù hợp (rev{" "}
              <code>{result.content_revision.slice(0, 12)}</code>)
            </h3>
            <table className="ovg-table">
              <thead>
                <tr>
                  <th>Mã đối tượng</th>
                  <th>Thuộc tính</th>
                </tr>
              </thead>
              <tbody>
                {result.items.map((item) => (
                  <tr key={item.feature_id}>
                    <td>
                      <code>{item.feature_id}</code>
                    </td>
                    <td>
                      <code>{JSON.stringify(item.attributes)}</code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        <section className="ovg-card">
          <h3>Truy vấn đã lưu ({savedQuery.data?.items.length ?? 0})</h3>
          <ul>
            {(savedQuery.data?.items ?? []).map((q) => (
              <li key={q.id}>
                <strong>{q.name}</strong> — Lớp <code>{q.layer_id}</code>
              </li>
            ))}
          </ul>
        </section>
      </main>
    </div>
  );
}
