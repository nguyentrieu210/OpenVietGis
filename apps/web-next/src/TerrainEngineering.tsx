import {useEffect,useMemo,useRef,useState} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import type {Map as MLMap} from "maplibre-gl";
import type {FeatureCollection} from "geojson";
import {geographic3857,validateTerrain,type TerrainGrid} from "@openvietgis/spatial-workspace/terrain";
import {analysisFeatures,contours,cutFill,differenceTerrain,conditionedFlow,conditionedFlowFeatures,connectedInundation,elevationPoints,flowD8,gridGeoTiff,interpolateTIN,terrainDerivatives,type AnalysisMode,type ElevationPoint} from "@openvietgis/spatial-workspace/terrain-analysis";
import {eligibleDem} from "./Spatial3DSettings";
import {rasterPath,type StoredRaster} from "./ProjectRaster";
import type {StudioRow} from "./studio-contract";
const sourceId="ovg-engineering-result",fillId=sourceId+"-fill",lineId=sourceId+"-line";
const fmt=(n:number)=>n.toLocaleString("vi-VN",{maximumFractionDigits:2});
const modes={elevation:"Màu cao độ",contour:"Đường đồng mức",slope:"Độ dốc",aspect:"Hướng dốc",cutfill:"Đào / đắp",compare:"Đào / đắp hai DEM",flow:"Hướng dòng chảy D8",conditioned:"D8 hiệu chỉnh hố trũng",flood:"Vùng thấp dưới mực nước",connected:"Vùng ngập kết nối biên"} as const;
type Mode=keyof typeof modes;
type Props={client:ApiClient;org:string;pid:string;map:MLMap|null;rows:StudioRow[];rasters:StoredRaster[];canWrite:boolean;onBusy:(v:boolean)=>void;close:()=>void;onStored:(r:StoredRaster)=>void;expire:()=>void};
function download(name:string,data:Blob){const url=URL.createObjectURL(data),a=document.createElement("a");a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)}
export function TerrainEngineering({client,org,pid,map,rows,rasters,canWrite,onBusy,close,onStored,expire}:Props){
 const [kind,setKind]=useState<"dem"|"tin">("dem"),[dem,setDem]=useState(""),[layer,setLayer]=useState(""),[field,setField]=useState(""),[reference,setReference]=useState<TerrainGrid["vertical_reference"]>("local"),[label,setLabel]=useState(""),[ack,setAck]=useState(false),[edge,setEdge]=useState(1000),[grid,setGrid]=useState<TerrainGrid|null>(null),[mode,setMode]=useState<Mode>("elevation"),[interval,setInterval]=useState(5),[level,setLevel]=useState(""),[output,setOutput]=useState<FeatureCollection|null>(null),[resultMode,setResultMode]=useState<Mode>("elevation"),[summary,setSummary]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [designId,setDesignId]=useState(""),[designAck,setDesignAck]=useState(false);
 const abort=useRef<AbortController|null>(null),alive=useRef(true);
 const live=useRef({rasters,canWrite});live.current={rasters,canWrite};
 useEffect(()=>{onBusy(busy);return()=>onBusy(false)},[busy,onBusy]);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;abort.current?.abort()}},[]);
 const candidates=useMemo(()=>[...new Map(rows.filter(r=>r.geometry&&["Point","MultiPoint","LineString","MultiLineString"].includes(r.geometry.type)).map(r=>[r.layer.id,r.layer])).values()],[rows]);
 const sourceRows=useMemo(()=>rows.filter(r=>r.layer.id===layer),[rows,layer]);
 const fields=useMemo(()=>[...new Set(sourceRows.flatMap(r=>Object.keys(r.feature.attributes).filter(k=>typeof r.feature.attributes[k]==="number"&&Number.isFinite(r.feature.attributes[k]))))].sort(),[sourceRows]);
 useEffect(()=>{setGrid(null);setOutput(null);setSummary("");setAck(false)},[kind,dem,layer,field,reference,label,edge]);
 // Stale project source must not remain analyzable after a permission or revision change.
 useEffect(()=>{if(grid&&(kind==="dem"&&!rasters.some(r=>r.id===dem)||kind==="tin"||resultMode==="compare"&&!rasters.some(r=>r.id===designId))){setGrid(null);setOutput(null);setSummary("")}},[rows,rasters]);
 const enabled=ack&&label.trim().length>0&&!busy&&(kind==="dem"?rasters.some(r=>r.id===dem&&eligibleDem(r)):!!layer&&!!field&&Number.isFinite(edge)&&edge>=1&&edge<=100000);
 async function fail(e:unknown){if(e instanceof ApiError&&e.status===401){expire();return}if(alive.current)setError(e instanceof Error?e.message:String(e))}
 const load=async()=>{if(!enabled)return;setBusy(true);setError("");setGrid(null);setOutput(null);setSummary("");const a=new AbortController();abort.current=a;
  try{
   let g:TerrainGrid;
   if(kind==="dem"){await loadCsrfToken(client);if(a.signal.aborted)return;g=validateTerrain(await client.request<unknown>(rasterPath(org,pid)+"/"+encodeURIComponent(dem)+"/terrain-preview",{method:"POST",body:JSON.stringify({elevation_unit:"metre",vertical_reference:reference,reference_label:label.trim(),acknowledge_dem:true}),signal:a.signal,cache:"no-store"}),dem);if(g.reference_label!==label.trim()||g.vertical_reference!==reference)throw Error("Mốc cao độ phản hồi không khớp")}
   else {
    const selected=candidates.find(r=>r.id===layer);if(!selected||selected.source.feature_count!==sourceRows.length)throw Error("Cần tải đủ lớp nguồn, không dùng lớp tile hoặc danh sách bị lọc.");
    const points:ElevationPoint[]=[];
    for(const r of sourceRows){const z=r.feature.attributes[field];if(typeof z!=="number"||!r.geometry)throw Error("Mỗi đối tượng nguồn phải có hình học và trường cao độ kiểu số.");points.push(...elevationPoints(r.geometry,z));if(points.length>8000)throw Error("Tối đa 8.000 đỉnh; hãy cắt lớp nguồn theo khu vực.")}
    g=interpolateTIN(points,edge,label,reference);
   }
   if(a.signal.aborted||!alive.current)return;
   if(kind==="dem"&&!live.current.rasters.some(r=>r.id===dem&&eligibleDem(r)))throw Error("DEM nguồn đã thay đổi hoặc không còn quyền truy cập.");
   setGrid(g);setLevel(String(Math.round(g.min_m*100)/100));setMode("elevation");setResultMode("elevation");setOutput(analysisFeatures(g,"elevation",0));setSummary("Cao độ "+fmt(g.min_m)+"–"+fmt(g.max_m)+" m");
   const [l,b,r,t]=g.extent;map?.fitBounds([geographic3857(l,b),geographic3857(r,t)],{padding:80,duration:350,maxZoom:19});
  }catch(e){if(!a.signal.aborted)await fail(e)}finally{if(!a.signal.aborted&&alive.current)setBusy(false)}
 };
 const run=async()=>{if(!grid||busy)return;try{setError("");const z=Number(level);if((mode==="cutfill"||mode==="flood")&&(!level.trim()||!Number.isFinite(z)||z< -12000||z>10000))throw Error("Nhập cao độ mét hợp lệ.");if(mode==="compare"){
  if(!designAck||!live.current.rasters.some(r=>r.id===designId&&eligibleDem(r))||designId===grid.source_id)throw Error("Chọn DEM thiết kế khác và xác nhận cùng mốc cao độ.");
  setBusy(true);const a=new AbortController();abort.current=a;
  try{await loadCsrfToken(client);if(a.signal.aborted)return;
   const design=validateTerrain(await client.request<unknown>(rasterPath(org,pid)+"/"+encodeURIComponent(designId)+"/terrain-preview",{method:"POST",body:JSON.stringify({elevation_unit:"metre",vertical_reference:grid.vertical_reference,reference_label:grid.reference_label,acknowledge_dem:true}),signal:a.signal,cache:"no-store"}),designId);
   if(a.signal.aborted||!alive.current)return;
   if(!live.current.rasters.some(r=>r.id===designId&&eligibleDem(r))||(kind==="dem"&&!live.current.rasters.some(r=>r.id===grid.source_id)))throw Error("DEM đã thay đổi hoặc không còn quyền.");
   const d=differenceTerrain(grid,design),v=cutFill(d,0);setOutput(analysisFeatures(d,"cutfill",0));setResultMode(mode);setSummary("Hai DEM · Đào "+fmt(v.cut)+" m³ · Đắp "+fmt(v.fill)+" m³ · Chênh "+fmt(v.net)+" m³ · Phạm vi "+fmt(v.coveredArea)+" m²");
  }finally{if(!a.signal.aborted&&alive.current)setBusy(false)}
  return;
 }
 if(mode==="connected"&&(!level.trim()||!Number.isFinite(z)))throw Error("Nhập mực nước hợp lệ.");
 const conn=mode==="connected"?connectedInundation(grid,z):null;
 const f=mode==="contour"?contours(grid,interval):mode==="conditioned"?conditionedFlowFeatures(grid):conn?conn.features:analysisFeatures(grid,mode as AnalysisMode,z);
  let s=f.features.length+" đối tượng kết quả";
  if(mode==="cutfill"){const v=cutFill(grid,z);s="Đào "+fmt(v.cut)+" m³ · Đắp "+fmt(v.fill)+" m³ · Chênh "+fmt(v.net)+" m³ · Phạm vi có dữ liệu "+fmt(v.coveredArea)+" m²"}
  else if(mode==="conditioned"){const d=conditionedFlow(grid);s="Hiệu chỉnh "+d.raised+" mẫu · nâng tối đa "+fmt(d.maxRaise)+" m · "+d.outlets+" cửa thoát · "+d.closed+" mẫu kín bị loại. Không mô phỏng lưu lượng."}
  else if(conn)s=conn.connected+" ô kết nối biên · "+conn.isolated+" ô thấp cô lập. Không mô phỏng thủy lực.";
  else if(mode==="flow"){const f=flowD8(grid);s="D8 · "+f.sinks+" điểm không có lối xuống (gồm biên, vùng phẳng/hố trũng). Chưa lấp hố trũng."}
  else if(mode==="slope"||mode==="aspect"){const v=terrainDerivatives(grid)[mode].filter((v):v is number=>v!==null);if(v.length)s=fmt(Math.min(...v))+"–"+fmt(Math.max(...v))+"° · "+v.length+" mẫu nội bộ"}
  else if(mode==="flood")s=f.features.length+" ô dưới ngưỡng · không mô phỏng kết nối thủy lực";
  setResultMode(mode);setOutput(f);setSummary(s);
 }catch(e){setOutput(null);setSummary("");void fail(e)}};
 useEffect(()=>{if(!map||!output)return;const clear=()=>{try{if(map.getLayer(lineId))map.removeLayer(lineId);if(map.getLayer(fillId))map.removeLayer(fillId);if(map.getSource(sourceId))map.removeSource(sourceId)}catch{}};
  clear();try{map.addSource(sourceId,{type:"geojson",data:output});map.addLayer({id:fillId,type:"fill",source:sourceId,filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":["coalesce",["get","color"],"#568e5e"],"fill-opacity":.6}});map.addLayer({id:lineId,type:"line",source:sourceId,filter:["==",["geometry-type"],"LineString"],paint:{"line-color":["coalesce",["get","color"],"#80552d"],"line-width":["coalesce",["get","width"],1.5]}})}catch(e){void fail(e)}return clear
 },[map,output]);
 const save=async()=>{if(!grid||kind!=="tin"||!canWrite||busy)return;const a=new AbortController();abort.current=a;setBusy(true);setError("");try{const form=new FormData();form.set("method","band");form.set("band_a","1");form.set("name",("DEM TIN · "+label.trim()).slice(0,160));form.set("file",new File([gridGeoTiff(grid)],"tin-dem-3857.tif",{type:"image/tiff"}));await loadCsrfToken(client);if(a.signal.aborted)return;if(!live.current.canWrite)throw Error("Quyền lưu raster đã thay đổi.");const saved=await client.request<StoredRaster>(rasterPath(org,pid),{method:"POST",body:form,signal:a.signal});if(!a.signal.aborted&&alive.current){onStored(saved);close()}}catch(e){if(!a.signal.aborted)await fail(e)}finally{if(!a.signal.aborted&&alive.current)setBusy(false)}};
 return <aside className="panel pro-dock raster-workbench ovg-terrain-engineering" role="dialog" aria-label="Địa hình kỹ thuật"><header><div><small>GIS CORE · DEM / TIN</small><b>Địa hình kỹ thuật</b></div><button aria-label="Đóng địa hình kỹ thuật" disabled={busy} onClick={close}>✕</button></header><div className="pro-dock-body">
  <fieldset disabled={busy}><label>Nguồn cao độ<select aria-label="Nguồn cao độ" value={kind} onChange={e=>setKind(e.target.value as typeof kind)}><option value="dem">DEM dự án (GeoTIFF)</option><option value="tin">TIN từ điểm / đường bình độ</option></select></label>
  {kind==="dem"?<label>DEM đã nhập<select aria-label="DEM đã nhập" value={dem} onChange={e=>setDem(e.target.value)}><option value="">Chọn raster một band</option>{rasters.filter(eligibleDem).map(r=><option key={r.id} value={r.id}>{r.name}</option>)}</select></label>:<><label>Lớp điểm / bình độ<select aria-label="Lớp bình độ" value={layer} onChange={e=>{setLayer(e.target.value);setField("")}}><option value="">Chọn lớp GIS đã tải đủ</option>{candidates.map(r=><option key={r.id} value={r.id}>{r.name}</option>)}</select></label><label>Trường cao độ (m)<select aria-label="Trường cao độ" value={field} onChange={e=>setField(e.target.value)}><option value="">Chọn trường số</option>{fields.map(f=><option key={f}>{f}</option>)}</select></label><label>Cạnh TIN tối đa (m)<input aria-label="Cạnh TIN tối đa" type="number" min={1} max={100000} value={edge} onChange={e=>setEdge(Number(e.target.value))}/></label><small>Delaunay không ràng buộc breakline; lấy đỉnh đường và điểm, không ngoại suy ngoài tam giác. Cắt cạnh dài, giữ NoData. Tối đa 8.000 đỉnh.</small></>}
  <label>Hệ cao độ<select aria-label="Hệ cao độ" value={reference} onChange={e=>setReference(e.target.value as typeof reference)}><option value="local">Mốc cục bộ</option><option value="orthometric">Cao độ thủy chuẩn</option><option value="ellipsoidal">Cao độ ellipsoid</option></select></label>
  <label>Tên mốc cao độ<input aria-label="Tên mốc cao độ" maxLength={120} value={label} onChange={e=>setLabel(e.target.value)} placeholder="Mốc khảo sát đã biết"/></label>
  <label><input type="checkbox" checked={ack} onChange={e=>setAck(e.target.checked)}/> Xác nhận cao độ tính bằng mét, cùng mốc đã khai báo.</label>
  <button className="primary" disabled={!enabled} onClick={()=>void load()}>{busy?"Đang xử lý…":"Tạo lưới phân tích"}</button></fieldset>
  {grid&&<><p className="pro-note">{grid.width}×{grid.height} · {grid.valid_count} mẫu hợp lệ · {grid.nodata_count} NoData · {grid.reference_label}</p><label>Phân tích<select aria-label="Phân tích địa hình" disabled={busy} value={mode} onChange={e=>{setMode(e.target.value as Mode);setOutput(null);setSummary("")}}>{Object.entries(modes).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select></label>
   {mode==="contour"&&<label>Khoảng cao đều (m)<input type="number" min=".01" step="any" value={interval} onChange={e=>{setInterval(Number(e.target.value));setOutput(null);setSummary("")}}/></label>}
   {mode==="compare"&&<><label>DEM thiết kế<select aria-label="DEM thiết kế" disabled={busy} value={designId} onChange={e=>{setDesignId(e.target.value);setDesignAck(false);setOutput(null);setSummary("")}}><option value="">Chọn DEM thiết kế</option>{rasters.filter(r=>eligibleDem(r)&&r.id!==grid.source_id).map(r=><option key={r.id} value={r.id}>{r.name}</option>)}</select></label><label><input type="checkbox" disabled={busy} checked={designAck} onChange={e=>{setDesignAck(e.target.checked);setOutput(null);setSummary("")}}/> Hai DEM có cùng đơn vị và mốc cao độ.</label><small>Hai lưới phải trùng kích thước, phạm vi và mốc; không tự nội suy lệch lưới.</small></>}
   {(mode==="cutfill"||mode==="flood"||mode==="connected")&&<label>{mode==="cutfill"?"Cao độ thiết kế phẳng":"Mực nước giả định"} (m)<input aria-label="Cao độ thiết kế hoặc mực nước" type="number" step="any" value={level} onChange={e=>{setLevel(e.target.value);setOutput(null);setSummary("")}}/></label>}
   <button disabled={busy||(mode==="compare"&&(!designId||!designAck))} onClick={()=>void run()}>Tính &amp; hiển thị kết quả</button><p className="ovg-engineering-result" role="status">{summary}</p>
   {output&&<button onClick={()=>download("terrain-"+resultMode+".geojson",new Blob([JSON.stringify({...output,ovg_terrain:{mode:resultMode,source:grid.source_id,design_source:resultMode==="compare"?designId:null,vertical_reference:grid.vertical_reference,reference_label:grid.reference_label,elevation_unit:"metre",sampled:true,survey_grade:false,level_m:resultMode==="cutfill"||resultMode==="flood"||resultMode==="connected"?Number(level):null,interval_m:resultMode==="contour"?interval:null}})],{type:"application/geo+json"}))}>Tải GeoJSON kết quả</button>}
   <button onClick={()=>download("terrain-"+kind+"-3857.tif",new Blob([gridGeoTiff(grid)],{type:"image/tiff"}))}>Tải DEM GeoTIFF</button>
   {kind==="tin"&&canWrite&&<button className="primary" disabled={busy} onClick={()=>void save()}>Lưu TIN thành DEM dự án</button>}
   <small>DEM đã lưu có thể mở bằng GIS → ⚙ → 3D nâng cao → Raster địa hình. Khai báo lại đúng mốc cao độ.</small>
  </>}
  {error&&<p className="error" role="alert">{error}</p>}<p className="pro-note">Lưới lấy mẫu tối đa 128×128, diện tích quy đổi gần đúng theo vĩ độ từ Web Mercator; đào/đắp so với mặt phẳng thiết kế, D8 hiệu chỉnh giả định biên mở và NoData là rào chắn; vùng ngập kết nối chỉ là ngưỡng tĩnh, không mô phỏng thủy lực. Không dùng cho nghiệm thu hoặc thiết kế chính xác. Dùng công cụ GDAL raster gốc để tạo slope/aspect/contour ở độ phân giải đầy đủ.</p>
 </div></aside>;
}
