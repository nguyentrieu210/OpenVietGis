import {useEffect,useRef,useState} from "react";
import type {Geometry} from "geojson";
import type {StoredRaster} from "./ProjectRaster";
import type {StudioRow} from "./studio-contract";
import type {ApiClient} from "@openvietgis/client-sdk";
import {ModelVault,type ModelRecord} from "./ModelVault";
import {MODEL_MAX_BYTES,parseStaticModel,type StaticModel} from "@openvietgis/spatial-workspace/gltf";
export type LocalModel={name:string;mesh:StaticModel;rowKey:string;scale:string;rotation:string;offset:string;file?:File;saved?:ModelRecord};
export type SceneOptions={demId:string;reference:"local"|"orthometric"|"ellipsoidal";label:string;ack:boolean;model:LocalModel|null};
export const emptyScene=():SceneOptions=>({demId:"",reference:"local",label:"",ack:false,model:null});
export const eligibleDem=(r:StoredRaster)=>r.method==="band"&&(r.output_bands??1)===1;
export function geometryPoints(geometry:Geometry|null):[number,number][]{
 if(!geometry||geometry.type==="GeometryCollection")return [];
 const out:[number,number][]=[];
 const walk=(a:unknown)=>{if(!Array.isArray(a))return;if(a.length>=2&&typeof a[0]==="number"&&typeof a[1]==="number"){if(Number.isFinite(a[0])&&Number.isFinite(a[1])&&Math.abs(a[0])<=180&&Math.abs(a[1])<=85)out.push([a[0],a[1]])}else for(const b of a)walk(b)};
 walk(geometry.coordinates);return out;
}
export function anchorOf(geometry:Geometry|null):[number,number]|null{
 const points=geometryPoints(geometry);if(!points.length)return null;
 let l=180,r=-180,b=90,t=-90;for(const [x,y] of points){l=Math.min(l,x);r=Math.max(r,x);b=Math.min(b,y);t=Math.max(t,y)}
 if(r-l>180)return null;return [(l+r)/2,(b+t)/2];
}
export function modelNumbers(m:LocalModel){const nums=[m.scale,m.rotation,m.offset].map(v=>v.trim()?Number(v):NaN);const [scale,rotation,offset]=nums;if(!nums.every(Number.isFinite)||scale!<.01||scale!>100||Math.abs(rotation!)>360||Math.abs(offset!)>1000)throw Error("Kiểm tra tỷ lệ, góc xoay và độ lệch cao độ");return {scale:scale!,rotationDeg:rotation!,offset:offset!}}
export function Spatial3DSettings({options,onChange,rasters,row,rows,enabled,onBusy,client,modelPath,canEdit,expire}:{options:SceneOptions;onChange:(next:SceneOptions)=>void;rasters:StoredRaster[];row:StudioRow|undefined;rows:StudioRow[];enabled:boolean;onBusy:(busy:boolean)=>void;client:ApiClient;modelPath:string;canEdit:boolean;expire:()=>void}){
 const [busy,setBusy]=useState(false),[vaultBusy,setVaultBusy]=useState(false),[error,setError]=useState("");const live=useRef(true),generation=useRef(0),latest=useRef({enabled,options});latest.current={enabled,options};
 useEffect(()=>{live.current=true;return()=>{live.current=false;generation.current++;onBusy(false)}},[]);
 const patch=(p:Partial<SceneOptions>)=>onChange({...options,...p});
 const patchModel=(p:Partial<LocalModel>)=>{if(options.model)patch({model:{...options.model,...p}})};
 const attach=async(file:File)=>{if(!row||!anchorOf(row.geometry)||!enabled||busy)return;
 const ext=file.name.toLowerCase().split(".").at(-1);if((ext!=="glb"&&ext!=="gltf")||file.size>MODEL_MAX_BYTES||!file.size){setError("Chỉ tệp GLB/GLTF tự chứa tối đa 10 MiB.");return}
 const ticket=++generation.current;setBusy(true);onBusy(true);setError("");try{const bytes=await file.arrayBuffer();if(!live.current||ticket!==generation.current||!latest.current.enabled)return;const mesh=parseStaticModel(bytes,ext);onChange({...latest.current.options,model:{name:file.name,mesh,rowKey:row.key,scale:"1",rotation:"0",offset:"0",file}})}catch(e){if(live.current&&ticket===generation.current)setError(e instanceof Error?e.message:"Không mở được mô hình")}finally{if(live.current&&ticket===generation.current){setBusy(false);onBusy(false)}}};
 const attached=rows.find(r=>r.key===options.model?.rowKey),anchor=attached&&anchorOf(attached.geometry);
 return <section className="ovg-3d-settings" aria-label="Cấu hình 3D nâng cao">
 <fieldset disabled={!enabled||busy||vaultBusy}><legend>Địa hình DEM</legend><label>Raster địa hình <select value={options.demId} onChange={e=>patch({demId:e.target.value,ack:false})}><option value="">Không dùng địa hình</option>{rasters.filter(eligibleDem).map(r=><option key={r.id} value={r.id}>{r.name}</option>)}</select></label>
 {options.demId&&<><label>Tham chiếu cao độ <select value={options.reference} onChange={e=>patch({reference:e.target.value as SceneOptions["reference"],ack:false})}><option value="local">Mốc cục bộ</option><option value="orthometric">Cao độ thủy chuẩn</option><option value="ellipsoidal">Cao độ ellipsoid</option></select></label><label>Tên mốc/hệ cao độ <input maxLength={120} value={options.label} placeholder="Nhập mốc đã biết" onChange={e=>patch({label:e.target.value,ack:false})}/></label><label><input type="checkbox" checked={options.ack} onChange={e=>patch({ack:e.target.checked})}/> Tôi xác nhận raster là DEM, giá trị theo mét và mốc khai báo đúng.</label><small>DEM lấy mẫu tối đa 128×128; hiển thị mesh địa hình, vùng NoData để trống. Không quy đổi hệ cao độ.</small></>}
 </fieldset>
 <fieldset disabled={!enabled||busy||vaultBusy}><legend>Mô hình tĩnh GLB/GLTF</legend><label>Gắn mô hình vào đối tượng GIS đang chọn <input type="file" accept=".glb,.gltf" disabled={!row||!anchorOf(row.geometry)} onChange={e=>{const file=e.target.files?.[0];e.target.value="";if(file)void attach(file)}}/></label><small>Chỉ mesh tĩnh tự chứa, không texture/animation/tệp từ xa. Xem trước cục bộ; chỉ tải tệp lên khi chọn lưu vào kho. Tâm hộp bao GIS làm điểm gắn.</small>
 {options.model&&<><strong>{options.model.name} · {options.model.mesh.triangles.toLocaleString("vi-VN")} tam giác</strong><span>{attached?attached.layer.name+" · "+attached.feature.feature_id:"Đối tượng không còn hiển thị"}{anchor?" · "+anchor.map(v=>v.toFixed(7)).join(", "):""}</span><label>Tỷ lệ mô hình <input type="number" min=".01" max="100" step="any" value={options.model.scale} onChange={e=>patchModel({scale:e.target.value})}/></label><label>Góc xoay (°) <input type="number" min="-360" max="360" value={options.model.rotation} onChange={e=>patchModel({rotation:e.target.value})}/></label><label>Độ lệch cao độ (m) <input type="number" min="-1000" max="1000" step="any" value={options.model.offset} onChange={e=>patchModel({offset:e.target.value})}/></label><button onClick={()=>patch({model:null})}>Bỏ mô hình khỏi phiên</button></>}
 </fieldset><ModelVault client={client} path={modelPath} options={options} onChange={onChange} rows={rows} enabled={enabled&&!busy} canEdit={canEdit} expire={expire} onBusy={v=>{setVaultBusy(v);onBusy(v)}}/><span role="status">{busy?"Đang đọc mô hình…":error}</span></section>;
}
