import {useEffect,useMemo,useRef,useState,type ReactNode} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import type {Map as MLMap} from "maplibre-gl";
import type {StudioRow} from "./studio-contract";
import {ModeCoordinator} from "@openvietgis/spatial-workspace/coordinator";
import {createSession,type ProjectScope,type SpatialMode,type ModeCapability} from "@openvietgis/spatial-workspace/session";
import {SpatialModeSwitch} from "@openvietgis/spatial-workspace/ModeSwitch";
import {footprint3d,type Footprint} from "@openvietgis/spatial-workspace/footprint3d";
import "./spatial-workspace.css";

type CadVersion={id:string;filename:string;format:"dwg"|"dxf";version:number;byte_size:number;sha256:string;created_at:string};
type CadList={items:CadVersion[]};
const noEngine={activate(){},dispose(){}};
const allowed:ModeCapability={available:true};
const denied=(reason:string):ModeCapability=>({available:false,reason});
const errorText=(error:unknown)=>error instanceof ApiError?error.status===409?"Phiên bản đã thay đổi. Hãy tải lại danh sách.":error.status===403?"Không có quyền thao tác.":error.status===413?"Vượt giới hạn dung lượng hoặc số phiên bản.":error.status===422?"Tệp DWG/DXF không hợp lệ.":"Yêu cầu CAD thất bại.":"Không thể thực hiện thao tác.";

