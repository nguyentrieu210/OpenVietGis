import {useEffect,useMemo,useState,type RefObject} from "react";
import type * as ml from "./map-surface";
import type {StudioRow} from "./studio-contract";
import type {LayerView} from "./LayerProperties";
import {FeatureInfo} from "./FeatureInfo";
import {identifyHits,identifyRows,identifyId,mapTipText,type IdentifyHit} from "./identify-plan";
export function MultiIdentify({hits,rows,close}:{hits:IdentifyHit[];rows:StudioRow[];close:()=>void}){
 const matches=useMemo(()=>identifyRows(hits,rows),[hits,rows]);
 const [active,setActive]=useState("");
 const selected=matches.find(r=>identifyId({layerId:r.layer.id,key:r.key})===active)??matches[0];
 return <aside className="panel" aria-label="Identify nhiều lớp" style={{position:"absolute",top:48,right:12,width:"min(360px, 90%)",maxHeight:"70%",overflow:"auto",zIndex:8,padding:12}}>
 <header><strong>Identify nhiều lớp</strong> <button type="button" aria-label="Đóng Identify" onClick={close}>×</button></header>
 <p>{matches.length} đối tượng đang hiển thị{hits.length===100?" (giới hạn 100)":""}. Bấm bản đồ để xác định vị trí khác. Không thay đổi vùng chọn.</p>
 {matches.length>0&&<><label>Đối tượng<select value={selected?identifyId({layerId:selected.layer.id,key:selected.key}):""} onChange={e=>setActive(e.target.value)}>{matches.map(r=><option key={identifyId({layerId:r.layer.id,key:r.key})} value={identifyId({layerId:r.layer.id,key:r.key})}>{r.layer.name} · {r.feature.feature_id}</option>)}</select></label>{selected&&<FeatureInfo key={identifyId({layerId:selected.layer.id,key:selected.key})} row={selected}/>}</>}
 </aside>
}
export function MapTips({map,ready,rows,views,renderLayers,disabled}:{map:RefObject<ml.Map|null>;ready:boolean;rows:StudioRow[];views:Record<string,LayerView>;renderLayers:RefObject<string[]>;disabled:boolean}){
 const [hit,setHit]=useState<IdentifyHit|null>(null);
 const selected=useMemo(()=>hit?identifyRows([hit],rows)[0]:undefined,[hit,rows]);
 const value=selected?mapTipText(selected,views[selected.layer.id]?.mapTipField??""):"";
 const enabled=!disabled&&Object.values(views).some(v=>!!v.mapTipField);
 useEffect(()=>{const m=map.current;setHit(null);if(!enabled||!ready||!m)return;let timer=0;
  const clear=()=>{window.clearTimeout(timer);setHit(null)};
  const move=(e:ml.MapMouseEvent)=>{window.clearTimeout(timer);setHit(null);if(m.isMoving()||document.body.dataset.ovgSpacePan)return;const point=e.point;timer=window.setTimeout(()=>{try{const ids=renderLayers.current.filter(id=>!!m.getLayer(id));if(!ids.length)return;const hits=identifyHits(m.queryRenderedFeatures(point,{layers:ids}));setHit(hits.find(h=>!!views[h.layerId]?.mapTipField)??null)}catch{setHit(null)}},350)};
  m.on("mousemove",move);m.on("movestart",clear);m.getCanvas().addEventListener("mouseleave",clear);return()=>{window.clearTimeout(timer);m.off("mousemove",move);m.off("movestart",clear);m.getCanvas().removeEventListener("mouseleave",clear)};
 },[map,ready,enabled,views,renderLayers]);
 return enabled&&selected&&value?<div role="status" className="panel" style={{position:"absolute",left:12,bottom:48,maxWidth:"min(320px, 85%)",zIndex:6,pointerEvents:"none",padding:8,overflowWrap:"anywhere"}}><strong>{selected.layer.name}</strong><div>{value}</div></div>:null;
}
