import {useEffect,useRef,useState} from "react";
import {loadCsrfToken,type ApiClient} from "@openvietgis/client-sdk";
type Zone={code:number;central_meridian:number;scale_factor:number;bounds:number[]};
export type ProjectionContext={client:ApiClient;org:string;pid:string};
type Receipt={organization_id:string;project_id:string;source_epsg:number;target_epsg:number;coordinates:{x:number;y:number}[];scope:string};
export function Vn2000Projection({client,org,pid}:ProjectionContext){
 const [zones,setZones]=useState<Zone[]>([]),[source,setSource]=useState(4756),[target,setTarget]=useState(9212),[raw,setRaw]=useState(""),[ack,setAck]=useState(false),[busy,setBusy]=useState(false),[result,setResult]=useState(""),[error,setError]=useState("");
 const [opened,setOpened]=useState(false);const active=useRef<AbortController|null>(null),generation=useRef(0);
 const path="/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid)+"/spatial/vn2000-projections";
 useEffect(()=>{
  if(!opened)return;const ctl=new AbortController();setZones([]);setResult("");setError("");
  client.request<{organization_id:string;project_id:string;geographic_code:number;scope:string;zones:Zone[]}>(path,{signal:ctl.signal,cache:"no-store"}).then(r=>{
   if(ctl.signal.aborted)return;
   if(r.organization_id!==org||r.project_id!==pid||r.geographic_code!==4756||r.scope!=="mathematical_same_datum_projection_only"||!Array.isArray(r.zones)||r.zones.length>30||r.zones.some(z=>!Number.isInteger(z.code)||!Number.isFinite(z.central_meridian)||![.9996,.9999].includes(z.scale_factor)||!Array.isArray(z.bounds)||z.bounds.length!==4||!z.bounds.every(Number.isFinite)))throw Error("Danh mục phép chiếu không hợp lệ.");
   setZones(r.zones);
  }).catch(e=>{if(!ctl.signal.aborted)setError(e instanceof Error?e.message:String(e))});
  return()=>{ctl.abort();active.current?.abort();generation.current++};
 },[opened,client,path]);
 const clear=()=>{generation.current++;active.current?.abort();setBusy(false);setResult("");setError("")};
 async function convert(){
  if(!ack||busy||!zones.length)return;clear();const ctl=new AbortController(),ticket=++generation.current;active.current=ctl;setBusy(true);
  const valid=()=>!ctl.signal.aborted&&generation.current===ticket;
  try{
   const lines=raw.trim().split(/\r?\n/);if(!raw.trim()||lines.length>256)throw Error("Nhập từ 1 đến 256 dòng XY.");
   const coordinates=lines.map((line,i)=>{const parts=line.trim().split(/[\s,;]+/);if(parts.length!==2||parts.some(p=>!/^[-+]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(p)))throw Error("Dòng "+(i+1)+": cần hai số XY, dấu thập phân là dấu chấm.");return {x:Number(parts[0]),y:Number(parts[1])}});
   await loadCsrfToken(client);if(!valid())return;
   const r=await client.request<Receipt>(path,{method:"POST",signal:ctl.signal,cache:"no-store",body:JSON.stringify({source_epsg:source,target_epsg:target,acknowledge_native_datum:true,coordinates})});
   if(!valid())return;
   if(r.organization_id!==org||r.project_id!==pid||r.source_epsg!==source||r.target_epsg!==target||r.scope!=="mathematical_same_datum_projection_only"||!Array.isArray(r.coordinates)||r.coordinates.length!==coordinates.length||r.coordinates.some(p=>!Number.isFinite(p.x)||!Number.isFinite(p.y)))throw Error("Kết quả chuyển tọa độ không khớp yêu cầu.");
   setResult(r.coordinates.map(p=>p.x.toFixed(target===4756?9:3)+"\t"+p.y.toFixed(target===4756?9:3)).join("\n"));
  }catch(e){if(valid())setError(e instanceof Error?e.message:String(e))}finally{if(valid())setBusy(false)}
 }
 const choices=<><option value={4756}>EPSG:4756 · VN-2000 địa lý (độ)</option>{zones.map(z=><option key={z.code} value={z.code}>EPSG:{z.code} · {z.scale_factor===.9996?"UTM":"TM-3"} · {z.central_meridian}°</option>)}</>;
 return <details onToggle={e=>setOpened(e.currentTarget.open)}><summary>Chuyển tọa độ trong cùng datum VN-2000 (20 hệ chiếu)</summary>{opened&&<div className="crs-detail"><p>Đổi phép chiếu, không đổi datum. Không sử dụng WGS84 làm dữ liệu đầu vào. Giới hạn vùng theo EPSG, không xác nhận ranh giới tỉnh hoặc độ chính xác địa chính. Không sửa lớp dữ liệu.</p>
 <label>CRS nguồn<select value={source} disabled={busy} onChange={e=>{clear();setSource(Number(e.target.value));setAck(false)}}>{choices}</select></label>
 <label>CRS đích<select value={target} disabled={busy} onChange={e=>{clear();setTarget(Number(e.target.value));setAck(false)}}>{choices}</select></label>
 <label>Tọa độ XY (kinh độ, vĩ độ hoặc Đông, Bắc)<textarea rows={4} maxLength={20000} value={raw} disabled={busy} onChange={e=>{clear();setRaw(e.target.value)}} placeholder="106.70 11.50"/></label>
 <label><input type="checkbox" checked={ack} disabled={busy} onChange={e=>{clear();setAck(e.target.checked)}}/> Xác nhận toàn bộ tọa độ thực sự thuộc datum VN-2000 và thứ tự XY.</label>
 <button type="button" disabled={!ack||busy||!zones.length||!raw.trim()} onClick={()=>void convert()}>{busy?"Đang chuyển…":"Chuyển tọa độ"}</button>{busy&&<button type="button" onClick={clear}>Hủy</button>}
 {error&&<p role="alert">{error}</p>}{result&&<label>Kết quả XY<textarea aria-label="Kết quả chuyển tọa độ" rows={5} readOnly value={result}/></label>}
 </div>}</details>;
}
