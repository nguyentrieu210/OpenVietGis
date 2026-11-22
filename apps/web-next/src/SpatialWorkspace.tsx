import {useEffect,useMemo,useRef,useState,type ReactNode} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import type {Map as MLMap} from "maplibre-gl";
import {featureKey,type StudioRow} from "./studio-contract";
import type {SelectableFootprintEngine} from "@openvietgis/spatial-workspace/maplibre3d";
import {ModeCoordinator} from "@openvietgis/spatial-workspace/coordinator";
import {createSession,type ProjectScope,type SpatialMode,type ModeCapability} from "@openvietgis/spatial-workspace/session";
import {SpatialModeSwitch} from "@openvietgis/spatial-workspace/ModeSwitch";
import {footprint3d,type Footprint} from "@openvietgis/spatial-workspace/footprint3d";
import "./spatial-workspace.css";
import {CadEditorPanel} from "./CadEditorPanel";
import {CadGeorefPanel} from "./CadGeorefPanel";
import type {StoredRaster} from "./ProjectRaster";
import {Spatial3DSettings,emptyScene,eligibleDem,anchorOf,geometryPoints,modelNumbers} from "./Spatial3DSettings";
import {validateTerrain,sampleTerrain,geographic3857,WEB_WORLD,type TerrainGrid} from "@openvietgis/spatial-workspace/terrain";
import type {PlacedModel} from "@openvietgis/spatial-workspace/mesh-layer";
import {cadRelayOrigin} from "@openvietgis/spatial-workspace/cad-editor";

type CadVersion={id:string;filename:string;format:"dwg"|"dxf";version:number;byte_size:number;sha256:string;created_at:string};
type CadList={items:CadVersion[]};
type CadLink={document_id:string;version:number;filename:string;source_id:string;layer_id:string;feature_id:string;handle:string;layer_name:string;original_revision:boolean};
type CadLinks={items:CadLink[]};
const validLink=(v:CadLink)=>typeof v.document_id==="string"&&typeof v.source_id==="string"&&typeof v.layer_id==="string"&&typeof v.feature_id==="string"&&typeof v.filename==="string"&&typeof v.layer_name==="string"&&typeof v.original_revision==="boolean"&&Number.isInteger(v.version)&&v.version>=1&&v.version<=20&&typeof v.handle==="string"&&/^[0-9a-fA-F]{1,32}$/.test(v.handle);
const noEngine={activate(){},dispose(){}};
const allowed:ModeCapability={available:true};
const denied=(reason:string):ModeCapability=>({available:false,reason});
const errorText=(error:unknown)=>error instanceof ApiError?error.status===409?"Phiên bản đã thay đổi. Hãy tải lại danh sách.":error.status===403?"Không có quyền thao tác.":error.status===413?"Vượt giới hạn dung lượng hoặc số phiên bản.":error.status===422?"Tệp DWG/DXF không hợp lệ.":"Yêu cầu CAD thất bại.":"Không thể thực hiện thao tác.";