function CadVault({client,path,initial,canEdit,onBusy,expire}:{client:ApiClient;path:string;initial:CadVersion[];canEdit:boolean;onBusy:(busy:boolean)=>void;expire:()=>void}){
 const [docs,setDocs]=useState(initial),[selected,setSelected]=useState(""),[versions,setVersions]=useState<CadVersion[]>([]),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const active=useRef(true),abort=useRef(new AbortController());
 useEffect(()=>{active.current=true;abort.current=new AbortController();return()=>{active.current=false;abort.current.abort();onBusy(false)}},[]);
 const current=docs.find(d=>d.id===selected);
 const task=async(fn:()=>Promise<void>)=>{if(busy)return;setBusy(true);onBusy(true);setMessage("");try{await fn()}catch(e){if(active.current){if(e instanceof ApiError&&e.status===401)expire();else setMessage(errorText(e))}}finally{if(active.current){setBusy(false);onBusy(false)}}};
 const list=async()=>{const x=await client.request<CadList>(path,{cache:"no-store",signal:abort.current.signal});if(active.current)setDocs(x.items)};
 const history=async(id:string)=>{const x=await client.request<CadList>(path+"/"+id+"/versions",{cache:"no-store",signal:abort.current.signal});if(active.current){setSelected(id);setVersions(x.items)}};
 const save=(file:File,revision?:CadVersion)=>void task(async()=>{if(file.size>25*1024*1024){setMessage("Tối đa 25 MiB mỗi tệp.");return}await loadCsrfToken(client);if(!active.current)return;const form=new FormData();form.set("file",file);if(revision)form.set("expected_version",String(revision.version));const x=await client.request<CadVersion>(revision?path+"/"+revision.id+"/versions":path,{method:"POST",body:form,signal:abort.current.signal});if(!active.current)return;await list();await history(x.id);if(active.current)setMessage("Đã lưu phiên bản "+x.version+".")});
 const download=(v:CadVersion)=>void task(async()=>{const response=await client.requestRaw(path+"/"+v.id+"/versions/"+v.version+"/download",{cache:"no-store",signal:abort.current.signal});const blob=await response.blob();if(!active.current)return;const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download=v.filename;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)});
 return <section className="ovg-cad-vault"><header><h2>Kho bản vẽ CAD của dự án</h2><p>DWG / DXF · tệp gốc và lịch sử phiên bản riêng tư. Chưa tích hợp chỉnh sửa trực tiếp.</p></header>
 <div className="ovg-cad-actions"><button disabled={busy} onClick={()=>void task(async()=>{await list();if(selected)await history(selected)})}>Tải lại</button>{canEdit&&<label>Thêm bản vẽ <input type="file" accept=".dwg,.dxf" disabled={busy} onChange={e=>{const f=e.target.files?.[0];e.target.value="";if(f)save(f)}}/></label>}</div>
 <div className="ovg-cad-columns"><div><h3>Bản vẽ ({docs.length})</h3>{!docs.length&&<p>Chưa có bản vẽ trong dự án.</p>}{docs.map(d=><button className="ovg-cad-doc" aria-pressed={selected===d.id} disabled={busy} key={d.id} onClick={()=>void task(()=>history(d.id))}>{d.filename}<small>v{d.version} · {(d.byte_size/1024).toFixed(1)} KiB</small></button>)}</div><div><h3>Lịch sử phiên bản</h3>{!current&&<p>Chọn bản vẽ để xem lịch sử.</p>}{current&&canEdit&&<label>Lưu phiên bản mới <input type="file" accept={"."+current.format} disabled={busy||current.version>=20} onChange={e=>{const f=e.target.files?.[0];e.target.value="";if(f)save(f,current)}}/></label>}{versions.map(v=><div className="ovg-cad-version" key={v.version}><span>v{v.version} · {v.filename}<small>{new Date(v.created_at).toLocaleString("vi-VN")}</small></span><button disabled={busy} onClick={()=>download(v)}>Tải xuống</button></div>)}</div></div>
 <p role="status">{busy?"Đang xử lý…":message}</p></section>;
}
type Props={scope:ProjectScope;client:ApiClient;canRead:boolean;canEdit:boolean;blocked:boolean;rows:StudioRow[];camera:()=>MLMap|null;expire:()=>void;children:ReactNode};
export function SpatialWorkspace(props:Props){
 const {scope,client,rows}=props;
 const path="/organizations/"+encodeURIComponent(scope.organizationId)+"/projects/"+encodeURIComponent(scope.projectId)+"/cad/documents";
 const [session,setSession]=useState(()=>createSession(scope)),[field,setField]=useState(""),[docs,setDocs]=useState<CadVersion[]>([]),[cadBusy,setCadBusy]=useState(false),[picked,setPicked]=useState("");
 const host=useRef<HTMLDivElement>(null),controller=useRef<ModeCoordinator|null>(null),latest=useRef({props,field,cadBusy,path}),snapshot=useRef<StudioRow[]|null>(null);
 latest.current={props,field,cadBusy,path};
 const options=useMemo(()=>{const result=new Map<string,{key:string;label:string;layer:string;attribute:string}>();for(const row of rows){if(row.geometry?.type!=="Polygon"&&row.geometry?.type!=="MultiPolygon")continue;for(const [name,value] of Object.entries(row.feature.attributes)){if(typeof value!=="number"||!Number.isFinite(value)||value<=0||value>10000)continue;const key=JSON.stringify([row.layer.id,name]);result.set(key,{key,label:row.layer.name+" · "+name,layer:row.layer.id,attribute:name})}}return [...result.values()]},[rows]);
 const selectedField=options.find(o=>o.key===field);
 const selectedRef=useRef(selectedField);selectedRef.current=selectedField;
 const capability=(mode:SpatialMode):ModeCapability=>{const p=latest.current.props;if(mode==="gis")return allowed;if(!p.canRead)return denied("Không có quyền đầy đủ hoặc dữ liệu chưa sẵn sàng.");if(mode==="3d"&&(!selectedRef.current||!p.camera()))return denied("Chọn trường chiều cao (m) của lớp polygon.");return allowed};
 useEffect(()=>{
  const c=new ModeCoordinator(scope,{capability,blockers:()=>({dirty:latest.current.props.blocked,saving:latest.current.cadBusy}),onChange:setSession,prepare:async(mode,_scope,signal)=>{
   const p=latest.current.props;
   if(mode==="gis")return noEngine;
   if(mode==="cad"){try{const data=await p.client.request<CadList>(latest.current.path,{signal,cache:"no-store"});return {activate(){setDocs(data.items)},dispose(){setDocs([])}}}catch(e){if(!signal.aborted&&e instanceof ApiError&&e.status===401)p.expire();throw e}}
   const choice=selectedRef.current,m=p.camera(),target=host.current;if(!choice||!m||!target)throw Error("Missing 3D selection");
   const features:Footprint[]=[];for(const r of p.rows){const h=r.feature.attributes[choice.attribute];if(r.layer.id===choice.layer&&(r.geometry?.type==="Polygon"||r.geometry?.type==="MultiPolygon")&&typeof h==="number"&&Number.isFinite(h)&&h>0&&h<=10000)features.push({id:r.key,geometry:r.geometry,baseMetres:0,heightMetres:h})}
   if(!features.length)throw Error("No valid footprints");
   const view=footprint3d(features,"EPSG:4326","relative-flat-reference"),center=m.getCenter().wrap(),camera={longitude:center.lng,latitude:center.lat,zoom:Math.min(22,m.getZoom()),bearing:m.getBearing()};
   const {prepareFootprint3d}=await import("@openvietgis/spatial-workspace/maplibre3d");
   const engine=await prepareFootprint3d(target,view,camera,signal,setPicked);
   return {activate(){if(latest.current.props.rows!==p.rows)throw Error("GIS data changed");snapshot.current=p.rows;setPicked("");engine.activate()},dispose(){snapshot.current=null;engine.dispose()}};
  }});
  controller.current=c;setSession(c.state);return()=>{controller.current=null;c.dispose()};
 },[scope.accountId,scope.organizationId,scope.projectId]);
 useEffect(()=>{controller.current?.revalidate();if(snapshot.current&&snapshot.current!==rows)void controller.current?.switchTo("gis")},[props.canRead,rows,field]);
 useEffect(()=>{if(session.active==="gis")requestAnimationFrame(()=>props.camera()?.resize())},[session.active]);
 const active=session.active;
 return <div className="ovg-spatial-workspace">
 <div className="ovg-spatial-top"><SpatialModeSwitch session={session} capabilities={{gis:allowed,cad:capability("cad"),"3d":capability("3d")}} onSelect={m=>void controller.current?.switchTo(m)}/><label>Chiều cao 3D (m) <select disabled={active!=="gis"||!props.canRead||!!session.pending} value={field} onChange={e=>setField(e.target.value)}><option value="">Chọn trường số</option>{props.canRead&&options.map(o=><option key={o.key} value={o.key}>{o.label}</option>)}</select></label></div>
 <div className="ovg-spatial-body"><div style={{display:active==="gis"?"contents":"none"}} inert={active!=="gis"}>{props.children}</div>
 <div className="ovg-alternate-pane" data-ovg-alternate-active={active!=="gis"?active:undefined} style={{display:active!=="gis"&&!props.canRead?"none":undefined,visibility:active==="gis"?"hidden":"visible",pointerEvents:active==="gis"?"none":"auto"}} ref={host}>
 {active==="cad"&&props.canRead&&<CadVault client={client} path={path} initial={docs} canEdit={props.canEdit} onBusy={setCadBusy} expire={props.expire}/>}
 {active==="3d"&&props.canRead&&<div className="ovg-3d-note">3D từ polygon GIS · {selectedField?.label} · mốc phẳng 0 m{picked&&" · Đối tượng: "+picked}</div>}
 </div></div></div>;
}
