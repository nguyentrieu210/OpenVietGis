import {
  ApiError, getCurrentUser, getEffectivePermissions,
  cancelProcessingJob,
  createProcessingJob,
  listProcessingAlgorithms,
  listProcessingJobs,
  listSpatialLayers,
  loadCsrfToken,
  retryProcessingJob
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect,useRef, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { ProjectToolbar } from "./ProjectToolbar";
import {processingLayer,processingRequest,readPendingProcessing,supportedAlgorithm,verifyProcessingReceipt,type ProcessingRequest} from "./processing-request";

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
  const [pending,setPending]=useState<ProcessingRequest|null>(null);
  const [loadedKey,setLoadedKey]=useState("");
  const userQuery=useQuery({queryKey:["r008",organizationId,projectId,"identity"],queryFn:()=>getCurrentUser(apiClient),enabled:!!organizationId&&!!projectId});
  const accessQuery=useQuery({queryKey:["r008",organizationId,projectId,"permissions"],queryFn:async()=>{const [p,a]=await Promise.all([getEffectivePermissions(apiClient,organizationId,{project_id:projectId}),apiClient.request<{restricted:boolean}>("/organizations/"+encodeURIComponent(organizationId)+"/projects/"+encodeURIComponent(projectId)+"/access/mine")]);return p.permissions.includes("project.edit")&&a.restricted===false},enabled:!!userQuery.data?.id});
  const pendingKey=userQuery.data?.status==="active"?"ovg:processing:pending:v1:"+[userQuery.data.id,organizationId,projectId].map(encodeURIComponent).join(":"):"";
  useEffect(()=>{let value:ProcessingRequest|null=null;if(pendingKey)try{value=readPendingProcessing(sessionStorage.getItem(pendingKey))}catch{}setPending(value);setLoadedKey(pendingKey)},[pendingKey]);
  const remember=(value:ProcessingRequest|null)=>{if(!pendingKey)throw Error("Chưa xác minh tài khoản.");if(value)sessionStorage.setItem(pendingKey,JSON.stringify(value));else sessionStorage.removeItem(pendingKey);setPending(value)};
  const canWrite=accessQuery.data===true&&!!pendingKey&&loadedKey===pendingKey;
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

  const layers = (layersQuery.data?.items ?? []).filter(processingLayer);
  const activeInput=inputLayerId;
  const input=layers.find(l=>l.id===activeInput);
  const overlay=layers.find(l=>l.id===overlayLayerId);
  const algorithms=(algosQuery.data?.items??[]).filter(a=>supportedAlgorithm(a.id));
  const descriptor=algorithms.find(a=>a.id===algo);
  const validation=(()=>{if(!canWrite)return "Chưa xác minh quyền tạo tác vụ.";if(!descriptor)return "Thuật toán chưa được máy chủ cung cấp.";try{processingRequest(algo,input,overlay,distance,segments,groupField,outputName,"preview");return ""}catch(e){return e instanceof Error?e.message:"Tham số không hợp lệ."}})();
  const jobs=(jobsQuery.data?.items??[]).filter(j=>!filter||j.status===filter);
  const mutate=async(fn:()=>Promise<unknown>)=>{if(inFlight.current)return;inFlight.current=true;setBusy(true);setError(null);try{await fn();await qc.invalidateQueries({queryKey:["r008",organizationId,projectId]})}catch(e){setError(e instanceof Error?e.message:"Thao tác thất bại.")}finally{inFlight.current=false;setBusy(false)}};

  async function runProcessing(): Promise<void> {
    if(!canWrite||(!pending&&validation)||inFlight.current)return;
    inFlight.current=true;setBusy(true);setError(null);setNotice(null);
    let request:ProcessingRequest|undefined;
    try{
      request=pending??processingRequest(algo,input,overlay,distance,segments,groupField,outputName,crypto.randomUUID());
      remember(request); // fail closed if recovery storage is unavailable
      const csrf=await loadCsrfToken(apiClient);
      const job=verifyProcessingReceipt(await createProcessingJob(apiClient,organizationId,projectId,request,csrf),request);
      remember(null);
      setNotice(processingStatusLabel(job.status)+" · "+job.output_name+" · ID "+job.id+(job.feature_count==null?"":" · "+job.feature_count+" đối tượng"));
      await qc.invalidateQueries({queryKey:["r008",organizationId,projectId]});
    }catch(err){
      // Once a request is ambiguous, even a later 4xx cannot prove it never committed.
      if(!pending&&err instanceof ApiError&&[400,403,404,422].includes(err.status))try{remember(null)}catch{}
      setError(err instanceof Error?err.message:"Xử lý không thành công.");
    }finally{inFlight.current=false;setBusy(false)}
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
          <h2>Tham số phân tích không gian ({algorithms.length} thuật toán)</h2>
          <div className="ovg-algorithm-picker" aria-label="Chọn thuật toán">{algorithms.map(a=><button type="button" key={a.id} className="ovg-algorithm-choice" aria-pressed={algo===a.id} disabled={busy||!!pending} onClick={()=>{setAlgo(a.id);setOverlayLayerId("");setGroupField("")}}><b>{algorithmLabel(a.id)}</b><small>{a.name}</small></button>)}</div>
          {algosQuery.isError&&<p role="alert">Không tải được danh mục. <button type="button" onClick={()=>void algosQuery.refetch()}>Thử lại</button></p>}
          {descriptor&&<p className="ovg-processing-description">{algo==="buffer"?"Tạo vùng đệm theo khoảng cách. Đầu vào địa lý được xử lý trong Web Mercator; không phải phép đo trắc địa chính xác.":algo==="clip"?"Cắt đối tượng theo vùng phủ polygon.":"Gộp hình học toàn lớp hoặc theo trường thuộc tính."}</p>}
          {pending&&<p role="status">Chưa xác minh phản hồi: {pending.algorithm_id} · {pending.output_name} · mã {pending.operation_id}. Gửi lại cùng yêu cầu, không tạo tác vụ mới.</p>}
          <fieldset disabled={busy||!!pending} className="ovg-form-row">
            <label>
              Thuật toán
              <select
                value={algo}
                onChange={(e) => {if(supportedAlgorithm(e.target.value)){setAlgo(e.target.value);setOverlayLayerId("");setGroupField("")}}}
              >
                {algorithms.map(a=><option key={a.id} value={a.id}>{algorithmLabel(a.id)} ({a.name})</option>)}
              </select>
            </label>
            <label>
              Lớp đầu vào
              <select aria-label="Lớp đầu vào" value={activeInput} onChange={(e) => {setInputLayerId(e.target.value);setOverlayLayerId("");setGroupField("")}}>
                <option value="">Chọn lớp đầu vào</option>
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
                <select aria-label="Lớp phủ cắt" value={overlayLayerId} onChange={(e) => setOverlayLayerId(e.target.value)}>
                  <option value="">Chọn lớp chồng phủ</option>
                  {layers.filter(l=>l.source.schema.geometry_family==="Polygon"&&l.id!==activeInput&&l.source_id!==input?.source_id).map((l) => (
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
                <select aria-label="Trường gom nhóm" value={groupField} onChange={e=>setGroupField(e.target.value)}><option value="">Không gom theo trường</option>{input?.source.schema.fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select>
              </label>
            )}
            <label>
              Tên lớp kết quả mới
              <input value={outputName} onChange={(e) => setOutputName(e.target.value)} />
            </label>
            </fieldset>
            {input&&<p className="ovg-processing-description">CRS đầu vào EPSG:{input.source.stored_crs?.code} · {input.source.feature_count} đối tượng · revision {input.source.content_revision}</p>}
            <button type="button" className="ovg-btn" disabled={busy||!canWrite||(!pending&&Boolean(validation))} onClick={() => void runProcessing()}>
              {busy?"Đang thực hiện…":pending?"Xác minh / gửi lại tác vụ":"Chạy tác vụ"}
            </button>
          {validation&&!pending&&<p className="ovg-processing-description" role="status">{validation}</p>}
        </section>
        <section className="ovg-card">
          <div className="ovg-processing-history-head"><h2>Lịch sử tác vụ ({jobs.length})</h2><label>Trạng thái <select aria-label="Lọc trạng thái tác vụ" value={filter} onChange={e=>setFilter(e.target.value)}><option value="">Tất cả</option>{["queued","running","succeeded","failed","cancelled"].map(v=><option value={v} key={v}>{processingStatusLabel(v)}</option>)}</select></label><button type="button" onClick={()=>void jobsQuery.refetch()}>Làm mới</button></div>
          {jobsQuery.isError&&<p role="alert">Không tải được lịch sử tác vụ.</p>}
          <div className="ovg-processing-table-scroll">
          <table className="ovg-table">
            <thead>
              <tr>
                <th>Thuật toán / ID</th>
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
                  <td>{algorithmLabel(j.algorithm_id)}<br/><small>{j.id}</small><br/><small>Lần chạy: {j.attempt_count}</small></td>
                  <td>{j.output_name}</td>
                  <td>
                    <span className="ovg-badge">{processingStatusLabel(j.status)}</span>
                  </td>
                  <td>{j.feature_count ?? "—"}</td>
                  <td>{j.engine_version}<br/><small>EPSG:{j.output_crs}</small>{j.error_message&&<p role="alert">{j.error_message}</p>}</td>
                  <td>
                    {j.status === "queued" && (
                      <button
                        type="button"
                        className="ovg-btn ovg-btn-secondary"
                        disabled={busy||!canWrite||!!pending} onClick={()=>void mutate(async()=>cancelProcessingJob(apiClient,organizationId,projectId,j.id,await loadCsrfToken(apiClient)))}
                      >
                        Hủy
                      </button>
                    )}
                    {(j.status === "queued" || j.status === "failed" || j.status === "cancelled") && (
                      <button
                        type="button"
                        className="ovg-btn ovg-btn-secondary"
                        disabled={busy||!canWrite||!!pending} onClick={()=>void mutate(async()=>retryProcessingJob(apiClient,organizationId,projectId,j.id,await loadCsrfToken(apiClient)))}
                      >
                        {j.status==="queued"?"Chạy tác vụ đang chờ":"Thử lại"}
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
