import React,{useState} from "react";
import {arcgisFetch,arcgisLayers,asFile,csvToGeoJSON,guessDelim,guessXY,splitCsv,wfsFetch,wfsLayers,type Fc} from "./dsm-sources";

// Spec 058: extra Data Source Manager panes (QGIS: Delimited Text, WFS / OGC API Features, ArcGIS REST).
// Each produces a GeoJSON file handed to the normal import (server inspection + confirmation).
type Pane={onFile:(f:File)=>void;busy:boolean};
const errText=(e:unknown)=>e instanceof TypeError?"Không tải được (máy chủ chặn truy cập từ trình duyệt — CORS — hoặc sai địa chỉ).":e instanceof Error?e.message:String(e);

export function CsvPane({onFile,busy}:Pane){
 const [text,setText]=useState(""),[name,setName]=useState("diem"),[delim,setDelim]=useState(","),[xy,setXy]=useState({x:"",y:""}),[msg,setMsg]=useState("");
 const header=text?splitCsv(text.replace(/^﻿/,"").split(/\r?\n/,1)[0],delim)[0]??[]:[];
 const load=(t:string)=>{const d=guessDelim(t);setText(t);setDelim(d);setXy(guessXY(splitCsv(t.replace(/^﻿/,"").split(/\r?\n/,1)[0],d)[0]??[]));setMsg("")};
 const go=()=>{try{const {fc,skipped}=csvToGeoJSON(text,{delim,...xy});if(!fc.features.length)throw Error("Không có dòng nào có tọa độ hợp lệ");setMsg(fc.features.length+" điểm"+(skipped?" · bỏ qua "+skipped+" dòng thiếu tọa độ":""));onFile(asFile(fc,name))}catch(e){setMsg(errText(e))}};
 return <div className="dsm-form"><p>Tệp CSV / TXT có cột tọa độ (X/Y hoặc kinh độ/vĩ độ). Excel: Lưu thành CSV trước.</p>
  <label>Tệp<input type="file" accept=".csv,.txt,.tsv" disabled={busy} onChange={e=>{const f=e.target.files?.[0];if(f){setName(f.name.replace(/\.[^.]+$/,""));void f.text().then(load)}}}/></label>
  {text&&<><label>Dấu phân cách<select value={delim} onChange={e=>setDelim(e.target.value)}><option value=",">Dấu phẩy ,</option><option value=";">Chấm phẩy ;</option><option value={"\t"}>Tab</option><option value="|">Gạch |</option></select></label>
  <label>Cột X (kinh độ / Easting)<select value={xy.x} onChange={e=>setXy(v=>({...v,x:e.target.value}))}><option value="">—</option>{header.map(h=><option key={h} value={h}>{h}</option>)}</select></label>
  <label>Cột Y (vĩ độ / Northing)<select value={xy.y} onChange={e=>setXy(v=>({...v,y:e.target.value}))}><option value="">—</option>{header.map(h=><option key={h} value={h}>{h}</option>)}</select></label>
  <p className="live-note">Hệ tọa độ chọn ở bước kiểm tra tiếp theo (WGS84 hoặc VN-2000).</p>
  <div className="dsm-actions"><button className="primary" disabled={busy||!xy.x||!xy.y} onClick={go}>Thêm</button></div></>}
  {msg&&<p className="live-note" role="status">{msg}</p>}</div>;
}

function RemotePane({onFile,busy,intro,placeholder,list,fetchLayer}:Pane&{intro:string;placeholder:string;list:(u:string)=>Promise<{name:string;title:string}[]>;fetchLayer:(u:string,layer:string,limit:number)=>Promise<Fc>}){
 const [url,setUrl]=useState(""),[layers,setLayers]=useState<{name:string;title:string}[]>([]),[layer,setLayer]=useState(""),[limit,setLimit]=useState(5000),[msg,setMsg]=useState(""),[working,setWorking]=useState(false);
 const run=async(fn:()=>Promise<void>)=>{setWorking(true);setMsg("");try{await fn()}catch(e){setMsg(errText(e))}finally{setWorking(false)}};
 return <div className="dsm-form"><p>{intro}</p>
  <label>Địa chỉ dịch vụ<input value={url} placeholder={placeholder} onChange={e=>{setUrl(e.target.value);setLayers([])}}/></label>
  <div className="dsm-actions"><button disabled={busy||working||!url.trim()} onClick={()=>void run(async()=>{const l=await list(url);if(!l.length)throw Error("Không thấy lớp nào");setLayers(l);setLayer(l[0].name)})}>{working&&!layers.length?"Đang kết nối…":"Kết nối"}</button></div>
  {layers.length>0&&<><label>Lớp<select value={layer} onChange={e=>setLayer(e.target.value)}>{layers.map(l=><option key={l.name} value={l.name}>{l.title||l.name}</option>)}</select></label>
  <label>Tối đa số đối tượng<input type="number" min={1} max={50000} value={limit} onChange={e=>setLimit(Number(e.target.value)||1)}/></label>
  <p className="live-note">Dữ liệu được sao chép vào dự án tại thời điểm thêm (không tự cập nhật theo máy chủ gốc).</p>
  <div className="dsm-actions"><button className="primary" disabled={busy||working||!layer} onClick={()=>void run(async()=>{const fc=await fetchLayer(url,layer,limit);const n=fc.features?.length??0;if(!n)throw Error("Lớp không có đối tượng");setMsg(n+" đối tượng"+(n>=limit?" (đã đạt giới hạn)":""));onFile(asFile(fc,layers.find(l=>l.name===layer)?.title||layer))})}>{working?"Đang tải…":"Thêm"}</button></div></>}
  {msg&&<p className="live-note" role="status">{msg}</p>}</div>;
}
export const WfsPane=(p:Pane)=><RemotePane {...p} intro="Lấy dữ liệu vector từ máy chủ WFS 2.0 (GeoServer, QGIS Server…) hoặc OGC API Features (địa chỉ kết thúc bằng /collections)." placeholder="https://…/geoserver/wfs" list={wfsLayers} fetchLayer={wfsFetch}/>;
export const ArcgisPane=(p:Pane)=><RemotePane {...p} intro="Lấy dữ liệu từ ArcGIS REST: địa chỉ FeatureServer / MapServer, hoặc một lớp cụ thể (…/FeatureServer/0)." placeholder="https://…/arcgis/rest/services/…/FeatureServer" list={arcgisLayers} fetchLayer={(_u,layer,limit)=>arcgisFetch(layer,limit)}/>;
