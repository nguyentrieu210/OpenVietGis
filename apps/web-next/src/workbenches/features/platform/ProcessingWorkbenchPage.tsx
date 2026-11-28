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
import {processingLayer,processingRequest,readPendingProcessing,supportedAlgorithm,verifyProcessingReceipt,overlayAlgorithm,type Algorithm,type ProcessingRequest} from "./processing-request";

import {appendBatch,readBatch,receivedBatch,nextBatchRow,MAX_BATCH,type BatchRow} from "./processing-batch";

const activeProcessingScopes=new Set<string>();
async function definitelyRejected(error:unknown){
  if(!(error instanceof ApiError))return false;
  if([400,403,404,422].includes(error.status))return true;
  if(error.status!==409)return false;
  try{const body=await error.response.clone().json();return body.code==="STALE_INPUT_PIN"||body.error?.code==="STALE_INPUT_PIN"}catch{return false}
}
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
    dissolve: "Gộp vùng",
    difference: "Hiệu hình học",
    intersection: "Giao hình học"
  };
  return labels[id] ?? id;
}

export function ProcessingWorkbenchPage() {
  const { organizationId = "", projectId = "" } = useParams();
  const qc = useQueryClient();
  const [search] = useSearchParams();
  const requested = search.get("algorithm");
  const [algo, setAlgo] = useState<Algorithm>(supportedAlgorithm(requested)?requested:"buffer");
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
  const [batch,setBatch]=useState<BatchRow[]>([]),[batchError,setBatchError]=useState(""),[batchRunning,setBatchRunning]=useState(false);
  const stopBatch=useRef(false),alive=useRef(true);
  useEffect(()=>{alive.current=true;return()=>{alive.current=false;stopBatch.current=true}},[]);
  const [loadedKey,setLoadedKey]=useState("");
  const userQuery=useQuery({queryKey:["r008",organizationId,projectId,"identity"],queryFn:()=>getCurrentUser(apiClient),enabled:!!organizationId&&!!projectId});
  const accessQuery=useQuery({queryKey:["r008",organizationId,projectId,"permissions"],queryFn:async()=>{const [p,a]=await Promise.all([getEffectivePermissions(apiClient,organizationId,{project_id:projectId}),apiClient.request<{restricted:boolean}>("/organizations/"+encodeURIComponent(organizationId)+"/projects/"+encodeURIComponent(projectId)+"/access/mine")]);return p.permissions.includes("project.edit")&&a.restricted===false},enabled:!!userQuery.data?.id});
  const pendingKey=userQuery.data?.status==="active"?"ovg:processing:pending:v1:"+[userQuery.data.id,organizationId,projectId].map(encodeURIComponent).join(":"):"";
  useEffect(()=>{let value:ProcessingRequest|null=null;if(pendingKey)try{value=readPendingProcessing(sessionStorage.getItem(pendingKey))}catch{}setPending(value);try{setBatch(pendingKey?readBatch(sessionStorage.getItem(pendingKey+":batch")):[]);setBatchError("")}catch(e){setBatch([]);setBatchError(e instanceof Error?e.message:"Không đọc được nhật ký batch.")}setLoadedKey(pendingKey)},[pendingKey]);
  const remember=(value:ProcessingRequest|null)=>{if(!pendingKey)throw Error("Chưa xác minh tài khoản.");if(value)sessionStorage.setItem(pendingKey,JSON.stringify(value));else sessionStorage.removeItem(pendingKey);setPending(value)};
  const canWrite=accessQuery.data===true&&!!pendingKey&&loadedKey===pendingKey;
  const storeBatch=(rows:BatchRow[])=>{if(!pendingKey||batchError||activeProcessingScopes.has(pendingKey)&&!inFlight.current)throw Error("Không thể ghi nhật ký batch khi yêu cầu khác đang gửi.");if(!inFlight.current){const actual=readBatch(sessionStorage.getItem(pendingKey+":batch"));if(JSON.stringify(actual)!==JSON.stringify(batch)){setBatch(actual);throw Error("Nhật ký đã đổi; kiểm tra lại trước khi chỉnh sửa.")}}sessionStorage.setItem(pendingKey+":batch",JSON.stringify(rows));setBatch(rows)};
  const uncertainBatch=batch.some(r=>r.state==="uncertain");
  const canSingle=canWrite&&!uncertainBatch&&!batchRunning&&!batchError;
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
    if(!canSingle||(!pending&&validation)||inFlight.current)return;
    if(activeProcessingScopes.has(pendingKey)){setError("Một yêu cầu đang gửi trong phiên này. Mở lại công cụ sau khi có phản hồi.");return}try{const actual=readPendingProcessing(sessionStorage.getItem(pendingKey)),actualBatch=readBatch(sessionStorage.getItem(pendingKey+":batch"));if(JSON.stringify(actual)!==JSON.stringify(pending)||actualBatch.some(r=>r.state==="uncertain")){setPending(actual);setBatch(actualBatch);setError("Nhật ký đã đổi; kiểm tra trước khi gửi lại.");return}}catch(e){setError(String(e));return}activeProcessingScopes.add(pendingKey);
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
      if(!pending&&await definitelyRejected(err))try{remember(null)}catch{}
      setError(err instanceof Error?err.message:"Xử lý không thành công.");
    }finally{activeProcessingScopes.delete(pendingKey);inFlight.current=false;if(alive.current)setBusy(false)}
  }

  function addBatch(){
    if(!canWrite||validation||pending||busy||batchError||uncertainBatch)return;
    try{storeBatch(appendBatch(batch,processingRequest(algo,input,overlay,distance,segments,groupField,outputName,crypto.randomUUID())));setError(null)}catch(e){setError(e instanceof Error?e.message:String(e))}
  }
  async function runBatch(){
    if(!canWrite||pending||batchError||inFlight.current||nextBatchRow(batch)<0)return;
    if(activeProcessingScopes.has(pendingKey)){setError("Một yêu cầu đang gửi trong phiên này. Mở lại công cụ sau khi có phản hồi.");return}try{const actual=readBatch(sessionStorage.getItem(pendingKey+":batch")),actualPending=readPendingProcessing(sessionStorage.getItem(pendingKey));if(JSON.stringify(actual)!==JSON.stringify(batch)||actualPending){setBatch(actual);setPending(actualPending);setError("Nhật ký đã đổi; kiểm tra trước khi tiếp tục.");return}}catch(e){setError(String(e));return}activeProcessingScopes.add(pendingKey);
    inFlight.current=true;stopBatch.current=false;setBatchRunning(true);setBusy(true);setError(null);let rows=batch;
    try{
      while(alive.current&&!stopBatch.current){
        const index=nextBatchRow(rows);if(index<0)break;
        const row=rows[index]!,wasUncertain=row.state==="uncertain";
        const me=await getCurrentUser(apiClient);
        if(!alive.current||stopBatch.current)break;
        if(me.id!==userQuery.data?.id||me.status!=="active")throw Error("Tài khoản đã đổi; dừng batch.");
        const csrf=await loadCsrfToken(apiClient);
        if(!alive.current||stopBatch.current)break;
        // Write the immutable operation before sending; an interrupted request is recoverable.
        rows=rows.map((r,i)=>i===index?{request:row.request,state:"uncertain"}:r);storeBatch(rows);
        try{
          const job=await createProcessingJob(apiClient,organizationId,projectId,row.request,csrf);
          rows=rows.map((r,i)=>i===index?receivedBatch(row,job):r);storeBatch(rows);
        }catch(e){
          const definitive=!wasUncertain&&await definitelyRejected(e);
          rows=rows.map((r,i)=>i===index?{request:row.request,state:definitive?"rejected":"uncertain",error:(e instanceof Error?e.message:String(e)).slice(0,500)}:r);
          storeBatch(rows);
          if(!definitive||e instanceof ApiError&&(e.status===403||e.status===401))break;
        }
      }
      if(alive.current){setNotice("Batch: "+rows.filter(r=>r.state==="received").length+"/"+rows.length+" đã nhận biên nhận; kiểm tra trạng thái tác vụ bên dưới.");await qc.invalidateQueries({queryKey:["r008",organizationId,projectId]})}
    }catch(e){if(alive.current)setError(e instanceof Error?e.message:String(e))}
    finally{activeProcessingScopes.delete(pendingKey);inFlight.current=false;if(alive.current){setBatchRunning(false);setBusy(false)}}
  }
  function exportBatch(){
    const data=batch.map(r=>({...r,current_status:(jobsQuery.data?.items??[]).find(j=>j.id===r.jobId)?.status??r.status??null}));
    const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type:"application/json"})),a=document.createElement("a");a.href=url;a.download="processing-batch.json";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)
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
          {descriptor&&<p className="ovg-processing-description">{algo==="buffer"?"Tạo vùng đệm theo khoảng cách. Đầu vào địa lý được xử lý trong Web Mercator; không phải phép đo trắc địa chính xác.":algo==="clip"?"Cắt theo hợp các vùng phủ polygon.":algo==="difference"?"Trừ hợp các vùng phủ polygon; giữ ID nguồn.":algo==="intersection"?"Giao từng cặp đối tượng với vùng phủ polygon; giữ hai ID, không nối toàn bộ thuộc tính.":"Gộp hình học toàn lớp hoặc theo trường thuộc tính."}</p>}
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
            {overlayAlgorithm(algo) && (
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
            <button type="button" className="ovg-btn" disabled={busy||!canSingle||(!pending&&Boolean(validation))} onClick={() => void runProcessing()}>
              {busy?"Đang thực hiện…":pending?"Xác minh / gửi lại tác vụ":"Chạy tác vụ"}
            </button>
          <button type="button" className="ovg-btn ovg-btn-secondary" disabled={busy||!!pending||!!validation||!!batchError||uncertainBatch||batch.length>=MAX_BATCH} onClick={addBatch}>Thêm vào batch</button>
          {validation&&!pending&&<p className="ovg-processing-description" role="status">{validation}</p>}
        </section>
        <section className="ovg-card" aria-label="Batch Processing">
          <h2>Batch Processing ({batch.length}/{MAX_BATCH})</h2>
          <p>Thêm từng bộ tham số từ biểu mẫu phía trên. Chạy tuần tự; mỗi hàng tạo tác vụ độc lập, không rollback toàn batch. Không tự chạy lại sau khi đóng hoặc tải lại trang.</p>
          {batchError&&<p role="alert">{batchError}</p>}
          {uncertainBatch&&<p role="status">Có yêu cầu chưa xác minh. Tiếp tục sẽ gửi lại đúng mã thao tác cũ trước các hàng còn lại.</p>}
          <div className="ovg-processing-table-scroll"><table className="ovg-table"><thead><tr><th>Thuật toán</th><th>Đầu vào</th><th>Đầu ra</th><th>Tham số</th><th>Trạng thái</th><th>Hành động</th></tr></thead><tbody>
           {batch.map((r,i)=><tr key={r.request.operation_id}><td>{algorithmLabel(r.request.algorithm_id)}</td><td>{layers.find(l=>l.id===r.request.input_layer_id)?.name??r.request.input_layer_id}{r.request.overlay_layer_id&&" / "+(layers.find(l=>l.id===r.request.overlay_layer_id)?.name??r.request.overlay_layer_id)}</td><td>{r.request.output_name}</td><td><code>{JSON.stringify(r.request.parameters)}</code></td><td>{r.state==="received"?processingStatusLabel((jobsQuery.data?.items??[]).find(j=>j.id===r.jobId)?.status??r.status??""):""+({draft:"Chưa gửi",uncertain:"Chưa xác minh",rejected:"Bị từ chối"} as const)[r.state]}{r.jobId&&<small> · {r.jobId}</small>}{r.error&&<p>{r.error}</p>}</td><td>{r.state==="draft"&&<button type="button" disabled={busy||!!batchError||uncertainBatch} onClick={()=>{try{storeBatch(batch.filter((_,j)=>j!==i))}catch(e){setError(String(e))}}}>Bỏ hàng {i+1}</button>}</td></tr>)}
           {!batch.length&&<tr><td colSpan={6}>Chưa có yêu cầu batch.</td></tr>}
          </tbody></table></div>
          <button type="button" className="ovg-btn" disabled={busy||!canWrite||!!pending||!!batchError||nextBatchRow(batch)<0} onClick={()=>void runBatch()}>{uncertainBatch?"Xác minh / tiếp tục batch":"Chạy batch"}</button>
          <button type="button" disabled={!batchRunning} onClick={()=>{stopBatch.current=true;setNotice("Sẽ dừng trước hàng kế tiếp; không hủy yêu cầu đang gửi.")}}>Dừng sau hàng hiện tại</button>
          <button type="button" disabled={!batch.length||busy} onClick={exportBatch}>Xuất lịch sử JSON</button>
          <button type="button" disabled={busy||!!batchError||uncertainBatch||!batch.length} onClick={()=>{try{storeBatch(batch.filter(r=>r.state==="draft"))}catch(e){setError(String(e))}}}>Dọn hàng đã có kết quả</button>
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
