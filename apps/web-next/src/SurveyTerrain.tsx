import {useEffect,useRef,useState} from "react";
import {LngLat,type Map as MLMap,type MapMouseEvent} from "maplibre-gl";

const demId="ovg-survey-dem",shadeId="ovg-survey-shade-dem",shadeLayer="ovg-survey-hillshade";
export const terrainSources={
 mapterhorn:{name:"Mapterhorn",tiles:["https://tiles.mapterhorn.com/{z}/{x}/{y}.webp"],tileSize:512,maxzoom:12,attribution:'<a href="https://mapterhorn.com/attribution/" target="_blank" rel="noopener">© Mapterhorn · nguồn DEM</a>'},
 aws:{name:"Mapzen / AWS",tiles:["https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"],tileSize:256,maxzoom:12,attribution:'<a href="https://registry.opendata.aws/terrain-tiles/" target="_blank" rel="noopener">Mapzen / AWS Terrain Tiles</a> · <a href="https://github.com/tilezen/joerd/blob/master/docs/attribution.md" target="_blank" rel="noopener">Nguồn dữ liệu</a>'},
} as const;
type Source=keyof typeof terrainSources;
type Status="loading"|"ready"|"error";
const remove=(m:MLMap)=>{
 if(m.getTerrain()?.source===demId)m.setTerrain(null);
 if(m.getLayer(shadeLayer))m.removeLayer(shadeLayer);
 for(const id of [shadeId,demId])if(m.getSource(id))m.removeSource(id);
};
export function SurveyTerrain({map,onClose}:{map:MLMap;onClose:()=>void}){
 const [source,setSource]=useState<Source>("mapterhorn"),[exaggeration,setExaggeration]=useState(1),[hillshade,setHillshade]=useState(true),[status,setStatus]=useState<Status>("loading"),[height,setHeight]=useState<number|null>(null);
 const prefs=useRef({exaggeration,hillshade});prefs.current={exaggeration,hillshade};
 const lastPoint=useRef<Parameters<MLMap["queryTerrainElevation"]>[0]|null>(null);
 const healthy=useRef(false);
 const [profileStart,setProfileStart]=useState<{lng:number;lat:number}|null>(null),[profile,setProfile]=useState<{length:number;values:(number|null)[]}|null>(null);
 const elevation=(p:Parameters<MLMap["queryTerrainElevation"]>[0])=>{if(!map.getTerrain()||!healthy.current)return null;const v=map.terrain?.getDrawnElevationForLngLat(LngLat.convert(p));return v!==undefined&&Number.isFinite(v)?v/prefs.current.exaggeration:null};
 // Own only our sources and layers. Do not replace the existing GIS style or map.
 useEffect(()=>{
  let disposed=false,errors=0,lastMove=0;healthy.current=false;setStatus("loading");setHeight(null);setProfile(null);setProfileStart(null);
  const cfg=terrainSources[source];
  const fail=()=>{if(disposed)return;healthy.current=false;setHeight(null);setStatus("error")};
  const onError=(e:unknown)=>{const v=e as {sourceId?:string;error?:{message?:string}};if(v.sourceId!==demId&&v.sourceId!==shadeId&&!v.error?.message?.includes(source==="mapterhorn"?"tiles.mapterhorn.com":"elevation-tiles-prod"))return;if(++errors>=3&&!healthy.current)fail()};
  const onData=()=>{if(disposed||!map.getTerrain()||!map.isSourceLoaded(demId))return;const v=map.terrain?.getDrawnElevationForLngLat(map.getCenter());if(v!==undefined&&Number.isFinite(v)){healthy.current=true;setStatus("ready");setHeight(lastPoint.current?elevation(lastPoint.current):elevation(map.getCenter()))}};
  const move=(e:MapMouseEvent)=>{lastPoint.current=e.lngLat;const now=performance.now();if(now-lastMove<80)return;lastMove=now;setHeight(elevation(e.lngLat))};
  const leave=()=>{lastPoint.current=null;setHeight(null)};
  const center=()=>{lastPoint.current=null;setHeight(elevation(map.getCenter()))};
  map.on("error",onError);map.on("idle",onData);map.on("mousemove",move);map.on("mouseout",leave);map.on("moveend",center);
  try{
   remove(map);
   for(const id of [demId,shadeId])map.addSource(id,{type:"raster-dem",tiles:[...cfg.tiles],tileSize:cfg.tileSize,maxzoom:cfg.maxzoom,encoding:"terrarium",attribution:cfg.attribution});
   const before=map.getStyle().layers.find(l=>l.id!=="bg"&&l.id!=="basemap")?.id;
   map.addLayer({id:shadeLayer,type:"hillshade",source:shadeId,layout:{visibility:prefs.current.hillshade?"visible":"none"},paint:{"hillshade-exaggeration":.35,"hillshade-shadow-color":"#333b36","hillshade-highlight-color":"#ffffff","hillshade-accent-color":"#58625e"}},before);
   map.setTerrain({source:demId,exaggeration:prefs.current.exaggeration});
   map.easeTo({pitch:55,duration:350});
  }catch{fail()}
  const timer=window.setTimeout(()=>{if(!healthy.current)fail()},18000);
  return ()=>{disposed=true;healthy.current=false;clearTimeout(timer);map.off("error",onError);map.off("idle",onData);map.off("mousemove",move);map.off("mouseout",leave);map.off("moveend",center);try{remove(map);map.jumpTo({pitch:0})}catch{/* map may already have been removed */}};
 },[map,source]);
 useEffect(()=>{if(map.getSource(demId)&&map.getTerrain()?.source===demId){map.setTerrain({source:demId,exaggeration});setHeight(lastPoint.current?elevation(lastPoint.current):elevation(map.getCenter()));setProfile(null)}},[map,exaggeration]);
 useEffect(()=>{if(map.getLayer(shadeLayer))map.setLayoutProperty(shadeLayer,"visibility",hillshade?"visible":"none")},[map,hillshade]);
 const capture=()=>{
  const p=map.getCenter();if(!profileStart){setProfileStart({lng:p.lng,lat:p.lat});setProfile(null);return}
  const start=profileStart,delta=((p.lng-start.lng+540)%360)-180,dist=p.distanceTo(new LngLat(start.lng,start.lat));
  if(dist<1)return;
  const values=Array.from({length:33},(_,i)=>elevation([start.lng+delta*i/32,start.lat+(p.lat-start.lat)*i/32]));
  setProfile({length:dist,values});setProfileStart(null);
 };
 const values=profile?.values.filter((v):v is number=>v!==null)??[],min=values.length?Math.min(...values):0,max=values.length?Math.max(...values):0;
 return <section className="ovg-survey-terrain" data-terrain-status={status} data-terrain-source={source} data-terrain-height={height??""} aria-label="Địa hình khảo sát">
  <div className="ovg-terrain-summary"><strong>Địa hình 3D</strong><span>{height===null?"Cao độ —":"~ "+height.toFixed(1)+" m"}</span><button type="button" aria-label="Thoát địa hình 3D" onClick={onClose}>×</button></div>
  <div className="ovg-terrain-status" role="status">{status==="ready"?terrainSources[source].name+" · DEM tổng quan":status==="error"?"Chưa tải được DEM. Chọn nguồn khác hoặc quay về 2D.":"Đang tải dữ liệu cao độ…"}</div>
  <details><summary>Thiết lập địa hình</summary><div className="ovg-terrain-settings">
   <label>Nguồn DEM<select aria-label="Nguồn DEM" value={source} onChange={e=>setSource(e.target.value as Source)}><option value="mapterhorn">Mapterhorn (30 m toàn cầu)</option><option value="aws">Mapzen / AWS (dự phòng)</option></select></label>
   <label>Phóng đại địa hình<select aria-label="Phóng đại địa hình" value={exaggeration} onChange={e=>setExaggeration(Number(e.target.value))}>{[1,1.5,2,3].map(n=><option key={n} value={n}>×{n}</option>)}</select></label>
   <label><input type="checkbox" checked={hillshade} onChange={e=>setHillshade(e.target.checked)}/> Đổ bóng địa hình</label>
   <button type="button" onClick={()=>map.easeTo({pitch:55,bearing:map.getBearing(),duration:300})}>Góc nhìn nghiêng</button>
   <button type="button" onClick={()=>map.easeTo({bearing:0,duration:300})}>Hướng Bắc</button>
   <small>Giữ chuột phải hoặc Ctrl + kéo để xoay/nghiêng. Cao độ DEM tham khảo, không phải cao độ đo đạc VN-2000; phóng đại chỉ thay đổi hiển thị.</small>
   <button type="button" disabled={status!=="ready"} onClick={capture}>{profileStart?"Lấy điểm cuối tại tâm bản đồ":"Lấy điểm đầu mặt cắt tại tâm"}</button>
   {profileStart&&<small>Dịch bản đồ đến điểm cuối. <button type="button" onClick={()=>setProfileStart(null)}>Hủy</button></small>}
   {profile&&<div className="ovg-terrain-profile"><strong>Mặt cắt tham khảo · {profile.length.toFixed(0)} m</strong><small>{values.length}/33 mẫu đã tải{values.length?" · "+min.toFixed(1)+"–"+max.toFixed(1)+" m":""}</small><svg viewBox="0 0 240 76" role="img" aria-label="Mặt cắt cao độ tham khảo">{profile.values.slice(1).map((v,i)=>{const prev=profile.values[i];return v===null||prev===null?null:<line key={i} x1={i*7.5} x2={(i+1)*7.5} y1={70-(prev-min)/Math.max(1,max-min)*60} y2={70-(v-min)/Math.max(1,max-min)*60} stroke="#166b75" strokeWidth="2"/>})}</svg><small>Chỉ mẫu DEM đã tải; đoạn thiếu dữ liệu để trống. Không dùng nghiệm thu khảo sát.</small></div>}
  </div></details>
 </section>;
}
