import {useEffect,useRef,useState} from "react";
import {ApiError,loadCsrfToken,type ApiClient} from "@openvietgis/client-sdk";
import {fetchModelRecord,validModelRecord,type ModelRecord} from "./ModelVault";
import {checkModels,type LocalModel,type SceneOptions} from "./Spatial3DSettings";
export type SceneCamera={crs_code:4326;longitude:number;latitude:number;zoom:number;bearing:number;pitch:number};
export type SceneProfile={camera:SceneCamera;models:{id:string;version:number;visible:boolean}[];dem:{id:string;vertical_reference:SceneOptions["reference"];reference_label:string;visible:boolean}|null;footprint:{layer_id:string;height_field:string;visible:boolean}|null};
export type SceneRecord={id:string;version:number;name:string;profile:SceneProfile;created_at:string};
const uuid=(v:unknown):v is string=>typeof v==="string"&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v);
const version=(v:unknown):v is number=>Number.isInteger(v)&&Number(v)>=1&&Number(v)<=50;
export function validScene(v:SceneRecord):boolean{
 const p=v?.profile,c=p?.camera;
 return !!c&&uuid(v.id)&&version(v.version)&&typeof v.name==="string"&&!!v.name.trim()&&c.crs_code===4326&&[c.longitude,c.latitude,c.zoom,c.bearing,c.pitch].every(Number.isFinite)&&Math.abs(c.longitude)<=180&&Math.abs(c.latitude)<=85&&c.zoom>=0&&c.zoom<=22&&Math.abs(c.bearing)<=180&&c.pitch>=0&&c.pitch<=75&&Array.isArray(p.models)&&p.models.length<=8&&p.models.every(m=>uuid(m?.id)&&version(m.version)&&typeof m.visible==="boolean")&&new Set(p.models.map(m=>m.id)).size===p.models.length&&(p.dem===null||!!p.dem&&uuid(p.dem.id)&&["local","orthometric","ellipsoidal"].includes(p.dem.vertical_reference)&&typeof p.dem.reference_label==="string"&&!!p.dem.reference_label.trim()&&typeof p.dem.visible==="boolean")&&(p.footprint===null||!!p.footprint&&uuid(p.footprint.layer_id)&&typeof p.footprint.height_field==="string"&&!!p.footprint.height_field&&typeof p.footprint.visible==="boolean")&&!!(p.models.length||p.dem||p.footprint);
}
type Props={client:ApiClient;path:string;modelPath:string;mode:"gis"|"3d";enabled:boolean;canEdit:boolean;selected:SceneRecord|null;onSave:(record:SceneRecord)=>void;onLoad:(record:SceneRecord,models:LocalModel[])=>void;capture:()=>SceneProfile;onBusy:(v:boolean)=>void;expire:()=>void};
export function SavedScenePanel({client,path,modelPath,mode,enabled,canEdit,selected,onSave,onLoad,capture,onBusy,expire}:Props){
 const [items,setItems]=useState<SceneRecord[]>([]),[hist,setHist]=useState<SceneRecord[]>([]),[historyId,setHistoryId]=useState(""),[name,setName]=useState(selected?.name??""),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const abort=useRef(new AbortController()),running=useRef(false),latest=useRef({enabled,mode,canEdit,capture,onSave,onLoad});latest.current={enabled,mode,canEdit,capture,onSave,onLoad};
 const list=async(signal:AbortSignal)=>{const r=await client.request<{items:SceneRecord[]}>(path,{cache:"no-store",signal});if(!signal.aborted)setItems(r.items.filter(validScene))};
 const error=(e:unknown)=>{if(e instanceof ApiError&&e.status===401)expire();else setMessage(e instanceof ApiError?e.status===409?"Cảnh đã thay đổi hoặc lớp nguồn không còn khả dụng. Tải lại và kiểm tra.":e.status===403?"Không có quyền truy cập cảnh.":e.status===422?"Thông tin cảnh hoặc mốc cao độ không hợp lệ.":"Không xử lý được cảnh 3D.":e instanceof Error?e.message:"Không xử lý được cảnh 3D.")};
 useEffect(()=>{const a=new AbortController();abort.current=a;running.current=false;void list(a.signal).catch(e=>{if(!a.signal.aborted)error(e)});return()=>{a.abort();onBusy(false)}},[client,path]);
 useEffect(()=>setName(selected?.name??""),[selected?.id,selected?.version]);
 const task=async(fn:(signal:AbortSignal)=>Promise<void>)=>{if(!latest.current.enabled||running.current)return;running.current=true;const signal=abort.current.signal;setBusy(true);onBusy(true);setMessage("");try{await fn(signal)}catch(e){if(!signal.aborted)error(e)}finally{if(!signal.aborted){running.current=false;setBusy(false);onBusy(false)}}};
 const history=(id:string,signal:AbortSignal)=>client.request<{items:SceneRecord[]}>(path+"/"+id+"/versions",{cache:"no-store",signal}).then(r=>{if(!signal.aborted){setHistoryId(id);setHist(r.items.filter(v=>validScene(v)&&v.id===id))}});
 const open=(v:SceneRecord)=>void task(async signal=>{
  const d=await client.request<{scene:SceneRecord;models:ModelRecord[]}>(path+"/"+v.id+"/versions/"+v.version,{cache:"no-store",signal});
  if(!validScene(d.scene)||d.scene.id!==v.id||d.scene.version!==v.version||!Array.isArray(d.models)||d.models.length!==d.scene.profile.models.length)throw Error("Phiên bản cảnh không hợp lệ.");
  const models:LocalModel[]=[];let triangles=0;for(const pin of d.scene.profile.models){const item=d.models.find(m=>validModelRecord(m)&&m.id===pin.id&&m.version===pin.version);if(!item)throw Error("Thiếu phiên bản mô hình.");const model=await fetchModelRecord(client,modelPath,item,signal);triangles+=model.mesh.triangles;if(triangles>250000)throw Error("Cảnh vượt 250.000 tam giác.");models.push(model)}
  if(signal.aborted||latest.current.mode!=="gis")return;
  const dem=d.scene.profile.dem;checkModels({demId:dem?.id??"",reference:dem?.vertical_reference??"local",label:dem?.reference_label??"",ack:false,model:null,models});
  latest.current.onLoad(d.scene,models);setName(d.scene.name);setMessage("Đã mở cảnh v"+d.scene.version+(dem?" · xác nhận DEM trước khi vào 3D.":"."));
 });
 const save=(revise:boolean)=>void task(async signal=>{
  if(latest.current.mode!=="3d"||!latest.current.canEdit||!name.trim())return;
  const profile=latest.current.capture();
  await loadCsrfToken(client);if(signal.aborted||!latest.current.canEdit)return;
  const body={...profile,name:name.trim(),...(revise&&selected?{expected_version:selected.version}:{})};
  const r=await client.request<SceneRecord>(revise&&selected?path+"/"+selected.id+"/versions":path,{method:"POST",body:JSON.stringify(body),signal});
  if(signal.aborted)return;if(!validScene(r))throw Error("Phản hồi lưu cảnh không hợp lệ.");latest.current.onSave(r);await list(signal);await history(r.id,signal);if(!signal.aborted)setMessage("Đã lưu cảnh v"+r.version+".");
 });
 return <section className="ovg-saved-scenes" aria-label="Cảnh 3D dùng chung"><fieldset disabled={!enabled||busy}><legend>Cảnh 3D dùng chung</legend>
 <small>Cảnh lưu tham chiếu phiên bản mô hình, DEM, lớp GIS trực tiếp, trạng thái hiển thị và góc nhìn. Không lưu tệp nháp hoặc chia sẻ công khai.</small>
 <button onClick={()=>void task(async signal=>{await list(signal);if(historyId)await history(historyId,signal)})}>Tải lại cảnh</button>
 {mode==="3d"&&canEdit&&<><label>Tên cảnh 3D <input maxLength={160} value={name} onChange={e=>setName(e.target.value)} placeholder="Ví dụ: Tổng thể nhà máy"/></label><button disabled={!name.trim()} onClick={()=>save(false)}>Lưu thành cảnh mới</button>{selected&&<button disabled={!name.trim()||selected.version>=50} onClick={()=>save(true)}>Lưu phiên bản cảnh</button>}</>}
 {selected&&<small>Đang làm việc: {selected.name} · v{selected.version}</small>}
 {mode==="3d"&&<small>Về GIS để mở cảnh đã lưu.</small>}
 {!items.length&&<small>Chưa có cảnh.</small>}
 {items.map(r=><div className="ovg-model-item" key={r.id}><strong>{r.name} · v{r.version}</strong><button disabled={mode!=="gis"} onClick={()=>open(r)}>Mở cảnh</button><button onClick={()=>void task(signal=>history(r.id,signal))}>Lịch sử cảnh</button></div>)}
 {hist.map(r=><div className="ovg-model-item" key={r.version}><span>{r.name} · v{r.version}</span><button disabled={mode!=="gis"} onClick={()=>open(r)}>Mở cảnh v{r.version}</button></div>)}
 <span role="status">{busy?"Đang xử lý cảnh 3D…":message}</span></fieldset></section>
}
