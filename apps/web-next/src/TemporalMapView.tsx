import {useEffect,useMemo,useState,type RefObject} from "react";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {FeatureCollection} from "geojson";
import type {StudioRow} from "./studio-contract";
import type * as ml from "./map-surface";
import type {ComparisonLayer} from "./secondary-map-plan";
import {SecondaryMapView} from "./SecondaryMapView";
import {temporalFields,temporalReadable,temporalPopulation,temporalCollection,temporalWindowPopulation,timestampMs} from "./temporal-plan";
const empty:FeatureCollection={type:"FeatureCollection",features:[]};
const date=(t:number)=>new Date(t).toISOString().split("T")[0]!;
export function TemporalMapView({storageKey,primary,layers,rows,collection,templates,close}:{storageKey:string;primary:RefObject<ml.Map|null>;layers:SpatialLayer[];rows:StudioRow[];collection:FeatureCollection;templates:ml.LayerSpecification[];close:()=>void}){
 const eligible=useMemo(()=>layers.filter(l=>temporalReadable(l)&&temporalFields(l).length>0),[layers]);
 const [id,setId]=useState(""),[field,setField]=useState(""),[index,setIndex]=useState(0),[playing,setPlaying]=useState(false);
 const [custom,setCustom]=useState(false),[endField,setEndField]=useState(""),[start,setStart]=useState(""),[end,setEnd]=useState(""),[minutes,setMinutes]=useState("60");
 const layer=eligible.find(l=>l.id===id),fields=layer?temporalFields(layer):[];
 const result=useMemo(()=>{if(!layer||!field)return {data:null,error:""};try{return {data:temporalPopulation(layer,rows,field),error:""}}catch(e){return {data:null,error:e instanceof Error?e.message:String(e)}}},[layer,rows,field]);
 const windowResult=useMemo(()=>{if(!custom||!layer||!field)return {data:null,error:""};try{
  const a=timestampMs(start),b=timestampMs(end);if(a===null||b===null||!minutes.trim())throw Error("Nhập thời điểm ISO 8601 kèm múi giờ, ví dụ 2026-11-28T00:00:00Z.");
  return {data:temporalWindowPopulation(layer,rows,field,endField,{start:a,end:b,stepMs:Number(minutes)*60000}),error:""};
 }catch(e){return {data:null,error:e instanceof Error?e.message:String(e)}}},[custom,layer,rows,field,endField,start,end,minutes]);
 const data=custom?windowResult.data:result.data;
 const days=custom?windowResult.data?.frames??[]:result.data?.days??[],at=Math.min(index,Math.max(0,days.length-1)),day=days[at];
 const ids=useMemo(()=>custom?windowResult.data?.idsAt(at):day===undefined?undefined:result.data?.byDay.get(day),[custom,windowResult.data,result.data,at,day]);
 useEffect(()=>{setIndex(0);setPlaying(false)},[id,field,endField,custom,start,end,minutes,layer?.source.content_revision,layer?.source.schema_revision]);
 useEffect(()=>{if(!playing||!data||at>=days.length-1){if(playing)setPlaying(false);return}const timer=window.setTimeout(()=>setIndex(at+1),800);return()=>clearTimeout(timer)},[playing,data,at,days.length]);
 const frame=useMemo(()=>ids?temporalCollection(collection,ids):empty,[collection,ids]);
 const mapLayers=useMemo<ComparisonLayer[]>(()=>layer?[{id:layer.id,name:layer.name,min:0,max:24}]:[],[layer]);
 const header=<div className="ovg-temporal-controls">
  <label>Lớp vector <select value={id} onChange={e=>{setId(e.target.value);setField("");setEndField("");setPlaying(false);setIndex(0)}}><option value="">Chọn lớp</option>{eligible.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select></label>
  <label>Trường timestamp <select value={field} disabled={!layer} onChange={e=>{setField(e.target.value);setPlaying(false);setIndex(0)}}><option value="">Chọn trường</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
  <label><input type="checkbox" checked={custom} onChange={e=>setCustom(e.target.checked)}/> Khoảng thời gian tùy chỉnh</label>
  {custom&&<><label>Trường kết thúc (tùy chọn)<select value={endField} disabled={!layer} onChange={e=>setEndField(e.target.value)}><option value="">Sự kiện tức thời</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label><label>Bắt đầu ISO 8601<input value={start} placeholder="2026-11-28T00:00:00Z" onChange={e=>setStart(e.target.value)}/></label><label>Kết thúc (không bao gồm)<input value={end} placeholder="2026-11-29T00:00:00Z" onChange={e=>setEnd(e.target.value)}/></label><label>Bước (phút)<input type="number" min={1} max={527040} step={1} value={minutes} onChange={e=>setMinutes(e.target.value)}/></label><small>Khoảng nửa mở [bắt đầu, kết thúc); tối đa 5.000 khung, kể cả khung trống.</small></>}
  {!eligible.length&&<p role="status">Không có lớp vector được phép đọc với trường timestamp.</p>}{(custom?windowResult.error:result.error)&&<p role="alert">{custom?windowResult.error:result.error}</p>}
  {data&&<><small>{data.total} đối tượng đã xác minh · {data.invalid} NULL hoặc timestamp không hợp lệ.</small>{days.length>0?<><label>{custom?"Khung UTC":"Ngày UTC"} <output>{custom?new Date(day!).toISOString():date(day!)}</output><input type="range" aria-label="Khung thời gian UTC" min={0} max={days.length-1} value={at} onChange={e=>{setPlaying(false);setIndex(Number(e.target.value))}}/></label><div className="ovg-temporal-buttons"><button disabled={at===0} onClick={()=>{setPlaying(false);setIndex(at-1)}}>‹ Trước</button><button disabled={days.length<2} onClick={()=>{if(!playing&&at===days.length-1)setIndex(0);setPlaying(v=>!v)}}>{playing?"Dừng":"Phát"}</button><button disabled={at===days.length-1} onClick={()=>{setPlaying(false);setIndex(at+1)}}>Sau ›</button><span>{at+1}/{days.length}</span></div><small>{frame.features.length}/{ids?.size??0} đối tượng có hình học đang hiển thị trong khung.</small></>:<p>Không có timestamp hợp lệ.</p>}</>}
  <small>UTC, chế độ ngày chỉ hiện ngày có dữ liệu; chế độ tùy chỉnh có khung trống. Lớp tối đa 5.000 đối tượng đã nạp đủ. Không sửa dữ liệu, không lọc bản đồ chính. Mesh dataset và LAS/LAZ chưa được hỗ trợ.</small>
 </div>;
 return <SecondaryMapView storageKey={storageKey} primary={primary} collection={frame} layers={mapLayers} templates={templates} close={close} title="Temporal Controller" headerContent={header}/>;
}