function CadVault({client,path,initial,canEdit,onBusy,expire,onImported,focus,onPickLink}:{client:ApiClient;path:string;initial:CadVersion[];canEdit:boolean;onBusy:(busy:boolean)=>void;expire:()=>void;onImported:()=>void;focus:CadLink|null;onPickLink:(link:CadLink)=>boolean}){
 const [docs,setDocs]=useState(initial),[selected,setSelected]=useState(""),[versions,setVersions]=useState<CadVersion[]>([]),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const active=useRef(true),abort=useRef(new AbortController());
 useEffect(()=>{active.current=true;abort.current=new AbortController();return()=>{active.current=false;abort.current.abort();onBusy(false)}},[]);
 const current=docs.find(d=>d.id===selected);
 const [editor,setEditor]=useState<{doc:CadVersion;writable:boolean}|null>(null);
 const [georef,setGeoref]=useState<CadVersion|null>(null);
 const [linkTarget,setLinkTarget]=useState<{id:string;version:number}|null>(focus?{id:focus.document_id,version:focus.version}:null),[handle,setHandle]=useState(focus?.handle??""),[linked,setLinked]=useState<CadLink[]>(focus?[focus]:[]);
 useEffect(()=>{if(focus)void task(()=>history(focus.document_id))},[focus]);
 useEffect(()=>{if(!canEdit&&georef){setGeoref(null);onBusy(false)}},[canEdit,georef,onBusy]);
 const relayOrigin=cadRelayOrigin(import.meta.env.VITE_OVG_CAD_RELAY_ORIGIN);
 const task=async(fn:()=>Promise<void>)=>{if(busy)return;setBusy(true);onBusy(true);setMessage("");try{await fn()}catch(e){if(active.current){if(e instanceof ApiError&&e.status===401)expire();else setMessage(errorText(e))}}finally{if(active.current){setBusy(false);onBusy(false)}}};
 const list=async()=>{const x=await client.request<CadList>(path,{cache:"no-store",signal:abort.current.signal});if(active.current)setDocs(x.items)};
 const history=async(id:string)=>{const x=await client.request<CadList>(path+"/"+id+"/versions",{cache:"no-store",signal:abort.current.signal});if(active.current){setSelected(id);setVersions(x.items)}};
 const lookup=()=>void task(async()=>{if(!linkTarget||!(/^[0-9a-fA-F]{1,32}$/).test(handle.trim()))return;const h=handle.trim().toUpperCase(),x=await client.request<CadLinks>(path+"/"+linkTarget.id+"/versions/"+linkTarget.version+"/links?handle="+encodeURIComponent(h),{cache:"no-store",signal:abort.current.signal});if(active.current){setLinked(x.items.filter(v=>validLink(v)&&v.document_id===linkTarget.id&&v.version===linkTarget.version&&v.handle.toUpperCase()===h));setMessage(x.items.length?"Đã tìm liên kết nguồn.":"Không có đối tượng GIS còn hiệu lực cho handle này.")}});
 const save=(file:File,revision?:CadVersion)=>void task(async()=>{if(file.size>25*1024*1024){setMessage("Tối đa 25 MiB mỗi tệp.");return}await loadCsrfToken(client);if(!active.current)return;const form=new FormData();form.set("file",file);if(revision)form.set("expected_version",String(revision.version));const x=await client.request<CadVersion>(revision?path+"/"+revision.id+"/versions":path,{method:"POST",body:form,signal:abort.current.signal});if(!active.current)return;await list();await history(x.id);if(active.current)setMessage("Đã lưu phiên bản "+x.version+".")});
 const download=(v:CadVersion)=>void task(async()=>{const response=await client.requestRaw(path+"/"+v.id+"/versions/"+v.version+"/download",{cache:"no-store",signal:abort.current.signal});const blob=await response.blob();if(!active.current)return;const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download=v.filename;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)});
 if(georef&&canEdit)return <CadGeorefPanel client={client} path={path} document={georef} canEdit={canEdit} expire={expire} close={()=>{setGeoref(null);onBusy(false)}} onImported={()=>{setGeoref(null);onBusy(false);onImported()}}/>;
 if(editor&&relayOrigin)return <CadEditorPanel client={client} path={path} document={editor.doc} origin={relayOrigin} writable={editor.writable} canEdit={canEdit} expire={expire} close={()=>{setEditor(null);onBusy(false);void task(async()=>{await list();await history(editor.doc.id)})}}/>;
 return <section className="ovg-cad-vault"><header><h2>Kho bản vẽ CAD của dự án</h2><p>DWG / DXF · tệp gốc và lịch sử phiên bản riêng tư. {relayOrigin?"Editor CAD được cấu hình.":"Editor trực tiếp chưa được kích hoạt an toàn."}</p></header>
 <div className="ovg-cad-actions"><button disabled={busy} onClick={()=>void task(async()=>{await list();if(selected)await history(selected)})}>Tải lại</button>{canEdit&&<label>Thêm bản vẽ <input type="file" accept=".dwg,.dxf" disabled={busy} onChange={e=>{const f=e.target.files?.[0];e.target.value="";if(f)save(f)}}/></label>}</div>
 <div className="ovg-cad-columns"><div><h3>Bản vẽ ({docs.length})</h3>{!docs.length&&<p>Chưa có bản vẽ trong dự án.</p>}{docs.map(d=><button className="ovg-cad-doc" aria-pressed={selected===d.id} disabled={busy} key={d.id} onClick={()=>void task(()=>history(d.id))}>{d.filename}<small>v{d.version} · {(d.byte_size/1024).toFixed(1)} KiB</small></button>)}</div><div><h3>Lịch sử phiên bản</h3>{!current&&<p>Chọn bản vẽ để xem lịch sử.</p>}{current&&canEdit&&<label>Lưu phiên bản mới <input type="file" accept={"."+current.format} disabled={busy||current.version>=20} onChange={e=>{const f=e.target.files?.[0];e.target.value="";if(f)save(f,current)}}/></label>}{versions.map(v=><div className="ovg-cad-version" key={v.version}><span>v{v.version} · {v.filename}<small>{new Date(v.created_at).toLocaleString("vi-VN")}</small></span><button disabled={busy||!relayOrigin} title={!relayOrigin?"Cần cấu hình relay CAD đã kiểm duyệt":undefined} onClick={()=>{onBusy(true);setEditor({doc:v,writable:canEdit&&current?.version===v.version&&v.version<20})}}>{canEdit&&current?.version===v.version&&v.version<20?"Mở & sửa":"Xem CAD"}</button>{canEdit&&v.format==="dxf"&&<button disabled={busy} onClick={()=>{onBusy(true);setGeoref(v)}}>Định vị → GIS</button>}{v.format==="dxf"&&<button disabled={busy} onClick={()=>{setLinkTarget(v);setHandle("");setLinked([])}}>Liên kết GIS</button>}<button disabled={busy} onClick={()=>download(v)}>Tải xuống</button></div>)}</div></div>
 {linkTarget&&<section className="ovg-cad-links"><h3>Đối tượng GIS ↔ CAD · v{linkTarget.version}</h3><p>Nhập handle DXF từ bản vẽ đúng phiên bản. Đây là liên kết nguồn, không điều khiển lựa chọn trong CAD editor.</p><label>Handle DXF <input value={handle} maxLength={32} onChange={e=>{setHandle(e.target.value);setLinked([])}}/></label><button disabled={busy||!(/^[0-9a-fA-F]{1,32}$/).test(handle.trim())} onClick={lookup}>Tra liên kết GIS</button>{linked.map(v=><div key={v.source_id+":"+v.feature_id}><span>{v.layer_name} · {v.feature_id} · {v.handle}{!v.original_revision?" · GIS đã thay đổi từ lần nhập":""}</span><button disabled={busy} onClick={()=>{if(!onPickLink(v))setMessage("Đối tượng không có trong dữ liệu GIS đang hiển thị. Mở lớp và tải lại GIS.")}}>Chọn trên GIS</button></div>)}</section>}<p role="status">{busy?"Đang xử lý…":message}</p></section>;
}
type Props={scope:ProjectScope;client:ApiClient;canRead:boolean;canEdit:boolean;blocked:boolean;rows:StudioRow[];rasters:StoredRaster[];camera:()=>MLMap|null;expire:()=>void;onCadImport:()=>void;selectedKey:string;onSelectRow:(key:string)=>void;children:ReactNode};
export function SpatialWorkspace(props:Props){
 const {scope,client,rows}=props;
 const path="/organizations/"+encodeURIComponent(scope.organizationId)+"/projects/"+encodeURIComponent(scope.projectId)+"/cad/documents";
 const [session,setSession]=useState(()=>createSession(scope)),[field,setField]=useState(""),[docs,setDocs]=useState<CadVersion[]>([]),[cadBusy,setCadBusy]=useState(false),[cadPublished,setCadPublished]=useState(false),[picked,setPicked]=useState(""),[focus,setFocus]=useState<CadLink|null>(null),[sourceLinks,setSourceLinks]=useState<{key:string;items:CadLink[]}>({key:"",items:[]});
 const [scene,setScene]=useState(emptyScene),[settings,setSettings]=useState(false),[sceneBusy,setSceneBusy]=useState(false),[sceneInfo,setSceneInfo]=useState(""),[sceneError,setSceneError]=useState("");
 const host=useRef<HTMLDivElement>(null),controller=useRef<ModeCoordinator|null>(null),latest=useRef({props,field,cadBusy,path,scene,sceneBusy}),snapshot=useRef<StudioRow[]|null>(null),engineRef=useRef<SelectableFootprintEngine|null>(null),footprintIds=useRef<Set<string>>(new Set());
 latest.current={props,field,cadBusy,path,scene,sceneBusy};
 const selectedRow=rows.find(r=>r.key===props.selectedKey);
 const selectedSource=selectedRow?.layer.source_id, selectedFeature=selectedRow?.feature.feature_id;
 useEffect(()=>{const abort=new AbortController();setSourceLinks({key:"",items:[]});if(!props.canRead||!selectedSource||!selectedFeature)return;const key=featureKey(selectedSource,selectedFeature);void client.request<CadLinks>(path+"/links?"+new URLSearchParams({source_id:selectedSource,feature_id:selectedFeature}),{cache:"no-store",signal:abort.signal}).then(r=>{if(!abort.signal.aborted)setSourceLinks({key,items:r.items.filter(v=>validLink(v)&&v.source_id===selectedSource&&v.feature_id===selectedFeature)})}).catch(e=>{if(!abort.signal.aborted&&e instanceof ApiError&&e.status===401)latest.current.props.expire()});return()=>abort.abort()},[client,path,props.canRead,selectedSource,selectedFeature]);
 const options=useMemo(()=>{const result=new Map<string,{key:string;label:string;layer:string;attribute:string}>();for(const row of rows){if(row.geometry?.type!=="Polygon"&&row.geometry?.type!=="MultiPolygon")continue;for(const [name,value] of Object.entries(row.feature.attributes)){if(typeof value!=="number"||!Number.isFinite(value)||value<=0||value>10000)continue;const key=JSON.stringify([row.layer.id,name]);result.set(key,{key,label:row.layer.name+" · "+name,layer:row.layer.id,attribute:name})}}return [...result.values()]},[rows]);
 const selectedField=options.find(o=>o.key===field);
 const selectedRef=useRef(selectedField);selectedRef.current=selectedField;
 const capability=(mode:SpatialMode):ModeCapability=>{const p=latest.current.props,o=latest.current.scene;if(mode==="gis")return allowed;if(!p.canRead)return denied("Không có quyền đầy đủ hoặc dữ liệu chưa sẵn sàng.");if(mode==="3d"){if(!p.camera())return denied("Bản đồ chưa sẵn sàng.");if(o.demId&&(!o.ack||!o.label.trim()||!p.rasters.some(r=>r.id===o.demId&&eligibleDem(r))))return denied("Xác nhận DEM, đơn vị mét và mốc cao độ.");if(o.model){if(!p.rows.some(r=>r.key===o.model?.rowKey&&anchorOf(r.geometry)))return denied("Đối tượng gắn mô hình không còn hiển thị.");try{modelNumbers(o.model)}catch{return denied("Kiểm tra thông số mô hình.")}}if(!selectedRef.current&&!o.demId&&!o.model)return denied("Chọn chiều cao polygon, DEM hoặc mô hình GLB/GLTF.");}return allowed};
 useEffect(()=>{
  const c=new ModeCoordinator(scope,{capability,blockers:()=>({dirty:latest.current.props.blocked,saving:latest.current.cadBusy||latest.current.sceneBusy}),onChange:setSession,prepare:async(mode,_scope,signal)=>{
   const p=latest.current.props;
   if(mode==="gis")return noEngine;
   if(mode==="cad"){try{const data=await p.client.request<CadList>(latest.current.path,{signal,cache:"no-store"});return {activate(){setDocs(data.items)},dispose(){setDocs([])}}}catch(e){if(!signal.aborted&&e instanceof ApiError&&e.status===401)p.expire();throw e}}
   const choice=selectedRef.current,m=p.camera(),target=host.current,o=latest.current.scene;if(!m||!target)throw Error("Missing 3D camera");setSceneError("");
   let grid:TerrainGrid|undefined,model:PlacedModel|undefined;
   try{
    if(o.demId){await loadCsrfToken(p.client);if(signal.aborted)throw new DOMException("Aborted","AbortError");const raw=await p.client.request<unknown>("/organizations/"+encodeURIComponent(scope.organizationId)+"/projects/"+encodeURIComponent(scope.projectId)+"/raster/layers/"+encodeURIComponent(o.demId)+"/terrain-preview",{method:"POST",body:JSON.stringify({elevation_unit:"metre",vertical_reference:o.reference,reference_label:o.label.trim(),acknowledge_dem:true}),signal,cache:"no-store"});grid=validateTerrain(raw,o.demId);if(grid.reference_label!==o.label.trim()||grid.vertical_reference!==o.reference)throw Error("Mốc cao độ DEM không khớp.")}
    if(signal.aborted)throw new DOMException("Aborted","AbortError");
    const features:Footprint[]=[];let excluded=0;
    if(choice)for(const r of p.rows){const h=r.feature.attributes[choice.attribute];if(r.layer.id!==choice.layer||(r.geometry?.type!=="Polygon"&&r.geometry?.type!=="MultiPolygon")||typeof h!=="number"||!Number.isFinite(h)||h<=0||h>10000)continue;let base=0;if(grid){const anchor=anchorOf(r.geometry),points=geometryPoints(r.geometry);const height=anchor&&sampleTerrain(grid,...anchor);if(height==null||!points.length||points.some(point=>sampleTerrain(grid!,...point)==null)){excluded++;continue}base=height-grid.render_origin_m}features.push({id:r.key,geometry:r.geometry,baseMetres:base,heightMetres:h})}
    if(o.model){const row=p.rows.find(r=>r.key===o.model?.rowKey),anchor=row&&anchorOf(row.geometry);if(!anchor)throw Error("Không tìm thấy điểm gắn mô hình.");const config=modelNumbers(o.model),ground=grid?sampleTerrain(grid,...anchor):0;if(ground==null)throw Error("Mô hình nằm ngoài DEM hoặc trong vùng NoData.");model={model:o.model.mesh,longitude:anchor[0],latitude:anchor[1],altitude:ground-(grid?.render_origin_m??0)+config.offset,rotationDeg:config.rotationDeg,scale:config.scale}}
    if(!features.length&&!grid&&!model)throw Error("Không có đối tượng 3D hợp lệ.");
    const view=footprint3d(features,"EPSG:4326",grid?"relative-terrain-reference":"relative-flat-reference"),center=m.getCenter().wrap(),camera={longitude:center.lng,latitude:center.lat,zoom:Math.min(22,m.getZoom()),bearing:m.getBearing()};
    if(grid){const [l,b,r,t]=grid.extent;[camera.longitude,camera.latitude]=geographic3857((l+r)/2,(b+t)/2);camera.zoom=Math.max(4,Math.min(19,Math.log2(2*WEB_WORLD/Math.max(r-l,t-b))))}else if(model&&!features.length){camera.longitude=model.longitude;camera.latitude=model.latitude;camera.zoom=Math.max(15,Math.min(18,camera.zoom))}
    const {prepareFootprint3d}=await import("@openvietgis/spatial-workspace/maplibre3d");
    const ids=new Set(features.map(f=>f.id));const engine=await prepareFootprint3d(target,view,camera,signal,id=>{const current=latest.current.props;if(current.canRead&&ids.has(id)&&current.rows===p.rows){setPicked(id);current.onSelectRow(id)}},{terrain:grid,model});
    const info=grid?grid.name+" · "+grid.min_m.toFixed(2)+" → "+grid.max_m.toFixed(2)+" m · "+grid.reference_label+" · gốc hiển thị "+grid.render_origin_m.toFixed(2)+" m · "+grid.width+"×"+grid.height+" · NoData "+grid.nodata_count+ (excluded?" · "+excluded+" polygon ngoài DEM/NoData":""):"mốc phẳng 0 m";
    const description=info+(choice?" · "+choice.label+" · "+features.length+" polygon":"")+(o.model?" · "+o.model.name+" · mô hình phiên":"");
    return {activate(){if(latest.current.props.rows!==p.rows||latest.current.scene!==o||(grid&&!latest.current.props.rasters.some(r=>r.id===grid?.source_id)))throw Error("GIS data changed");snapshot.current=p.rows;engineRef.current=engine;footprintIds.current=ids;const key=latest.current.props.selectedKey;setPicked(ids.has(key)?key:"");setSceneInfo(description);engine.activate();engine.select(key)},dispose(){snapshot.current=null;engineRef.current=null;footprintIds.current=new Set();setSceneInfo("");engine.dispose()}};
   }catch(e){if(!signal.aborted){if(e instanceof ApiError&&e.status===401)p.expire();else setSceneError(e instanceof Error?e.message:"Không mở được 3D nâng cao.")}throw e}
  }});
  controller.current=c;setSession(c.state);return()=>{controller.current=null;c.dispose()};
 },[scope.accountId,scope.organizationId,scope.projectId]);
 useEffect(()=>{controller.current?.revalidate();if(snapshot.current&&snapshot.current!==rows)void controller.current?.switchTo("gis")},[props.canRead,rows,field,props.rasters,scene]);
 useEffect(()=>{if(!props.canRead){setScene(emptyScene());setSettings(false);setSceneBusy(false);setSceneError("")}},[props.canRead]);
 useEffect(()=>{if(cadPublished&&!cadBusy){setCadPublished(false);void controller.current?.switchTo("gis").then(()=>latest.current.props.onCadImport())}},[cadPublished,cadBusy]);
 useEffect(()=>{if(session.active==="gis")requestAnimationFrame(()=>props.camera()?.resize())},[session.active]);
 useEffect(()=>{if(session.active==="3d"){const key=props.selectedKey;engineRef.current?.select(key);setPicked(footprintIds.current.has(key)?key:"")}},[props.selectedKey,session.active]);
 const active=session.active;
 const pickLink=(link:CadLink)=>{const p=latest.current.props;const key=featureKey(link.source_id,link.feature_id);if(!p.canRead||!p.rows.some(r=>r.key===key&&r.layer.id===link.layer_id))return false;p.onSelectRow(key);void controller.current?.switchTo("gis");return true};
 return <div className="ovg-spatial-workspace">
 <div className="ovg-spatial-top"><SpatialModeSwitch session={session} capabilities={{gis:allowed,cad:capability("cad"),"3d":capability("3d")}} onSelect={m=>{if(m==="cad")setFocus(null);void controller.current?.switchTo(m)}}/><label>Chiều cao 3D (m) <select disabled={active!=="gis"||!props.canRead||!!session.pending} value={field} onChange={e=>setField(e.target.value)}><option value="">Chọn trường số</option>{props.canRead&&options.map(o=><option key={o.key} value={o.key}>{o.label}</option>)}</select></label><button disabled={active!=="gis"||!props.canRead||!!session.pending} aria-expanded={settings&&active==="gis"} onClick={()=>setSettings(v=>!v)}>3D nâng cao</button>{active!=="cad"&&props.canRead&&selectedRow&&sourceLinks.key===props.selectedKey&&sourceLinks.items.map(link=><button key={link.document_id+":"+link.version} title={link.original_revision?"Liên kết bản vẽ gốc":"GIS đã thay đổi từ lần nhập"} onClick={()=>{setFocus(link);void controller.current?.switchTo("cad")}}>CAD nguồn · v{link.version} · {link.handle}</button>)}</div>
 {active==="gis"&&props.canRead&&settings&&<Spatial3DSettings options={scene} onChange={setScene} rasters={props.rasters} row={selectedRow} rows={rows} enabled={!session.pending&&!props.blocked} onBusy={setSceneBusy}/>}{active==="gis"&&sceneError&&<p role="alert" className="ovg-3d-error">{sceneError}</p>}
 <div className="ovg-spatial-body"><div style={{display:active==="gis"?"contents":"none"}} inert={active!=="gis"}>{props.children}</div>
 <div className="ovg-alternate-pane" data-ovg-alternate-active={active!=="gis"?active:undefined} style={{display:active!=="gis"&&!props.canRead?"none":undefined,visibility:active==="gis"?"hidden":"visible",pointerEvents:active==="gis"?"none":"auto"}} ref={host}>
 {active==="cad"&&props.canRead&&<CadVault client={client} path={path} initial={docs} canEdit={props.canEdit} onBusy={setCadBusy} expire={props.expire} onImported={()=>setCadPublished(true)} focus={focus} onPickLink={pickLink}/>}
 {active==="3d"&&props.canRead&&<div className="ovg-3d-note">3D · {sceneInfo}{picked&&" · Đối tượng: "+picked}</div>}
 </div></div></div>;
}
