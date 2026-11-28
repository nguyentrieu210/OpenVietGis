import React,{useMemo,useState,useRef,useEffect} from "react";
import {createPortal} from "react-dom";
import {ApiClient,downloadIntegrationLayerExport,previewExport,confirmExport,getDerivedArtifact,downloadDerivedArtifact,loadCsrfToken,type ExportPreviewInput,type IntegrationExportFormat,type SpatialLayer} from "@openvietgis/client-sdk";
import {validateBinaryPreview,validateBinaryArtifact,validateBinaryBytes,type BinaryPlan} from "./binary-export";
import {verifyExport,safeTransferName} from "./transfer-verification";
import {zipFiles} from "./zip";
import {safeName,toCsv,toGeoJson,toKml,type Feat} from "./export-formats";
import type {StudioRow} from "./studio-contract";
import {extendedNativeCrs} from "./workbenches/features/platform/processing-request";
import {useDesktopDialog} from "./studio-ui";

// Spec 058: new export (QGIS "Save Features As" + batch): pick layers, format and scope, then download.
// Whole layers go through the server export (validated, project CRS rules); selected / in-view features
// are written in the browser (GeoJSON, KML, CSV in WGS 84).
type Fmt={id:string;ext:string;name:string;note:string;transfer?:"geopackage"|"shapefile_zip";server?:IntegrationExportFormat;adapter?:"qgis"|"autocad"|"postgis"};
const FORMATS:Fmt[]=[
 {id:"gpkg",ext:"GPKG",name:"GeoPackage",note:"QGIS, ArcGIS · khuyên dùng",transfer:"geopackage"},
 {id:"shp",ext:"ZIP",name:"Shapefile",note:"ESRI Shapefile · DBF có giới hạn",transfer:"shapefile_zip"},
 {id:"geojson",ext:"GeoJSON",name:"GeoJSON",note:"Web, lập trình"},
 {id:"kml",ext:"KML",name:"Google Earth",note:"Mở trong Google Earth Pro"},
 {id:"csv",ext:"CSV",name:"Excel / CSV",note:"Bảng thuộc tính + cột WKT"},
 {id:"dxf",ext:"DXF",name:"AutoCAD",note:"Bản vẽ CAD (UTM)",server:"dxf",adapter:"autocad"},
 {id:"sql",ext:"SQL",name:"PostGIS",note:"Câu lệnh nạp CSDL",server:"postgis_sql",adapter:"postgis"},
];
type Scope="all"|"selected"|"view";
const geomIcon=(l:SpatialLayer)=>{const f=l.source.schema?.geometry_family;return f==="Point"?"●":f==="LineString"?"╱":f==="Polygon"?"▰":"◇"};
const down=(blob:Blob,name:string)=>{const a=document.createElement("a");a.href=URL.createObjectURL(blob);a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(a.href),4000)};
export function ExportDialog({client,org,pid,projectName,layers,rows,selection,inView,initial,label,close}:{client:ApiClient;org:string;pid:string;projectName:string;layers:SpatialLayer[];rows:StudioRow[];selection:Set<string>;inView:(r:StudioRow)=>boolean;initial?:string[];label:(r:StudioRow)=>string;close:()=>void}){
 const [picked,setPicked]=useState<Set<string>>(()=>new Set(initial?.length?initial:layers.filter(l=>rows.some(r=>r.layer.id===l.id&&selection.has(r.key))).map(l=>l.id)));
 const [fmt,setFmt]=useState("gpkg"),[scope,setScope]=useState<Scope>(()=>selection.size?"selected":"all"),[q,setQ]=useState(""),[name,setName]=useState(()=>safeName(projectName)),[busy,setBusy]=useState(""),[done,setDone]=useState("");
 const [plans,setPlans]=useState<BinaryPlan[]>([]),[approved,setApproved]=useState<Set<string>>(new Set());
 const mounted=useRef(true),running=useRef(false);useEffect(()=>{mounted.current=true;return()=>{mounted.current=false}},[]);
 const canRead=(l:SpatialLayer)=>l.lifecycle==="active"&&l.capabilities.some(c=>c.name==="export"&&c.available&&c.actor_authorized);
 const matches=(l:SpatialLayer,r:StudioRow)=>r.layer.id===l.id&&r.layer.source_id===l.source_id&&r.layer.source.content_revision===l.source.content_revision&&r.layer.source.schema_revision===l.source.schema_revision;
 const current=useRef({layers,org,pid});current.current={layers,org,pid};
 const f=FORMATS.find(x=>x.id===fmt)!;
 const counts=useMemo(()=>{const c:Record<string,{all:number;selected:number;view:number}>={};for(const l of layers)c[l.id]={all:l.source.feature_count??0,selected:0,view:0};for(const r of rows){const x=c[r.layer.id];if(!x)continue;if(selection.has(r.key))x.selected++;if(inView(r))x.view++}return c},[layers,rows,selection]);
 const n=(id:string)=>counts[id]?.[scope]??0;
 const serverOnlyWhole=!!(f.server||f.transfer)&&scope!=="all";
 const chosen=[...picked].filter(id=>layers.some(l=>l.id===id&&canRead(l)));
 const nativeOnly=chosen.some(id=>extendedNativeCrs(layers.find(l=>l.id===id)?.source.stored_crs?.code));
 const nativeBlocked=nativeOnly&&(!f.transfer||scope!=="all");
 const serverWhole=scope==="all"&&(!!f.server||!!f.transfer||fmt==="geojson");
 const incomplete=scope==="all"&&!serverWhole&&chosen.some(id=>{const l=layers.find(l=>l.id===id)!;return l.source.feature_count===null||new Set(rows.filter(r=>matches(l,r)).map(r=>r.feature.feature_id)).size!==l.source.feature_count});
 const missingSelected=scope==="selected"&&[...selection].some(key=>!rows.some(r=>r.key===key&&layers.some(l=>matches(l,r)&&canRead(l))));
 const total=chosen.reduce((a,id)=>a+n(id),0);
 const planKey=JSON.stringify([org,pid,fmt,scope,chosen.map(id=>{const l=layers.find(x=>x.id===id)!;return [id,l.source_id,l.source.content_revision,l.source.schema_revision,l.source.stored_crs?.code]})]);
 useEffect(()=>{setPlans([]);setApproved(new Set())},[planKey]);
 const planReady=!!f.transfer&&plans.length===chosen.length&&chosen.length>0&&plans.every((p,i)=>{const l=layers.find(x=>x.id===chosen[i]);return !!l&&p.input.layer_id===l.id&&p.input.content_revision===l.source.content_revision&&p.input.schema_revision===l.source.schema_revision&&p.input.output_format===f.transfer&&p.input.output_crs===l.source.stored_crs?.code});
 const required=plans.flatMap(p=>p.preview.disclosures.filter(d=>d.requires_approval).map(d=>({key:p.input.layer_id+":"+d.code,layer:p.input.layer_id,disclosure:d})));
 const prepare=async()=>{if(!f.transfer||running.current||scope!=="all"||nativeBlocked)return;running.current=true;setBusy("Đang kiểm tra chuyển đổi…");setDone("");setApproved(new Set());setPlans([]);
  try{await loadCsrfToken(client);const result:BinaryPlan[]=[];for(const id of chosen){const l=layers.find(x=>x.id===id)!;const crs=l.source.stored_crs?.code;if(!crs)throw Error("Thiếu CRS nguồn");const input:ExportPreviewInput={layer_id:id,content_revision:l.source.content_revision,schema_revision:l.source.schema_revision,output_format:f.transfer,output_crs:crs};const p=validateBinaryPreview(input,l.source_id,l.source.feature_count,await previewExport(client,org,pid,input));result.push({input,preview:p,op:crypto.randomUUID()})}if(mounted.current)setPlans(result)}
  catch(e){if(mounted.current)setDone(String(e))}finally{running.current=false;if(mounted.current)setBusy("")}
 };
 const run=async()=>{if(running.current||nativeBlocked||incomplete||missingSelected||serverOnlyWhole||f.transfer&&(!planReady||required.some(d=>!approved.has(d.key))))return;running.current=true;setBusy("Đang chuẩn bị…");setDone("");const files:{name:string;blob:Blob}[]=[],errs:string[]=[],names=new Set<string>();let totalBytes=0;
  const pinned=(l:SpatialLayer)=>mounted.current&&current.current.org===org&&current.current.pid===pid&&current.current.layers.some(x=>x.id===l.id&&x.source_id===l.source_id&&x.source.content_revision===l.source.content_revision&&x.source.schema_revision===l.source.schema_revision&&canRead(x));
  try{
  for(const [i,id] of chosen.entries()){const l=layers.find(x=>x.id===id)!;if(!pinned(l))throw Error("Phạm vi xuất đã đổi.");setBusy("Đang xuất "+(i+1)+"/"+chosen.length+": "+l.name);
   try{let blob:Blob;if(f.transfer){const p=plans.find(x=>x.input.layer_id===id)!;await loadCsrfToken(client);const j=await confirmExport(client,org,pid,{...p.input,operation_id:p.op,preview_fingerprint:p.preview.preview_fingerprint,approved_disclosures:p.preview.disclosures.filter(d=>d.requires_approval&&approved.has(id+":"+d.code)).map(d=>d.code)});if(!j.artifact_id)throw Error("Không có tệp đã xác nhận");const a=validateBinaryArtifact(p,org,pid,j,await getDerivedArtifact(client,org,pid,j.artifact_id));blob=await validateBinaryBytes(a,await downloadDerivedArtifact(client,org,pid,a.id))}
    else if(serverWhole){const r=await verifyExport({export_permitted:canRead(l),content_revision:l.source.content_revision,schema_revision:l.source.schema_revision},await downloadIntegrationLayerExport(client,org,pid,f.adapter??"qgis",id,f.server??"geojson"));blob=r.blob}
    else{const src=rows.filter(r=>matches(l,r)&&(scope==="all"||(scope==="selected"?selection.has(r.key):inView(r)))),feats:Feat[]=src.map(r=>({geometry:r.geometry as Feat["geometry"],properties:{...r.feature.attributes}}));if(!feats.length)continue;
     const body=fmt==="kml"?toKml(l.name,feats,ft=>label(src[feats.indexOf(ft)]!)):fmt==="csv"?toCsv(feats):toGeoJson(feats);
     blob=new Blob([body],{type:fmt==="kml"?"application/vnd.google-earth.kml+xml":fmt==="csv"?"text/csv;charset=utf-8":"application/geo+json"})}
    if(!pinned(l))throw Error("Layer đã đổi trong khi xuất.");if(totalBytes+blob.size>128*1024*1024)throw Error("Vượt giới hạn 128 MiB mỗi batch.");totalBytes+=blob.size;
    files.push({name:safeTransferName((chosen.length===1?safeName(name):safeName(l.name))+"."+f.ext.toLowerCase(),names),blob})}
   catch(e){errs.push(l.name+": "+(e instanceof Error?e.message:String(e)))}}
  if(!mounted.current)return;if(!chosen.every(id=>pinned(layers.find(x=>x.id===id)!)))throw Error("Phạm vi xuất đã đổi, không tải tệp cũ.");
  if(files.length===1)down(files[0]!.blob,files[0]!.name);else if(files.length>1){setBusy("Đang nén ZIP…");const zip=await zipFiles(files);if(!chosen.every(id=>pinned(layers.find(x=>x.id===id)!)))throw Error("Phạm vi xuất đã đổi.");down(zip,(safeName(name)||"xuat")+".zip")}
  setDone(files.length?"Đã tải "+files.length+" tệp"+(errs.length?" · lỗi: "+errs.join("; "):""):errs.length?"Lỗi: "+errs.join("; "):"Không có đối tượng trong phạm vi đã chọn")
  }catch(e){if(mounted.current)setDone(String(e))}finally{running.current=false;if(mounted.current)setBusy("")}};

 const panel=useRef<HTMLDivElement>(null);
 const requestClose=()=>{if(!running.current&&!busy)close()};
 useDesktopDialog(panel,requestClose);
 const shown=layers.filter(l=>canRead(l)&&l.name.toLocaleLowerCase("vi").includes(q.toLocaleLowerCase("vi")));
 return createPortal(<div className="qgis-studio ovg-ws-shade" onMouseDown={e=>{if(e.target===e.currentTarget)requestClose()}}><div ref={panel} tabIndex={-1} className="xp" role="dialog" aria-modal="true" aria-label="Xuất dữ liệu" aria-busy={!!busy}>
  <header><div><b>Xuất dữ liệu</b><small>{projectName}</small></div><button type="button" aria-label="Đóng" disabled={!!busy} onClick={requestClose}>×</button></header>
  <div className="xp-body">
   <section inert={!!busy} className="xp-layers" aria-label="Lớp">
    <div className="xp-col-head"><b>Lớp</b><button type="button" onClick={()=>setPicked(picked.size===layers.length?new Set():new Set(layers.filter(canRead).map(l=>l.id)))}>{picked.size===layers.length?"Bỏ chọn":"Chọn tất cả"}</button></div>
    <input className="xp-search" value={q} onChange={e=>setQ(e.target.value)} placeholder="Tìm lớp…" aria-label="Tìm lớp"/>
    <div className="xp-layer-list">{shown.map(l=><label key={l.id} className={picked.has(l.id)?"on":""}><input type="checkbox" checked={picked.has(l.id)} onChange={()=>setPicked(p=>{const s=new Set(p);if(s.has(l.id))s.delete(l.id);else s.add(l.id);return s})}/><span className="xp-gi">{geomIcon(l)}</span><span className="xp-ln"><b>{l.name}</b><small>{(scope==="all"?counts[l.id]?.all:n(l.id))?.toLocaleString("vi-VN")} đối tượng{scope!=="all"?(scope==="selected"?" đang chọn":" trong khung nhìn"):""}</small></span></label>)}</div>
   </section>
   <section inert={!!busy} className="xp-opts">
    <h4>Định dạng</h4>
    <div className="xp-fmt-grid" role="radiogroup" aria-label="Định dạng">{FORMATS.map(x=><button type="button" role="radio" aria-checked={fmt===x.id} key={x.id} className={"xp-fmt"+(fmt===x.id?" on":"")} onClick={()=>setFmt(x.id)}><span className="xp-ext">{x.ext}</span><b>{x.name}</b><small>{x.note}</small></button>)}</div>
    <h4>Phạm vi</h4>
    <div className="xp-scope" role="radiogroup" aria-label="Phạm vi">{([["all","Toàn bộ lớp"],["selected","Đối tượng đang chọn ("+selection.size.toLocaleString("vi-VN")+")"],["view","Trong khung nhìn"]] as [Scope,string][]).map(([k,t])=><button type="button" role="radio" aria-checked={scope===k} key={k} disabled={k==="selected"&&!selection.size} className={scope===k?"on":""} onClick={()=>setScope(k)}>{t}</button>)}</div>
    {nativeBlocked&&<p role="alert">Lớp VN-2000 mở rộng chỉ xuất toàn lớp GeoPackage hoặc Shapefile theo CRS gốc. Reproject trước nếu cần định dạng WGS84.</p>}{incomplete&&<p role="alert">Lớp chưa nạp đầy đủ. Xuất toàn lớp CSV/KML bị chặn; chọn GeoJSON hoặc GeoPackage qua máy chủ.</p>}{missingSelected&&<p role="alert">Vùng chọn chưa nạp đầy đủ. Mở đối tượng còn thiếu trước khi xuất.</p>}{scope==="view"&&<p className="xp-note">Chỉ xuất đối tượng đã nạp có ít nhất một đỉnh trong khung nhìn; không phải truy vấn giao cắt toàn nguồn.</p>}{serverOnlyWhole&&<p className="xp-note">{f.name} xuất cả lớp qua máy chủ. Với phạm vi này, chọn GeoJSON, Google Earth hoặc Excel/CSV.</p>}
    {f.transfer&&scope==="all"&&<section aria-label="Kiểm tra chuyển đổi">{planReady?<>{plans.map(p=><div key={p.input.layer_id}><b>{layers.find(l=>l.id===p.input.layer_id)?.name}</b><small> · EPSG:{p.input.output_crs} · {p.preview.feature_count} đối tượng</small>{p.preview.field_mappings.filter(m=>m.output_name!==m.field_id).map(m=><p key={m.field_id}>{m.field_id} → {m.output_name}</p>)}{p.preview.disclosures.filter(d=>!d.requires_approval).map(d=><p key={d.code}>{d.message}</p>)}</div>)}{required.map(d=><label key={d.key}><input type="checkbox" checked={approved.has(d.key)} onChange={e=>setApproved(old=>{const next=new Set(old);if(e.target.checked)next.add(d.key);else next.delete(d.key);return next})}/>{layers.find(l=>l.id===d.layer)?.name}: {d.disclosure.message}</label>)}{!required.length&&<p>Không có chuyển đổi cần phê duyệt.</p>}</>:<p>Kiểm tra các trường và khả năng mất dữ liệu trước khi xuất.</p>}</section>}
    <h4>Tên tệp</h4>
    <div className="xp-name"><input value={name} onChange={e=>setName(e.target.value)} aria-label="Tên tệp"/><span>{chosen.length>1?".zip":"."+(f.ext.toLowerCase())}</span></div>
    <p className="xp-note">{serverWhole?"Hệ tọa độ theo định dạng (GeoJSON WGS84; các định dạng khác theo lớp gốc (DXF tự chuyển sang UTM khi cần).":"Tọa độ WGS 84 (EPSG:4326)."}</p>
   </section>
  </div>
  <footer><span role="status">{busy||done||(chosen.length?chosen.length+" lớp · "+total.toLocaleString("vi-VN")+" đối tượng · "+f.ext:"Chọn ít nhất một lớp")}</span><button type="button" onClick={requestClose} disabled={!!busy}>Đóng</button><button type="button" className="primary" disabled={!!busy||!chosen.length||nativeBlocked||serverOnlyWhole||incomplete||missingSelected||(scope!=="all"&&total===0)||planReady&&required.some(d=>!approved.has(d.key))} onClick={()=>void (f.transfer&&!planReady?prepare():run())}>{busy?"Đang xuất…":f.transfer&&!planReady?"Kiểm tra trước khi xuất":"⬇ Xuất "+(chosen.length>1?chosen.length+" lớp":"")}</button></footer>
 </div></div>,document.body);
}
