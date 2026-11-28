import {useRef,useState} from "react";
import {getPluginLicense,importPluginLicense,loadCsrfToken,type ApiClient} from "@openvietgis/client-sdk";
export function PluginLicensePanel({client,org,digest,onClose}:{client:ApiClient;org:string;digest:string;onClose:()=>void}){
 const [body,setBody]=useState(""),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const flight=useRef(false);
 const run=async(fn:()=>Promise<void>)=>{if(flight.current)return;flight.current=true;setBusy(true);setMessage("");try{await fn()}catch{setMessage("Không thể xác minh chứng thư. Kiểm tra tổ chức, release, chữ ký và quyền quản trị.")}finally{flight.current=false;setBusy(false)}};
 const status=()=>void run(async()=>{const r=await getPluginLicense(client,org,digest);setMessage("Trạng thái: "+r.state+" · "+(r.allowed?"Được sử dụng":"Chưa được sử dụng"))});
 const submit=()=>void run(async()=>{
   if(new TextEncoder().encode(body).length>16384)throw Error();
   const data=JSON.parse(body);
   if(data?.certificate?.release_digest!==digest||data?.certificate?.organization_id!==org)throw Error();
   await loadCsrfToken(client);const r=await importPluginLicense(client,org,data);
   setBody("");setMessage("Đã xác minh chứng thư · "+r.state+" · sequence "+r.sequence);
 });
 return <section className="partner-form" aria-label="Bản quyền plugin"><h4>Chứng thư sử dụng V2</h4><p>Chỉ nhập chứng thư do đơn vị cấp phép ký. Không nhập khóa riêng hoặc thông tin thanh toán. Quyền GIS vẫn phải được phê duyệt riêng.</p><code>{digest}</code>
 <button disabled={busy} onClick={status}>Kiểm tra bản quyền</button><label>Chứng thư JSON<textarea rows={5} maxLength={16384} value={body} disabled={busy} onChange={e=>setBody(e.target.value)}/></label>
 <button disabled={busy||!body.trim()} onClick={submit}>Xác minh và nhập chứng thư</button><button disabled={busy} onClick={onClose}>Đóng</button>{message&&<p role="status">{message}</p>}</section>;
}
