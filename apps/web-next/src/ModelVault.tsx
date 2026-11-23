import {useEffect,useRef,useState} from "react";
import {ApiError,loadCsrfToken,type ApiClient} from "@openvietgis/client-sdk";
import {MODEL_MAX_BYTES,parseStaticModel} from "@openvietgis/spatial-workspace/gltf";
import {featureKey,type StudioRow} from "./studio-contract";
import {modelNumbers,modelId,checkModels,matchesReference,type LocalModel,type SceneOptions} from "./Spatial3DSettings";
type Profile={source_id:string;feature_id:string;source_revision:string;longitude:number;latitude:number;crs_code:4326;scale:number;rotation_deg:number;offset_m:number;dem_id:string|null;vertical_reference:SceneOptions["reference"];reference_label:string;altitude_mode:"relative-dem"|"relative-flat"};
export type ModelRecord={id:string;filename:string;format:"glb"|"gltf";sha256:string;byte_size:number;version:number;profile:Profile;created_at:string};
const uuid=(v:unknown):v is string=>typeof v==="string"&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v);
function valid(v:ModelRecord):boolean{
 const p=v?.profile;return !!p&&uuid(v.id)&&typeof v.filename==="string"&&["glb","gltf"].includes(v.format)&&/^[0-9a-f]{64}$/.test(v.sha256)&&Number.isInteger(v.byte_size)&&v.byte_size>0&&v.byte_size<=MODEL_MAX_BYTES&&Number.isInteger(v.version)&&v.version>=1&&v.version<=50&&uuid(p.source_id)&&typeof p.feature_id==="string"&&/^[0-9a-f]{64}$/.test(p.source_revision)&&p.crs_code===4326&&[p.longitude,p.latitude,p.scale,p.rotation_deg,p.offset_m].every(Number.isFinite)&&Math.abs(p.longitude)<=180&&Math.abs(p.latitude)<=85&&p.scale>=.01&&p.scale<=100&&Math.abs(p.rotation_deg)<=360&&Math.abs(p.offset_m)<=1000&&["local","orthometric","ellipsoidal"].includes(p.vertical_reference)&&typeof p.reference_label==="string"&&(p.dem_id===null?p.altitude_mode==="relative-flat"&&p.reference_label===""&&p.vertical_reference==="local":uuid(p.dem_id)&&p.altitude_mode==="relative-dem"&&!!p.reference_label);
}
export function ModelVault({client,path,options,onChange,rows,enabled,canEdit,expire,onBusy}:{client:ApiClient;path:string;options:SceneOptions;onChange:(s:SceneOptions)=>void;rows:StudioRow[];enabled:boolean;canEdit:boolean;expire:()=>void;onBusy:(b:boolean)=>void}){
 const [items,setItems]=useState<ModelRecord[]>([]),[history,setHistory]=useState<ModelRecord[]>([]),[selected,setSelected]=useState(""),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const active=useRef(false),abort=useRef(new AbortController()),pending=useRef(false),latest=useRef({options,rows,canEdit});latest.current={options,rows,canEdit};
 const list=async(signal:AbortSignal)=>{const r=await client.request<{items:ModelRecord[]}>(path,{cache:"no-store",signal});if(!signal.aborted)setItems(r.items.filter(valid))};
 const error=(e:unknown)=>{if(e instanceof ApiError&&e.status===401)expire();else setMessage(e instanceof ApiError?e.status===409?"Phiên bản đã thay đổi, tải lại kho.":e.status===403?"Không có quyền.":e.status===422?"Không hợp lệ: kiểm tra mô hình, đối tượng GIS và mốc DEM.":"Không tải/lưu được kho mô hình.":e instanceof Error?e.message:"Không tải/lưu được kho mô hình.")};
 useEffect(()=>{active.current=true;const a=new AbortController();abort.current=a;pending.current=false;setItems([]);setHistory([]);setSelected("");setMessage("");void list(a.signal).catch(e=>{if(!a.signal.aborted)error(e)});return()=>{active.current=false;a.abort();onBusy(false)}},[client,path]);
 const task=async(fn:(signal:AbortSignal)=>Promise<void>)=>{if(!enabled||pending.current)return;const signal=abort.current.signal;pending.current=true;setBusy(true);onBusy(true);setMessage("");try{await fn(signal)}catch(e){if(!signal.aborted)error(e)}finally{if(!signal.aborted){pending.current=false;setBusy(false);onBusy(false)}}};
 const loadHistory=async(id:string,signal:AbortSignal)=>{const r=await client.request<{items:ModelRecord[]}>(path+"/"+id+"/placements",{cache:"no-store",signal});if(!signal.aborted){setSelected(id);setHistory(r.items.filter(v=>valid(v)&&v.id===id))}};
 const readModel=async(v:ModelRecord,signal:AbortSignal):Promise<LocalModel>=>{
  const response=await client.requestRaw(path+"/"+v.id+"/download",{cache:"no-store",signal});
  const bytes=await response.arrayBuffer();if(bytes.byteLength!==v.byte_size)throw Error("Kích thước mô hình không khớp.");
  const digest=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),n=>n.toString(16).padStart(2,"0")).join("");
  if(digest!==v.sha256)throw Error("SHA-256 mô hình không khớp.");const mesh=parseStaticModel(bytes,v.format);if(signal.aborted)throw new DOMException("Aborted","AbortError");
  const p=v.profile;return {name:v.filename,mesh,rowKey:featureKey(p.source_id,p.feature_id),scale:String(p.scale),rotation:String(p.rotation_deg),offset:String(p.offset_m),saved:v};
 };
 const configured=(o:SceneOptions,v:ModelRecord)=>({...o,demId:v.profile.dem_id??"",reference:v.profile.vertical_reference,label:v.profile.reference_label,ack:matchesReference(o,v)&&o.ack});
 const open=(v:ModelRecord)=>void task(async signal=>{
  const model=await readModel(v,signal);if(signal.aborted)return;
  const next={...configured(latest.current.options,v),model};checkModels(next);onChange(next);
  setMessage("Đã mở mô hình v"+v.version+(v.profile.dem_id?" · xác nhận DEM trước khi xem 3D.":"."));
 });
 const pin=(v:ModelRecord)=>void task(async signal=>{
  const o=latest.current.options;
  if((o.models.length||o.model||o.demId)&&!matchesReference(o,v))throw Error("Mô hình không dùng chung DEM/mốc cao độ của cảnh. Bỏ các mô hình trước hoặc chọn đúng DEM.");
  const existing=o.models.find(m=>m.saved?.id===v.id&&m.saved.version===v.version);
  if(existing){setMessage("Mô hình này đã có trong cảnh.");return}
  const model=await readModel(v,signal);if(signal.aborted)return;
  const current=latest.current.options;
  if((current.models.length||current.model||current.demId)&&!matchesReference(current,v))throw Error("Mốc cao độ cảnh đã thay đổi.");
  // Pin published version; if the same model is being edited, the explicit pinned revision wins.
  const next={...configured(current,v),models:[...current.models.filter(m=>modelId(m)!==v.id),model],model:current.model&&modelId(current.model)===v.id?null:current.model};
  checkModels(next);onChange(next);setMessage("Đã thêm "+v.filename+" · v"+v.version+" vào cảnh.");
 });
 const save=()=>void task(async signal=>{
  const o=latest.current.options,m=o.model;if(!m||!latest.current.canEdit)return;const n=modelNumbers(m),r=latest.current.rows.find(r=>r.key===m.rowKey),source=m.saved?.profile.source_id??r?.layer.source_id,feature=m.saved?.profile.feature_id??r?.feature.feature_id;
  if(!source||!feature)throw Error("Chọn đối tượng GIS trước khi lưu.");
  if(o.demId&&(!o.ack||!o.label.trim()))throw Error("Xác nhận DEM và mốc cao độ trước khi lưu.");
  const profile={source_id:source,feature_id:feature,scale:n.scale,rotation_deg:n.rotationDeg,offset_m:n.offset,dem_id:o.demId||null,vertical_reference:o.demId?o.reference:"local",reference_label:o.demId?o.label.trim():""};
  await loadCsrfToken(client);if(signal.aborted||!latest.current.canEdit)return;
  let result:ModelRecord;if(m.saved){result=await client.request<ModelRecord>(path+"/"+m.saved.id+"/placements",{method:"POST",body:JSON.stringify({...profile,expected_version:m.saved.version}),signal})}else{if(!m.file)throw Error("Chưa có tệp gốc.");const form=new FormData();form.set("file",m.file);form.set("placement",JSON.stringify(profile));result=await client.request<ModelRecord>(path,{method:"POST",body:form,signal})}
  if(signal.aborted)return;if(!valid(result))throw Error("Phản hồi kho mô hình không hợp lệ.");
  const saved={...m,file:undefined,saved:result};onChange({...o,model:saved,models:o.models.map(x=>modelId(x)===result.id?saved:x)});await list(signal);await loadHistory(result.id,signal);if(!signal.aborted)setMessage("Đã lưu định vị v"+result.version+".");
 });
 const download=(v:ModelRecord)=>void task(async signal=>{const response=await client.requestRaw(path+"/"+v.id+"/download",{cache:"no-store",signal});const blob=await response.blob();if(signal.aborted)return;const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download=v.filename;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)});
 const model=options.model;
 return <fieldset disabled={!enabled||busy}><legend>Kho mô hình 3D của dự án</legend>
 <small>Tệp gốc riêng tư, định vị có lịch sử. Thêm tối đa 8 mô hình cùng mốc cao độ vào cảnh; mở để sửa một mô hình. Mở vị trí đã lưu không tự dời; khi lưu định vị mới, hệ thống lấy lại tâm GIS hiện tại.</small>
 <button onClick={()=>void task(async signal=>{await list(signal);if(selected)await loadHistory(selected,signal)})}>Tải lại kho 3D</button>
 {canEdit&&model&&<button disabled={!!model.saved&&model.saved.version>=50} onClick={save}>{model.saved?"Lưu phiên bản định vị":"Lưu mô hình vào kho"}</button>}
 {model?.saved&&<small>Đang mở v{model.saved.version} · EPSG:4326 · {model.saved.profile.longitude.toFixed(7)}, {model.saved.profile.latitude.toFixed(7)}</small>}
 {!items.length&&<small>Chưa có mô hình đã lưu.</small>}
 {items.map(v=><div className="ovg-model-item" key={v.id}><strong>{v.filename} · v{v.version}</strong><button onClick={()=>open(v)}>Mở mô hình</button><button onClick={()=>pin(v)}>Thêm vào cảnh</button><button onClick={()=>void task(signal=>loadHistory(v.id,signal))}>Lịch sử định vị</button><button onClick={()=>download(v)}>Tải tệp gốc</button></div>)}
 {history.length>0&&<div><strong>Lịch sử định vị</strong>{history.map(v=><div className="ovg-model-item" key={v.version}><span>v{v.version} · {v.profile.scale}× · {v.profile.rotation_deg}° · {v.profile.offset_m} m</span><button onClick={()=>open(v)}>Mở v{v.version}</button><button onClick={()=>pin(v)}>Thêm v{v.version} vào cảnh</button></div>)}</div>}
 <span role="status">{busy?"Đang xử lý kho 3D…":message}</span></fieldset>
}
