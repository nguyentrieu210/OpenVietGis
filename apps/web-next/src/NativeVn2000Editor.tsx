import {useEffect,useRef,useState} from "react";
import {attributeValue} from "./attribute-value";
import {loadCsrfToken,type ApiClient,type SpatialLayer} from "@openvietgis/client-sdk";
type NativeFeature={feature_id:string;attributes:Record<string,unknown>;geometry:{type:string;coordinates:unknown;coordinate_reference:{code:number}}};
type Props={client:ApiClient;org:string;pid:string;layer:SpatialLayer;onSaved:()=>void};
export function NativeVn2000Editor({client,org,pid,layer,onSaved}:Props){
 const [raw,setRaw]=useState(""),[ack,setAck]=useState(false),[busy,setBusy]=useState(false),[notice,setNotice]=useState("");
 const [items,setItems]=useState<NativeFeature[]>([]),[cursor,setCursor]=useState<string|null>(null),[pageBusy,setPageBusy]=useState(false),[pageLoaded,setPageLoaded]=useState(false),[fid,setFid]=useState(""),[field,setField]=useState(""),[value,setValue]=useState(""),[nil,setNil]=useState(false),[geometry,setGeometry]=useState("");
 const readAbort=useRef<AbortController|null>(null);
 const pending=useRef<{raw:string;operation_id:string}|null>(null),abort=useRef<AbortController|null>(null);
 const crs=layer.source.stored_crs?.code,source=layer.source_id,content=layer.source.content_revision,schema=layer.source.schema_revision;
 useEffect(()=>()=>{abort.current?.abort();readAbort.current?.abort()},[]);
 const activeFeature=items.find(f=>f.feature_id===fid),fields=layer.source.schema.fields;
 const base="/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid);
 async function readPage(after?:string){
  if(pageBusy||busy||pending.current||!crs)return;setPageBusy(true);setNotice("");
  readAbort.current?.abort();const ctl=new AbortController();readAbort.current=ctl;
  try{
   const q=new URLSearchParams({content_revision:content,schema_revision:schema,include_geometry:"true",response_crs:String(crs),limit:"100"});if(after)q.set("after_feature_id",after);
   const r=await client.request<{context:{organization_id:string;project_id:string;layer_id:string;source_id:string;content_revision:string;schema_revision:string;response_crs:{code:number}};items:NativeFeature[];next_after_feature_id:string|null}>(base+"/layers/"+encodeURIComponent(layer.id)+"/features?"+q,{signal:ctl.signal,cache:"no-store"});
   if(ctl.signal.aborted)return;
   const c=r.context;if(c.organization_id!==org||c.project_id!==pid||c.layer_id!==layer.id||c.source_id!==source||c.content_revision!==content||c.schema_revision!==schema||c.response_crs?.code!==crs||!Array.isArray(r.items)||r.items.length>100||r.items.some(x=>typeof x.feature_id!=="string"||x.geometry?.coordinate_reference?.code!==crs||!x.attributes||typeof x.attributes!=="object"))throw Error("Kết quả đọc tọa độ gốc không khớp phiên bản.");
   setItems(r.items);setCursor(r.next_after_feature_id);setPageLoaded(true);setFid("");setField("");setGeometry("");setValue("");setAck(false);
  }catch(e){if(!ctl.signal.aborted)setNotice(e instanceof Error?e.message:String(e))}finally{if(!ctl.signal.aborted)setPageBusy(false)}
 }
 function selectFeature(id:string){const r=items.find(x=>x.feature_id===id);setFid(id);setField("");setValue("");setNil(false);setGeometry(r?JSON.stringify({type:r.geometry.type,coordinates:r.geometry.coordinates},null,2):"");setAck(false)}
 function stage(kind:"update"|"delete"|"attributes"){
  if(!activeFeature||busy||pending.current)return;
  try{let op:Record<string,unknown>={op_type:kind==="attributes"?"update":kind,feature_id:fid};
   if(kind==="update"){const g=JSON.parse(geometry);if(!g||g.type!==layer.source.schema.geometry_family||!Array.isArray(g.coordinates)||Object.keys(g).some(k=>!["type","coordinates"].includes(k)))throw Error("Hình học phải đúng loại của lớp.");op.geometry=g}
   if(kind==="attributes"){const f=fields.find(x=>x.field_id===field);if(!f)throw Error("Chọn trường.");const next=attributeValue(f,value,nil);op.properties={...activeFeature.attributes,[field]:next}}
   setRaw(JSON.stringify([op],null,2));setAck(false);setNotice("Đã chuẩn bị thao tác; kiểm tra lại và xác nhận EPSG trước khi lưu.");
  }catch(e){setNotice(e instanceof Error?e.message:String(e))}
 }
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
 <button type="button" disabled={busy||pageBusy||!!pending.current} onClick={()=>void readPage()}>Đọc 100 đối tượng đầu</button> <button type="button" disabled={busy||pageBusy||!cursor||!!pending.current} onClick={()=>void readPage(cursor??undefined)}>Trang tiếp</button>{pageLoaded&&<p>{items.length} đối tượng trong trang · EPSG:{crs}</p>}
 {items.length>0&&<fieldset disabled={busy||pageBusy||!!pending.current}><legend>Biên tập đối tượng đang đọc</legend><label>FID<select value={fid} onChange={e=>selectFeature(e.target.value)}><option value="">Chọn đối tượng</option>{items.map(r=><option key={r.feature_id} value={r.feature_id}>{r.feature_id}</option>)}</select></label>{activeFeature&&<><label>Trường thuộc tính<select value={field} onChange={e=>{const key=e.target.value,v=activeFeature.attributes[key];setField(key);setValue(v==null?"":String(v));setNil(v==null)}}><option value="">Chọn trường</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>{field&&<><label>Giá trị<input disabled={nil} value={value} onChange={e=>setValue(e.target.value)}/></label><label><input type="checkbox" checked={nil} onChange={e=>setNil(e.target.checked)}/>NULL</label><button type="button" onClick={()=>stage("attributes")}>Chuẩn bị sửa thuộc tính</button></>}<label>Hình học XY (m)<textarea rows={5} value={geometry} onChange={e=>setGeometry(e.target.value)}/></label><button type="button" onClick={()=>stage("update")}>Chuẩn bị sửa hình học</button> <button type="button" onClick={()=>stage("delete")}>Chuẩn bị xóa đối tượng</button></>}</fieldset>}
 <label>Thao tác JSON<textarea rows={7} maxLength={512000} value={raw} disabled={busy||!!pending.current} onChange={e=>{setRaw(e.target.value);setAck(false)}} placeholder={'[{"op_type":"update","feature_id":"pt_1","geometry":{"type":"Point","coordinates":[500000,1200000]}}]'}/></label>
 <label><input type="checkbox" checked={ack} disabled={busy} onChange={e=>setAck(e.target.checked)}/> Xác nhận EPSG:{crs}, tọa độ gốc và ảnh hưởng đến bằng chứng datum.</label>
 <button type="button" disabled={busy||!ack||!raw.trim()} onClick={()=>void save()}>{busy?"Đang lưu…":pending.current?"Thử lại cùng mã thao tác":"Lưu changeset"}</button>
 {pending.current&&!busy&&<button type="button" onClick={()=>{pending.current=null;setAck(false);setNotice("Đã bỏ mã thao tác cũ. Tải lại lớp trước khi gửi yêu cầu khác.");onSaved()}}>Bỏ yêu cầu và tải lại</button>}
 {notice&&<p role="status">{notice}</p>}</details>;
}
