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
import { useRef, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { ProjectToolbar } from "./ProjectToolbar";

function processingStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    queued: "Đang chờ",
    active: "Đang xử lý",
    running: "Đang xử lý",
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
  const [search] = useSearchParams();
  const requested = search.get("algorithm");
  const [algo, setAlgo] = useState<"buffer" | "clip" | "dissolve">(requested === "clip" || requested === "dissolve" ? requested : "buffer");
  const [inputLayerId, setInputLayerId] = useState("");
  const [overlayLayerId, setOverlayLayerId] = useState("");
  const [distance, setDistance] = useState("500");
  const [segments,setSegments]=useState("8");
  const [filter,setFilter]=useState("");
  const [busy,setBusy]=useState(false);
  const inFlight=useRef(false);
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
    enabled: Boolean(organizationId && projectId),
    refetchInterval: q => q.state.data?.items.some(j=>j.status==="queued"||j.status==="running") ? 2000 : false
  });

  const layers = layersQuery.data?.items ?? [];
  const activeInput = layers.some(l=>l.id===inputLayerId)?inputLayerId:layers[0]?.id??"";
  const descriptor=algosQuery.data?.items.find(a=>a.id===algo);
  const validation=!descriptor?"Đang chờ danh mục thuật toán từ máy chủ.":!activeInput?"Chọn lớp đầu vào.":!outputName.trim()?"Nhập tên lớp kết quả.":algo==="clip"&&(!overlayLayerId||overlayLayerId===activeInput||!layers.some(l=>l.id===overlayLayerId))?"Chọn lớp phủ cắt khác lớp đầu vào.":algo==="buffer"&&(!Number.isFinite(Number(distance))||Number(distance)<0.1||Number(distance)>500000||!Number.isInteger(Number(segments))||Number(segments)<2||Number(segments)>16)?"Khoảng cách 0,1–500.000 m; số đoạn 2–16.":"";
  const jobs=(jobsQuery.data?.items??[]).filter(j=>!filter||j.status===filter);
  const mutate=async(fn:()=>Promise<unknown>)=>{if(inFlight.current)return;inFlight.current=true;setBusy(true);setError(null);try{await fn();await qc.invalidateQueries({queryKey:["r008",organizationId,projectId]})}catch(e){setError(e instanceof Error?e.message:"Thao tác thất bại.")}finally{inFlight.current=false;setBusy(false)}};

  async function runProcessing(): Promise<void> {
    if (validation || inFlight.current) return;
    inFlight.current=true;setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const parameters =
        algo === "buffer"
          ? { distance_meters: Number(distance), quad_segs: Number(segments) }
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
          output_name: outputName.trim(),
          operation_id: crypto.randomUUID()
        },
        csrf
      );
      setNotice(`${processingStatusLabel(job.status)} · ${job.output_name}${job.feature_count==null?"":" · "+job.feature_count+" đối tượng"}`);
      await qc.invalidateQueries({ queryKey: ["r008", organizationId, projectId] });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Xử lý không thành công.");
    } finally { inFlight.current=false;setBusy(false); }
  }

  return (
    <div className="ovg-suite" data-testid="processing-workbench">
      <ProjectToolbar
        organizationId={organizationId}
        projectId={projectId}
        active="processing"
        title="Xử lý Không gian & Tác vụ bền vững"
      />
      <main className="ovg-suite-body">
        {notice && <div className="ovg-notice" role="status">{notice}</div>}
        {error && <div className="ovg-error" role="alert">{error}</div>}
        <section className="ovg-card">
          <h2>Tham số phân tích không gian ({algosQuery.data?.items.length ?? 0} thuật toán)</h2>
          <div className="ovg-algorithm-picker" aria-label="Chọn thuật toán">{(algosQuery.data?.items??[]).map(a=><button type="button" key={a.id} className="ovg-algorithm-choice" aria-pressed={algo===a.id} disabled={busy} onClick={()=>setAlgo(a.id)}><b>{algorithmLabel(a.id)}</b><small>{a.name}</small></button>)}</div>
          {algosQuery.isError&&<p role="alert">Không tải được danh mục. <button type="button" onClick={()=>void algosQuery.refetch()}>Thử lại</button></p>}
          {descriptor&&<p className="ovg-processing-description">{algo==="buffer"?"Tạo vùng đệm theo khoảng cách. Đầu vào địa lý được xử lý trong Web Mercator; không phải phép đo trắc địa chính xác.":algo==="clip"?"Cắt đối tượng theo vùng phủ polygon.":"Gộp hình học toàn lớp hoặc theo trường thuộc tính."}</p>}
          <div className="ovg-form-row">
            <label>
              Thuật toán
              <select
                value={algo}
                onChange={(e) => setAlgo(e.target.value as "buffer" | "clip" | "dissolve")}
              >
                {(algosQuery.data?.items??[]).map(a=><option key={a.id} value={a.id}>{algorithmLabel(a.id)} ({a.name})</option>)}
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
              <><label>Khoảng cách (mét)<input type="number" min="0.1" max="500000" step="any" value={distance} onChange={e=>setDistance(e.target.value)}/></label><label>Số đoạn cung tròn<input type="number" min="2" max="16" step="1" value={segments} onChange={e=>setSegments(e.target.value)}/></label></>
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
            <button type="button" className="ovg-btn" disabled={busy||Boolean(validation)} onClick={() => void runProcessing()}>
              {busy?"Đang thực hiện…":"Chạy tác vụ"}
            </button>
          </div>
          {validation&&<p className="ovg-processing-description" role="status">{validation}</p>}
        </section>
        <section className="ovg-card">
          <div className="ovg-processing-history-head"><h2>Lịch sử tác vụ ({jobs.length})</h2><label>Trạng thái <select aria-label="Lọc trạng thái tác vụ" value={filter} onChange={e=>setFilter(e.target.value)}><option value="">Tất cả</option>{["queued","running","succeeded","failed","cancelled"].map(v=><option value={v} key={v}>{processingStatusLabel(v)}</option>)}</select></label><button type="button" onClick={()=>void jobsQuery.refetch()}>Làm mới</button></div>
          {jobsQuery.isError&&<p role="alert">Không tải được lịch sử tác vụ.</p>}
          <div className="ovg-processing-table-scroll">
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
              {jobs.map((j) => (
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
                        disabled={busy} onClick={()=>void mutate(async()=>cancelProcessingJob(apiClient,organizationId,projectId,j.id,await loadCsrfToken(apiClient)))}
                      >
                        Hủy
                      </button>
                    )}
                    {(j.status === "failed" || j.status === "cancelled") && (
                      <button
                        type="button"
                        className="ovg-btn ovg-btn-secondary"
                        disabled={busy} onClick={()=>void mutate(async()=>retryProcessingJob(apiClient,organizationId,projectId,j.id,await loadCsrfToken(apiClient)))}
                      >
                        Thử lại
                      </button>
                    )}
                  </td>
                </tr>
              ))}
              {!jobs.length&&<tr><td colSpan={6}>{jobsQuery.isPending?"Đang tải lịch sử…":"Không có tác vụ phù hợp."}</td></tr>}
            </tbody>
          </table></div>
        </section>
      </main>
    </div>
  );
}
