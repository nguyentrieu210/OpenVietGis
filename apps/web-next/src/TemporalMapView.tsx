import {useEffect,useMemo,useState,type RefObject} from "react";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {FeatureCollection} from "geojson";
import type {StudioRow} from "./studio-contract";
import type * as ml from "./map-surface";
import type {ComparisonLayer} from "./secondary-map-plan";
import {SecondaryMapView} from "./SecondaryMapView";
import {temporalFields,temporalReadable,temporalPopulation,temporalCollection} from "./temporal-plan";
const empty:FeatureCollection={type:"FeatureCollection",features:[]};
const date=(t:number)=>new Date(t).toISOString().split("T")[0]!;
export function TemporalMapView({storageKey,primary,layers,rows,collection,templates,close}:{storageKey:string;primary:RefObject<ml.Map|null>;layers:SpatialLayer[];rows:StudioRow[];collection:FeatureCollection;templates:ml.LayerSpecification[];close:()=>void}){
 const eligible=useMemo(()=>layers.filter(l=>temporalReadable(l)&&temporalFields(l).length>0),[layers]);
 const [id,setId]=useState(""),[field,setField]=useState(""),[index,setIndex]=useState(0),[playing,setPlaying]=useState(false);
 const layer=eligible.find(l=>l.id===id),fields=layer?temporalFields(layer):[];
 const result=useMemo(()=>{if(!layer||!field)return {data:null,error:""};try{return {data:temporalPopulation(layer,rows,field),error:""}}catch(e){return {data:null,error:e instanceof Error?e.message:String(e)}}},[layer,rows,field]);
 const data=result.data,days=data?.days??[],at=Math.min(index,Math.max(0,days.length-1)),day=days[at];
 useEffect(()=>{setIndex(0);setPlaying(false)},[id,field,layer?.source.content_revision,layer?.source.schema_revision]);
 useEffect(()=>{if(!playing||!data||at>=data.days.length-1){if(playing)setPlaying(false);return}const timer=window.setTimeout(()=>setIndex(at+1),800);return()=>clearTimeout(timer)},[playing,data,at]);
 const frame=useMemo(()=>data&&day!==undefined?temporalCollection(collection,data.byDay.get(day)??new Set<string>()):empty,[collection,data,day]);
 const mapLayers=useMemo<ComparisonLayer[]>(()=>layer?[{id:layer.id,name:layer.name,min:0,max:24}]:[],[layer]);
 const header=<div className="ovg-temporal-controls">
  <label>Lớp vector <select value={id} onChange={e=>{setId(e.target.value);setField("");setPlaying(false);setIndex(0)}}><option value="">Chọn lớp</option>{eligible.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select></label>
  <label>Trường timestamp <select value={field} disabled={!layer} onChange={e=>{setField(e.target.value);setPlaying(false);setIndex(0)}}><option value="">Chọn trường</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
  {!eligible.length&&<p role="status">Không có lớp vector được phép đọc với trường timestamp.</p>}{result.error&&<p role="alert">{result.error}</p>}
  {data&&<><small>{data.total} đối tượng đã xác minh · {data.invalid} NULL hoặc timestamp không hợp lệ.</small>{days.length>0?<><label>Ngày UTC <output>{date(day!)}</output><input type="range" aria-label="Khung thời gian UTC" min={0} max={days.length-1} value={at} onChange={e=>{setPlaying(false);setIndex(Number(e.target.value))}}/></label><div className="ovg-temporal-buttons"><button disabled={at===0} onClick={()=>{setPlaying(false);setIndex(at-1)}}>‹ Trước</button><button disabled={days.length<2} onClick={()=>{if(!playing&&at===days.length-1)setIndex(0);setPlaying(v=>!v)}}>{playing?"Dừng":"Phát"}</button><button disabled={at===days.length-1} onClick={()=>{setPlaying(false);setIndex(at+1)}}>Sau ›</button><span>{at+1}/{days.length}</span></div><small>{frame.features.length}/{data.byDay.get(day!)?.size??0} đối tượng có hình học đang hiển thị trong ngày.</small></>:<p>Không có timestamp hợp lệ.</p>}</>}
  <small>Chỉ các ngày có dữ liệu, UTC, lớp tối đa 5.000 đối tượng đã nạp đủ. Không sửa dữ liệu, không lọc bản đồ chính. Mesh dataset và LAS/LAZ chưa được hỗ trợ.</small>
 </div>;
 return <SecondaryMapView storageKey={storageKey} primary={primary} collection={frame} layers={mapLayers} templates={templates} close={close} title="Temporal Controller" headerContent={header}/>;
}
