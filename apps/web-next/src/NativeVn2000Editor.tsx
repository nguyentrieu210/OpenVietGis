import {useEffect,useRef,useState} from "react";
import {loadCsrfToken,type ApiClient,type SpatialLayer} from "@openvietgis/client-sdk";
type Props={client:ApiClient;org:string;pid:string;layer:SpatialLayer;onSaved:()=>void};
export function NativeVn2000Editor({client,org,pid,layer,onSaved}:Props){
 const [raw,setRaw]=useState(""),[ack,setAck]=useState(false),[busy,setBusy]=useState(false),[notice,setNotice]=useState("");
 const pending=useRef<{raw:string;operation_id:string}|null>(null),abort=useRef<AbortController|null>(null);
 const crs=layer.source.stored_crs?.code,source=layer.source_id,content=layer.source.content_revision,schema=layer.source.schema_revision;
 useEffect(()=>()=>abort.current?.abort(),[]);
 const save=async()=>{
  if(busy||!ack||!crs)return;setBusy(true);setNotice("");
  const ctl=new AbortController();abort.current=ctl;
  try{
   const operations=JSON.parse(raw);
   if(!Array.isArray(operations)||operations.length<1||operations.length>100||operations.some(o=>!o||!["create","update","delete"].includes(o.op_type)||typeof o.feature_id!=="string"||!/^[A-Za-z0-9_-]{1,128}$/.test(o.feature_id)||Object.keys(o).some(k=>!["op_type","feature_id","geometry","properties"].includes(k))))throw Error("Nhập 1–100 thao tác JSON create/update/delete với feature_id.");
   if(!pending.current||pending.current.raw!==raw)pending.current={raw,operation_id:crypto.randomUUID()};
   const op=pending.current.operation_id;
   await loadCsrfToken(client);if(ctl.signal.aborted)return;
   const path="/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid)+"/sources/"+encodeURIComponent(source)+"/changesets";
   const receipt=await client.request<{id:string;operation_id:string;source_id:string;status:string}>(path,{method:"POST",signal:ctl.signal,body:JSON.stringify({operation_id:op,base_content_revision:content,base_schema_revision:schema,acknowledge_native_crs:crs,summary:"Native VN-2000 edit EPSG:"+crs,operations})});
   if(ctl.signal.aborted)return;
   if(receipt.operation_id!==op||receipt.source_id!==source||receipt.status!=="committed"||!receipt.id)throw Error("Biên nhận không khớp; giữ nguyên yêu cầu để kiểm tra.");
   pending.current=null;setRaw("");setAck(false);setNotice("Đã lưu changeset. Dữ liệu sửa không còn được coi là điểm kiểm chứng datum; có thể phải Reproject trong cùng VN-2000 để tiếp tục.");onSaved();
  }catch(e){if(!ctl.signal.aborted)setNotice(e instanceof Error?e.message:String(e))}finally{if(!ctl.signal.aborted)setBusy(false)}
 };
 return <details><summary>Biên tập tọa độ gốc EPSG:{crs}</summary>
 <p>Nhập tọa độ Đông/Bắc (m), không nhập kinh độ/vĩ độ WGS84. Mọi thay đổi được lưu theo changeset, kiểm tra phiên bản và miền chiếu. Sửa nguồn dẫn xuất có thể làm mất chứng cứ chuyển datum và khả năng hiển thị WGS84.</p>
 <label>Thao tác JSON<textarea rows={7} maxLength={512000} value={raw} disabled={busy||!!pending.current} onChange={e=>{setRaw(e.target.value);setAck(false)}} placeholder={'[{"op_type":"update","feature_id":"pt_1","geometry":{"type":"Point","coordinates":[500000,1200000]}}]'}/></label>
 <label><input type="checkbox" checked={ack} disabled={busy} onChange={e=>setAck(e.target.checked)}/> Xác nhận EPSG:{crs}, tọa độ gốc và ảnh hưởng đến bằng chứng datum.</label>
 <button type="button" disabled={busy||!ack||!raw.trim()} onClick={()=>void save()}>{busy?"Đang lưu…":pending.current?"Thử lại cùng mã thao tác":"Lưu changeset"}</button>
 {pending.current&&!busy&&<button type="button" onClick={()=>{pending.current=null;setAck(false);setNotice("Đã bỏ mã thao tác cũ. Tải lại lớp trước khi gửi yêu cầu khác.");onSaved()}}>Bỏ yêu cầu và tải lại</button>}
 {notice&&<p role="status">{notice}</p>}</details>;
}
