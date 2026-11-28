import React,{useEffect,useRef,useState} from "react";
import {arcgisFetch,arcgisLayers,asFile,csvToGeoJSON,guessDelim,guessXY,splitCsv,wfsFetch,wfsLayers,DSM_CSV_MAX_BYTES,remoteLimit,type Fc} from "./dsm-sources";

// Spec 058: extra Data Source Manager panes (QGIS: Delimited Text, WFS / OGC API Features, ArcGIS REST).
// Each produces a GeoJSON file handed to the normal import (server inspection + confirmation).
type Pane={onFile:(f:File)=>void;busy:boolean};
const errText=(e:unknown)=>e instanceof TypeError?"Không tải được (máy chủ chặn truy cập từ trình duyệt — CORS — hoặc sai địa chỉ).":e instanceof Error?e.message:String(e);

export function CsvPane({onFile,busy}:Pane){
 const [text,setText]=useState(""),[name,setName]=useState("diem"),[delim,setDelim]=useState(","),[xy,setXy]=useState({x:"",y:""}),[msg,setMsg]=useState("");
 const serial=useRef(0),[reading,setReading]=useState(false),header=React.useMemo(()=>{try{return text?splitCsv(text.replace(/^﻿/,""),delim)[0]??[]:[]}catch{return []}},[text,delim]);
 useEffect(()=>()=>{serial.current++},[]);
 const read=async(f:File)=>{const ticket=++serial.current;setText("");setXy({x:"",y:""});setMsg("");setReading(true);try{if(f.size>DSM_CSV_MAX_BYTES)throw Error("CSV vượt giới hạn 16 MiB.");const t=await f.text();if(ticket!==serial.current)return;const d=guessDelim(t),h=splitCsv(t.replace(/^﻿/,""),d)[0];if(!h)throw Error("Tệp trống.");setName(f.name.replace(/\\.[^.]+$/,""));setText(t);setDelim(d);setXy(guessXY(h));}catch(e){if(ticket===serial.current)setMsg(errText(e))}finally{if(ticket===serial.current)setReading(false)}};
 const go=()=>{try{const {fc,skipped}=csvToGeoJSON(text,{delim,...xy});if(!fc.features.length)throw Error("Không có dòng nào có tọa độ hợp lệ");setMsg(fc.features.length+" điểm"+(skipped?" · bỏ qua "+skipped+" dòng thiếu tọa độ":""));onFile(asFile(fc,name))}catch(e){setMsg(errText(e))}};
 return <div className="dsm-form"><p>Tệp CSV / TXT có cột tọa độ (X/Y hoặc kinh độ/vĩ độ). Excel: Lưu thành CSV trước.</p>
  <label>Tệp<input type="file" accept=".csv,.txt,.tsv" disabled={busy||reading} onChange={e=>{const f=e.target.files?.[0];if(f)void read(f)}}/></label>
  {text&&<><label>Dấu phân cách<select value={delim} onChange={e=>{setDelim(e.target.value);setXy({x:"",y:""});setMsg("")}}><option value=",">Dấu phẩy ,</option><option value=";">Chấm phẩy ;</option><option value={"\t"}>Tab</option><option value="|">Gạch |</option></select></label>
  <label>Cột X (kinh độ / Easting)<select value={xy.x} onChange={e=>setXy(v=>({...v,x:e.target.value}))}><option value="">—</option>{header.map(h=><option key={h} value={h}>{h}</option>)}</select></label>
  <label>Cột Y (vĩ độ / Northing)<select value={xy.y} onChange={e=>setXy(v=>({...v,y:e.target.value}))}><option value="">—</option>{header.map(h=><option key={h} value={h}>{h}</option>)}</select></label>
  <p className="live-note">Hệ tọa độ chọn ở bước kiểm tra tiếp theo (WGS84 hoặc VN-2000).</p>
  <div className="dsm-actions"><button className="primary" disabled={busy||reading||!xy.x||!xy.y||xy.x===xy.y||!header.includes(xy.x)||!header.includes(xy.y)} onClick={go}>Thêm</button></div></>}
  {msg&&<p className="live-note" role="status">{msg}</p>}</div>;
}

