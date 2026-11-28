import React,{useRef,useState} from "react";
import {applyPluginGraph,installPluginGraph,loadCsrfToken,previewPluginGraph,type ApiClient,type PluginGraphPreview,type PluginGraphSelection} from "@openvietgis/client-sdk";
type Props={client:ApiClient;org:string;project:string;selection:PluginGraphSelection;canRegister:boolean;canManage:boolean;onChange:()=>Promise<void>;onClose:()=>void};
export function PluginGraphPanel({client,org,project,selection,canRegister,canManage,onChange,onClose}:Props){
 const [target,setTarget]=useState<"organization"|"project">(canRegister?"organization":"project");
 const [rollback,setRollback]=useState("");
 const [review,setReview]=useState<{selection:PluginGraphSelection;preview:PluginGraphPreview;target:"organization"|"project";request:string}|null>(null);
 const [busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const flight=useRef(false);
 const run=async(action:()=>Promise<void>)=>{if(flight.current)return;flight.current=true;setBusy(true);setMessage("");try{await loadCsrfToken(client);await action()}catch{setMessage("Thao tác bị từ chối hoặc kết nối gián đoạn. Kiểm tra quyền và kế hoạch trước khi thử lại.")}finally{flight.current=false;setBusy(false)}};
 const inspect=()=>void run(async()=>{
   setReview(null);
   const selected={...selection,...(target==="project"&&rollback.trim()?{rollback_of:rollback.trim()}:{})};
   const preview=await previewPluginGraph(client,org,selected,target==="project"?project:undefined);
   setReview({selection:selected,preview,target,request:crypto.randomUUID()});
 });
 const confirm=()=>{if(!review)return;void run(async()=>{
   const r=review.target==="organization"?await installPluginGraph(client,org,review.selection,review.preview,review.request):
    await applyPluginGraph(client,org,project,review.selection,review.preview,review.request);
   setReview(null);setMessage("Đã ghi nhận thao tác: "+r.request_id+". Không tự cấp quyền runtime v2.");
   await onChange();
 })};
 return <section className="partner-form" aria-label="Quản lý đồ thị plugin"><h4>{selection.root} · {selection.version}</h4>
 <label>Phạm vi<select value={target} disabled={busy} onChange={e=>{setTarget(e.target.value as typeof target);setReview(null);setRollback("")}}>
 {canRegister&&<option value="organization">Cài toàn bộ vào tổ chức</option>}{canManage&&<option value="project">Kích hoạt / nâng cấp / rollback trong dự án</option>}</select></label>
 {target==="project"&&<label>Mã thao tác trước (chỉ nhập khi rollback)<input value={rollback} disabled={busy} maxLength={36} onChange={e=>{setRollback(e.target.value);setReview(null)}}/></label>}
 <p className="live-note">Cài vào tổ chức trước, sau đó chuyển phiên bản tại dự án. Rollback chỉ hỗ trợ đồ thị trước đó còn hợp lệ.</p>
 <button disabled={busy} onClick={inspect}>Xem trước đồ thị đã ký</button>
 {review&&<div><p>Catalog epoch {review.preview.plan.catalog_epoch}</p><ol>{review.preview.plan.pins.map(p=><li key={p.key}>{p.key} · {p.version}<ul>{p.permissions.map(permission=><li key={permission}>{permission}</li>)}</ul></li>)}</ol>
 {review.preview.plan.expected?.map(p=><p key={p.key}>Hiện tại: {p.key} · revision {p.revision} · {p.status}</p>)}
 <button disabled={busy} onClick={confirm}>Xác nhận đúng kế hoạch này</button></div>}
 {message&&<p role="status">{message}</p>}<button disabled={busy} onClick={onClose}>Đóng</button></section>;
}
