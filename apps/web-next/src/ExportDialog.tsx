import React,{useMemo,useState} from "react";
import {createPortal} from "react-dom";
import {ApiClient,downloadIntegrationLayerExport,type IntegrationExportFormat,type SpatialLayer} from "@openvietgis/client-sdk";
import {zipFiles} from "./zip";
import {safeName,toCsv,toGeoJson,toKml,type Feat} from "./export-formats";
import type {StudioRow} from "./studio-contract";

// Spec 058: new export (QGIS "Save Features As" + batch): pick layers, format and scope, then download.
// Whole layers go through the server export (validated, project CRS rules); selected / in-view features
// are written in the browser (GeoJSON, KML, CSV in WGS 84).
type Fmt={id:string;ext:string;name:string;note:string;server?:IntegrationExportFormat;adapter?:"qgis"|"autocad"|"postgis"};
const FORMATS:Fmt[]=[
 {id:"gpkg",ext:"GPKG",name:"GeoPackage",note:"QGIS, ArcGIS · khuyên dùng",server:"geopackage",adapter:"qgis"},
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
 const f=FORMATS.find(x=>x.id===fmt)!;
 const counts=useMemo(()=>{const c:Record<string,{all:number;selected:number;view:number}>={};for(const l of layers)c[l.id]={all:l.source.feature_count??0,selected:0,view:0};for(const r of rows){const x=c[r.layer.id];if(!x)continue;if(selection.has(r.key))x.selected++;if(inView(r))x.view++}return c},[layers,rows,selection]);
 const n=(id:string)=>counts[id]?.[scope]??0;
 const serverOnlyWhole=!!f.server&&scope!=="all";
 const chosen=[...picked].filter(id=>layers.some(l=>l.id===id));
 const total=chosen.reduce((a,id)=>a+n(id),0);
 const run=async()=>{setBusy("Đang chuẩn bị…");setDone("");const files:{name:string;blob:Blob}[]=[],errs:string[]=[];
  for(const [i,id] of chosen.entries()){const l=layers.find(x=>x.id===id)!;setBusy("Đang xuất "+(i+1)+"/"+chosen.length+": "+l.name);
   try{if(f.server&&scope==="all"){const r=await downloadIntegrationLayerExport(client,org,pid,f.adapter!,id,f.server);files.push({name:safeName(l.name)+"."+f.ext.toLowerCase(),blob:r.blob})}
    else{const src=rows.filter(r=>r.layer.id===id&&(scope==="all"||(scope==="selected"?selection.has(r.key):inView(r)))),feats:Feat[]=src.map(r=>({geometry:r.geometry as Feat["geometry"],properties:{...r.feature.attributes}}));if(!feats.length)continue;
     const body=fmt==="kml"?toKml(l.name,feats,ft=>label(src[feats.indexOf(ft)]!)):fmt==="csv"?toCsv(feats):toGeoJson(feats);
     files.push({name:safeName(l.name)+"."+(fmt==="kml"?"kml":fmt==="csv"?"csv":"geojson"),blob:new Blob([body],{type:fmt==="kml"?"application/vnd.google-earth.kml+xml":fmt==="csv"?"text/csv;charset=utf-8":"application/geo+json"})})}}
   catch(e){errs.push(l.name+": "+(e instanceof Error?e.message:String(e)))}}
  if(files.length===1)down(files[0]!.blob,files[0]!.name);else if(files.length>1){setBusy("Đang nén ZIP…");down(await zipFiles(files),(safeName(name)||"xuat")+".zip")}
  setBusy("");setDone(files.length?"Đã tải "+files.length+" tệp"+(errs.length?" · lỗi: "+errs.join("; "):""):errs.length?"Lỗi: "+errs.join("; "):"Không có đối tượng trong phạm vi đã chọn")};
 const shown=layers.filter(l=>l.name.toLocaleLowerCase("vi").includes(q.toLocaleLowerCase("vi")));
 return createPortal(<div className="qgis-studio ovg-ws-shade" onMouseDown={e=>{if(e.target===e.currentTarget&&!busy)close()}}><div className="xp" role="dialog" aria-modal="true" aria-label="Xuất dữ liệu" onKeyDown={e=>{if(e.key==="Escape"&&!busy)close()}}>
  <header><div><b>Xuất dữ liệu</b><small>{projectName}</small></div><button type="button" aria-label="Đóng" disabled={!!busy} onClick={close}>×</button></header>
  <div className="xp-body">
   <section className="xp-layers" aria-label="Lớp">
    <div className="xp-col-head"><b>Lớp</b><button type="button" onClick={()=>setPicked(picked.size===layers.length?new Set():new Set(layers.map(l=>l.id)))}>{picked.size===layers.length?"Bỏ chọn":"Chọn tất cả"}</button></div>
    <input className="xp-search" value={q} onChange={e=>setQ(e.target.value)} placeholder="Tìm lớp…" aria-label="Tìm lớp"/>
    <div className="xp-layer-list">{shown.map(l=><label key={l.id} className={picked.has(l.id)?"on":""}><input type="checkbox" checked={picked.has(l.id)} onChange={()=>setPicked(p=>{const s=new Set(p);if(s.has(l.id))s.delete(l.id);else s.add(l.id);return s})}/><span className="xp-gi">{geomIcon(l)}</span><span className="xp-ln"><b>{l.name}</b><small>{(scope==="all"?counts[l.id]?.all:n(l.id))?.toLocaleString("vi-VN")} đối tượng{scope!=="all"?(scope==="selected"?" đang chọn":" trong khung nhìn"):""}</small></span></label>)}</div>
   </section>
   <section className="xp-opts">
    <h4>Định dạng</h4>
    <div className="xp-fmt-grid" role="radiogroup" aria-label="Định dạng">{FORMATS.map(x=><button type="button" role="radio" aria-checked={fmt===x.id} key={x.id} className={"xp-fmt"+(fmt===x.id?" on":"")} onClick={()=>setFmt(x.id)}><span className="xp-ext">{x.ext}</span><b>{x.name}</b><small>{x.note}</small></button>)}</div>
    <h4>Phạm vi</h4>
    <div className="xp-scope" role="radiogroup" aria-label="Phạm vi">{([["all","Toàn bộ lớp"],["selected","Đối tượng đang chọn ("+selection.size.toLocaleString("vi-VN")+")"],["view","Trong khung nhìn"]] as [Scope,string][]).map(([k,t])=><button type="button" role="radio" aria-checked={scope===k} key={k} disabled={k==="selected"&&!selection.size} className={scope===k?"on":""} onClick={()=>setScope(k)}>{t}</button>)}</div>
    {serverOnlyWhole&&<p className="xp-note">{f.name} xuất cả lớp qua máy chủ. Với phạm vi này, chọn GeoJSON, Google Earth hoặc Excel/CSV.</p>}
    <h4>Tên tệp</h4>
    <div className="xp-name"><input value={name} onChange={e=>setName(e.target.value)} aria-label="Tên tệp"/><span>{chosen.length>1?".zip":"."+(f.ext.toLowerCase())}</span></div>
    <p className="xp-note">{f.server&&scope==="all"?"Hệ tọa độ theo lớp gốc (DXF tự chuyển sang UTM để đo đúng mét).":"Tọa độ WGS 84 (EPSG:4326)."}</p>
   </section>
  </div>
  <footer><span role="status">{busy||done||(chosen.length?chosen.length+" lớp · "+total.toLocaleString("vi-VN")+" đối tượng · "+f.ext:"Chọn ít nhất một lớp")}</span><button type="button" onClick={close} disabled={!!busy}>Đóng</button><button type="button" className="primary" disabled={!!busy||!chosen.length||serverOnlyWhole||(scope!=="all"&&total===0)} onClick={()=>void run()}>{busy?"Đang xuất…":"⬇ Xuất "+(chosen.length>1?chosen.length+" lớp":"")}</button></footer>
 </div></div>,document.body);
}