function RemotePane({onFile,busy,intro,placeholder,list,fetchLayer}:Pane&{intro:string;placeholder:string;list:(u:string,signal?:AbortSignal)=>Promise<{name:string;title:string}[]>;fetchLayer:(u:string,layer:string,limit:number,signal?:AbortSignal)=>Promise<Fc>}){
 const [url,setUrl]=useState(""),[layers,setLayers]=useState<{name:string;title:string}[]>([]),[layer,setLayer]=useState(""),[limit,setLimit]=useState(5000),[msg,setMsg]=useState(""),[working,setWorking]=useState(false);
 const pending=useRef<AbortController|null>(null),latest=useRef({busy,onFile});latest.current={busy,onFile};
 useEffect(()=>()=>{pending.current?.abort();pending.current=null},[]);
 const current=(ctrl:AbortController)=>pending.current===ctrl&&!ctrl.signal.aborted;
 const run=async(fn:(ctrl:AbortController)=>Promise<void>)=>{if(pending.current||latest.current.busy)return;const ctrl=new AbortController();pending.current=ctrl;setWorking(true);setMsg("");const timeout=window.setTimeout(()=>ctrl.abort(),30000);try{await fn(ctrl)}catch(e){if(pending.current===ctrl)setMsg(ctrl.signal.aborted?"Đã hủy hoặc hết thời gian kết nối.":errText(e))}finally{clearTimeout(timeout);if(pending.current===ctrl){pending.current=null;setWorking(false)}}};
 return <div className="dsm-form"><p>{intro}</p>
  <label>Địa chỉ dịch vụ<input value={url} disabled={busy||working} placeholder={placeholder} onChange={e=>{setUrl(e.target.value);setLayers([]);setLayer("");setMsg("")}}/></label>
  <div className="dsm-actions"><button disabled={busy||working||!url.trim()} onClick={()=>void run(async ctrl=>{setLayers([]);setLayer("");const l=await list(url,ctrl.signal);if(!current(ctrl))return;if(!Array.isArray(l)||!l.length||l.length>1000||l.some(x=>typeof x.name!=="string"||!x.name||x.name.length>2048||typeof x.title!=="string")||new Set(l.map(x=>x.name)).size!==l.length)throw Error("Danh mục lớp không hợp lệ hoặc vượt 1.000 lớp.");setLayers(l);setLayer(l[0]!.name)})}>{working&&!layers.length?"Đang kết nối…":"Kết nối"}</button></div>
  {layers.length>0&&<><label>Lớp<select value={layer} disabled={busy||working} onChange={e=>setLayer(e.target.value)}>{layers.map(l=><option key={l.name} value={l.name}>{l.title||l.name}</option>)}</select></label>
  <label>Tối đa số đối tượng<input type="number" min={1} max={50000} value={limit} disabled={busy||working} onChange={e=>setLimit(Number(e.target.value))}/></label>
  <p className="live-note">Dữ liệu được sao chép vào dự án tại thời điểm thêm (không tự cập nhật theo máy chủ gốc).</p>
  <div className="dsm-actions"><button className="primary" disabled={busy||working||!layer} onClick={()=>void run(async ctrl=>{const nmax=remoteLimit(limit),fc=await fetchLayer(url,layer,nmax,ctrl.signal);if(!current(ctrl)||latest.current.busy)return;const n=fc.features.length;if(!n)throw Error("Lớp không có đối tượng");setMsg(n+" đối tượng"+(n>=nmax?" (đã đạt giới hạn)":""));latest.current.onFile(asFile(fc,layers.find(l=>l.name===layer)?.title||layer))})}>{working?"Đang tải…":"Thêm"}</button></div></>}
  {working&&<button type="button" onClick={()=>pending.current?.abort()}>Hủy kết nối</button>}{msg&&<p className="live-note" role="status">{msg}</p>}</div>;
}
export const WfsPane=(p:Pane)=><RemotePane {...p} intro="Lấy dữ liệu vector từ máy chủ WFS 2.0 (GeoServer, QGIS Server…) hoặc OGC API Features (địa chỉ kết thúc bằng /collections)." placeholder="https://…/geoserver/wfs" list={wfsLayers} fetchLayer={wfsFetch}/>;
export const ArcgisPane=(p:Pane)=><RemotePane {...p} intro="Lấy dữ liệu từ ArcGIS REST: địa chỉ FeatureServer / MapServer, hoặc một lớp cụ thể (…/FeatureServer/0)." placeholder="https://…/arcgis/rest/services/…/FeatureServer" list={arcgisLayers} fetchLayer={(_u,layer,limit,signal)=>arcgisFetch(layer,limit,signal)}/>;
