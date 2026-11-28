import {useEffect,useState} from "react";
import {listOwnPluginAudit,type ApiClient,type PluginAuditReceipt} from "@openvietgis/client-sdk";

/** Trusted host audit panel. Only the current user's scoped fingerprints. */
export function PluginAuditPanel({client,org,project,onClose}:{client:ApiClient;org:string;project:string;onClose:()=>void}){
 const [rows,setRows]=useState<PluginAuditReceipt[]>([]),[cursor,setCursor]=useState<string|undefined>(),[next,setNext]=useState<string|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState(""),[reload,setReload]=useState(0);
 useEffect(()=>{
  const controller=new AbortController();
  setRows([]);setNext(null);setLoading(true);setError("");
  void listOwnPluginAudit(client,org,project,cursor,50,controller.signal).then(r=>{
   if(controller.signal.aborted)return;
   if(r.scope!=="own_project_receipts"||!Array.isArray(r.items)||r.items.length>50)throw Error("Invalid audit response");
   setRows(r.items);setNext(r.next_after_id);
  }).catch(()=>{if(!controller.signal.aborted)setError("Không thể đọc lịch sử thực thi.")}).finally(()=>{if(!controller.signal.aborted)setLoading(false)});
  return()=>controller.abort();
 },[client,org,project,cursor,reload]);
 function exportPage(){
  if(loading||error||!rows.length)return;
  const data={schema:"openvietgis.audit.export.v1",scope:"own_project_receipts",organization_id:org,project_id:project,exported_at:new Date().toISOString(),after_id:cursor??null,next_after_id:next,items:rows};
  const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type:"application/json"}));
  const a=document.createElement("a");a.href=url;a.download="plugin-audit-page.json";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
 }
 return <section className="partner-form" aria-label="Lịch sử thực thi plugin của tôi">
  <h4>Lịch sử plugin V2 của tôi</h4>
  <p>Chỉ biên nhận lệnh thành công trong dự án hiện tại. Không chứa nội dung đầu vào hay kết quả GIS. Xuất từng trang, không phải bản sao lưu đầy đủ.</p>
  {loading&&<p role="status">Đang tải lịch sử…</p>}{error&&<p role="alert">{error}</p>}
  {!loading&&!error&&!rows.length&&<p>Chưa có biên nhận.</p>}
  {rows.map(r=><details key={r.id}><summary>{r.command_id} · {new Date(r.created_at).toLocaleString("vi-VN")}</summary><pre>{JSON.stringify(r,null,2)}</pre></details>)}
  <button disabled={loading} onClick={()=>{setCursor(undefined);setReload(v=>v+1)}}>Trang đầu / làm mới</button>
  <button disabled={loading||!next} onClick={()=>{if(next)setCursor(next)}}>Trang sau</button>
  <button disabled={loading||!!error||!rows.length} onClick={exportPage}>Xuất trang JSON</button>
  <button onClick={onClose}>Đóng lịch sử</button>
 </section>;
}
