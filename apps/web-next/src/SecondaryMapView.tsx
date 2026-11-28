import {useEffect,useMemo,useRef,useState} from "react";
import type {RefObject,ReactNode} from "react";
import type {FeatureCollection} from "geojson";
import * as ml from "./map-surface";
import {readCamera,type CameraResume} from "./map-render-plan";
import {comparisonPrefs,comparisonCollection,moveComparisonLayer,type ComparisonLayer,type ComparisonPrefs} from "./secondary-map-plan";
import {readPreference,writePreference} from "./spatial-resume";
import {rasterPaint,defaultRasterDisplay} from "./raster-contract";
const empty:FeatureCollection={type:"FeatureCollection",features:[]};
const capture=(m:ml.Map|null):CameraResume|null=>{if(!m)return null;const c=m.getCenter().wrap(),bearing=((m.getBearing()+180)%360+360)%360-180;return readCamera({center:[c.lng,c.lat],zoom:m.getZoom(),bearing,pitch:m.getPitch()})};
const idFor=(id:string,template:string)=>"comparison:"+id+":"+template;
export type ComparisonRaster={id:string;name:string;tiles:string;bounds:[number,number,number,number];maxzoom:number};
export function SecondaryMapView({storageKey,primary,collection,layers,templates,close,title="Bản đồ so sánh",headerContent,rasterFrame}:{storageKey:string;primary:RefObject<ml.Map|null>;collection:FeatureCollection;layers:ComparisonLayer[];templates:ml.LayerSpecification[];close:()=>void;title?:string;headerContent?:ReactNode;rasterFrame?:ComparisonRaster}){
 const host=useRef<HTMLDivElement>(null),map=useRef<ml.Map|null>(null),[ready,setReady]=useState(false),[error,setError]=useState(""),[controls,setControls]=useState(false);
 const [raw,setRaw]=useState<ComparisonPrefs>(()=>{try{return comparisonPrefs(JSON.parse(readPreference(storageKey)??"null"),layers)}catch{return comparisonPrefs(null,layers)}});
 const prefs=useMemo(()=>comparisonPrefs(raw,layers),[raw,layers]),current=useRef(prefs);current.current=prefs;
 const initial=useRef(prefs.camera??capture(primary.current)??{center:[106.4,14.7] as [number,number],zoom:5,bearing:0,pitch:0});
 const update=(patch:Partial<ComparisonPrefs>)=>setRaw(old=>({...comparisonPrefs(old,layers),...patch}));
 useEffect(()=>{writePreference(storageKey,prefs)},[storageKey,prefs]);
 const data=useMemo(()=>{try{return {collection:comparisonCollection(collection,layers),error:""}}catch(e){return {collection:empty,error:e instanceof Error?e.message:String(e)}}},[collection,layers]);
 const structural=JSON.stringify((data.error?[]:prefs.order).map(id=>{const l=layers.find(x=>x.id===id)!;return {id,min:l.min,max:l.max}}));
 useEffect(()=>{
  if(!host.current)return;let m:ml.Map;
  try{m=new ml.Map({container:host.current,style:{version:8,sources:{core:{type:"geojson",data:empty}},layers:[{id:"comparison-bg",type:"background",paint:{"background-color":"#e9eff3"}}]},...initial.current,attributionControl:false,maxPitch:60})}
  catch{setError("Không đủ tài nguyên WebGL để mở khung bản đồ thứ hai.");return}
  map.current=m;let live=true,frame=0;
  m.addControl(new ml.NavigationControl(),"top-right");m.addControl(new ml.ScaleControl({maxWidth:100,unit:"metric"}),"bottom-left");
  const loaded=()=>{if(live)setReady(true)};
  m.on("load",loaded);
  const moved=()=>{if(!live)return;const camera=capture(m);if(camera)setRaw(old=>({...old,camera}))};
  m.on("moveend",moved);
  const ro=new ResizeObserver(()=>{if(!frame)frame=requestAnimationFrame(()=>{frame=0;if(live&&host.current?.clientWidth&&host.current.clientHeight)m.resize()})});
  ro.observe(host.current);
  return()=>{live=false;ro.disconnect();if(frame)cancelAnimationFrame(frame);map.current=null;m.remove()};
 },[]);
 useEffect(()=>{const m=map.current;if(!m||!ready)return;
  try{const order=JSON.parse(structural) as {id:string;min:number;max:number}[];const keep=new Set(order.flatMap(l=>templates.map(t=>idFor(l.id,t.id))));
   for(const l of m.getStyle().layers)if(l.id.startsWith("comparison:")&&!keep.has(l.id))m.removeLayer(l.id);
   for(const l of [...order].reverse())for(const t of templates){const id=idFor(l.id,t.id);
    if(!m.getLayer(id))m.addLayer({...t,id,source:"core",minzoom:l.min,maxzoom:l.max,filter:["all",...((t as ml.FillLayerSpecification).filter?[(t as ml.FillLayerSpecification).filter]:[]),["==",["get","layerId"],l.id]]} as ml.LayerSpecification);
    else m.setLayerZoomRange(id,l.min,l.max);
    m.moveLayer(id);
   }
  }catch(e){setError(e instanceof Error?e.message:"Không tạo được thứ tự hiển thị.")}
 },[ready,structural,templates]);
 const rasterKey=rasterFrame?JSON.stringify(rasterFrame):"";
 useEffect(()=>{const m=map.current;if(!m||!ready)return;const id="comparison-temporal-raster";
  const clear=()=>{if(m.getLayer(id))m.removeLayer(id);if(m.getSource(id))m.removeSource(id)};
  try{clear();if(rasterKey){const r=JSON.parse(rasterKey) as ComparisonRaster;m.addSource(id,{type:"raster",tiles:[r.tiles],tileSize:256,bounds:r.bounds,maxzoom:r.maxzoom});const before=m.getStyle().layers.find(l=>l.id.startsWith("comparison:"))?.id;m.addLayer({id,type:"raster",source:id,paint:rasterPaint(defaultRasterDisplay)},before)}}catch{setError("Không tải được khung raster.")}
  return()=>{try{clear()}catch{/* map disposed */}};
 },[ready,rasterKey]);
 const hiddenKey=JSON.stringify(prefs.hidden);
 useEffect(()=>{const m=map.current;if(!m||!ready)return;const hidden=new Set<string>(JSON.parse(hiddenKey));
  for(const l of layers)for(const t of templates){const id=idFor(l.id,t.id);if(m.getLayer(id))m.setLayoutProperty(id,"visibility",hidden.has(l.id)?"none":"visible")}
 },[ready,structural,hiddenKey,templates]);
 useEffect(()=>{if(!ready)return;const source=map.current?.getSource("core") as ml.GeoJSONSource|undefined;if(source)void Promise.resolve(source.setData(data.collection)).catch(()=>setError("Không tải được dữ liệu vào khung phụ."))},[ready,data]);
 useEffect(()=>{const m=primary.current,second=map.current;if(!ready||!m||!second||!prefs.follow)return;
  const sync=()=>{const c=capture(m);if(c)second.jumpTo(c)};sync();m.on("moveend",sync);
  return()=>{m.off("moveend",sync)}
 },[ready,prefs.follow,primary]);
 const copyCamera=()=>{const c=capture(primary.current);if(c)map.current?.jumpTo(c)};
 const ordered=data.error?[]:prefs.order.map(id=>layers.find(l=>l.id===id)!);
 return <aside className="ovg-secondary-map" aria-label={title==="Bản đồ so sánh"?"Khung bản đồ so sánh chỉ đọc":title+" chỉ đọc"}><header><b>{title}</b><button type="button" aria-label={"Đóng "+title} onClick={close}>×</button></header>{headerContent}
 <div className="ovg-secondary-actions"><button type="button" aria-expanded={controls} onClick={()=>setControls(v=>!v)}>Thứ tự lớp</button><button type="button" onClick={copyCamera}>Lấy góc nhìn chính</button><label><input type="checkbox" checked={prefs.follow} onChange={e=>update({follow:e.target.checked})}/>Theo góc nhìn chính</label></div>
 {controls&&<div className="ovg-secondary-order"><small>Trên cùng vẽ sau. Chỉ đổi khung phụ.</small>{ordered.map((l,i)=><div key={l.id}><label><input type="checkbox" checked={!prefs.hidden.includes(l.id)} onChange={e=>update({hidden:e.target.checked?prefs.hidden.filter(id=>id!==l.id):[...prefs.hidden,l.id]})}/>{l.name}</label><button type="button" aria-label={"Đưa "+l.name+" lên trong khung phụ"} disabled={i===0} onClick={()=>update({order:moveComparisonLayer(prefs.order,l.id,-1)})}>↑</button><button type="button" aria-label={"Đưa "+l.name+" xuống trong khung phụ"} disabled={i===ordered.length-1} onClick={()=>update({order:moveComparisonLayer(prefs.order,l.id,1)})}>↓</button></div>)}<button type="button" onClick={()=>update({order:layers.map(l=>l.id),hidden:[]})}>Đặt lại khung phụ</button></div>}
 <div className="ovg-secondary-canvas" ref={host}/>{(data.error||error)&&<p role="alert">{data.error||error}</p>}<small className="ovg-secondary-status">{data.error?"Không hiển thị dữ liệu vượt giới hạn.":data.collection.features.length.toLocaleString("vi-VN")+" đối tượng vector đang nạp"} · Chỉ đọc · {rasterFrame?"Raster: "+rasterFrame.name+" · tile render sẵn":"Không gồm raster"} · Không gồm nền ảnh, nhãn hoặc vector tiles.</small></aside>
}
