import {useEffect,useRef,useState} from "react";
import {unified,nativeProviders,unifiedAssets,type UnifiedAsset,type UnifiedAssetKind,type AssetRead,type NativeProviderStatus,type Opened,type Project,type UnifiedCatalog,type UnifiedPlan,type UnifiedCursor,type UnifiedBatch,type AttributeField} from "./transport";
import maplibregl,{type GeoJSONSource} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
export default function UnifiedWorkbench(){
 const [projects,setProjects]=useState<Project[]>([]),[opened,setOpened]=useState<Opened>(),[catalog,setCatalog]=useState<UnifiedCatalog>();
 const [layer,setLayer]=useState(""),[name,setName]=useState(""),[source,setSource]=useState(""),[target,setTarget]=useState(""),[kind,setKind]=useState("one_many");
 const [batch,setBatch]=useState('{"edits":[],"links":[]}'),[filter,setFilter]=useState("[]"),[relation,setRelation]=useState(""),[sourceId,setSourceId]=useState("");
 const [schema,setSchema]=useState("[]"),[cursor,setCursor]=useState<UnifiedCursor|null>(null),[rows,setRows]=useState<GeoJSON.Feature[]>([]);
 const [tileMode,setTileMode]=useState(false),[visibleLayers,setVisibleLayers]=useState<string[]>([]);
 const [native,setNative]=useState<NativeProviderStatus>(),[nativeOutput,setNativeOutput]=useState("");
 const [ogrIndex,setOgrIndex]=useState("0"),[fromCrs,setFromCrs]=useState("4326"),[toCrs,setToCrs]=useState("3857"),[xyText,setXyText]=useState("[[106.7,11]]"),[band,setBand]=useState("1");
 const [asset,setAsset]=useState<UnifiedAsset>(),[assetKind,setAssetKind]=useState<UnifiedAssetKind>("cad"),[assetEpsg,setAssetEpsg]=useState("4326"),[assetChunk,setAssetChunk]=useState("0"),[assetWindow,setAssetWindow]=useState("0,0,256,256"),[assetBounds,setAssetBounds]=useState(""),[assetLod,setAssetLod]=useState("0"),[assetPreview,setAssetPreview]=useState(""),[rasterImage,setRasterImage]=useState("");
 const [message,setMessage]=useState(""),[busy,setBusy]=useState(false),[ready,setReady]=useState(false);
 const active=useRef<Opened|undefined>(undefined),locked=useRef(false),alive=useRef(true),div=useRef<HTMLDivElement>(null),map=useRef<maplibregl.Map|undefined>(undefined);
 const refresh=async(h:string)=>{const c=await unified.catalog(h);if(alive.current){setCatalog(c);setVisibleLayers(v=>v.filter(id=>c.layers.some(l=>l.id===id)));setLayer(l=>c.layers.some(x=>x.id===l)?l:c.layers[0]?.id??"");setCursor(null);setRows([]);(map.current?.getSource("features") as GeoJSONSource|undefined)?.setData({type:"FeatureCollection",features:[]})}};
 const run=async(f:()=>Promise<void>)=>{if(locked.current)return;locked.current=true;setBusy(true);setMessage("");try{await f()}catch(e){if(alive.current)setMessage(String(e))}finally{locked.current=false;if(alive.current)setBusy(false)}};
 const open=async(id:string)=>{if(active.current?.id===id)return;if(active.current){await unified.close(active.current.handle);active.current=undefined;setOpened(undefined);setCatalog(undefined);setAsset(undefined);setAssetPreview("");setRasterImage("")}const w=await unified.open(id);if(!alive.current){await unified.close(w.handle);return}active.current=w;setOpened(w);setVisibleLayers([]);await refresh(w.handle)};
 useEffect(()=>{alive.current=true;void run(async()=>{setProjects(await unified.list());setNative(await nativeProviders.status())});return()=>{alive.current=false;if(active.current)void unified.close(active.current.handle).catch(()=>{})}},[]);
 useEffect(()=>{if(!div.current)return;
 maplibregl.addProtocol("ovgr",async(params,abort)=>{
  const r=/^ovgr:\/\/v1\/([0-9a-f-]{36})\/([0-9a-f-]{36})\/([0-9]+)\/([0-9]+)\/([0-9]+)\.png$/.exec(params.url);if(!r)throw Error("INVALID_TILE_URI");if(abort.signal.aborted)throw new DOMException("Aborted","AbortError");
  const b=await unifiedAssets.read(r[1]!,r[2]!,{tile:[Number(r[3]),Number(r[4]),Number(r[5])]});if(abort.signal.aborted)throw new DOMException("Aborted","AbortError");return {data:b};
 });
 maplibregl.addProtocol("ovgu",async(params,abort)=>{
  const m=/^ovgu:\/\/v1\/([0-9a-f-]{36})\/([0-9a-f-]{36})\/([0-9]+)\/([0-9]+)\/([0-9]+)\/([0-9]+)\.mvt$/.exec(params.url);
  if(!m)throw Error("INVALID_TILE_URI");
  return {data:await unified.tile(m[1]!,m[2]!,m[3]!,Number(m[4]),Number(m[5]),Number(m[6]),abort.signal)};
 });
 const m=new maplibregl.Map({container:div.current,style:{version:8,sources:{},layers:[{id:"background",type:"background",paint:{"background-color":"#eef2f4"}}]},center:[106.7,11],zoom:6,renderWorldCopies:false});map.current=m;m.on("load",()=>{m.addSource("features",{type:"geojson",data:{type:"FeatureCollection",features:[]}});m.addSource("assets",{type:"geojson",data:{type:"FeatureCollection",features:[]}});m.addLayer({id:"fill",source:"features",type:"fill",filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":"#1976a3","fill-opacity":.25}});m.addLayer({id:"line",source:"features",type:"line",filter:["in",["geometry-type"],["literal",["Polygon","LineString"]]],paint:{"line-color":"#1976a3","line-width":2}});m.addLayer({id:"point",source:"features",type:"circle",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":"#1976a3","circle-radius":5}});m.addLayer({id:"asset-line",type:"line",source:"assets",filter:["==",["geometry-type"],"LineString"],paint:{"line-color":"#8548b5","line-width":2}});m.addLayer({id:"asset-point",type:"circle",source:"assets",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":"#8548b5","circle-radius":3}});setReady(true)});m.on("error",e=>{if(alive.current)setMessage(String(e.error))});m.addControl(new maplibregl.NavigationControl());return()=>{map.current=undefined;m.remove();maplibregl.removeProtocol("ovgu");maplibregl.removeProtocol("ovgr")}},[]);
 useEffect(()=>{
  const m=map.current;if(!m||!ready)return;
  const ids:string[]=[];
  for(const suffix of ["fill","line","point"])m.setLayoutProperty(suffix,"visibility",tileMode?"none":"visible");
  if(!tileMode||!opened||!catalog)return;
  for(const id of visibleLayers){
   if(!catalog.layers.some(l=>l.id===id))continue;
   const key="unified-"+id;ids.push(key);
   const color=["#1976a3","#388659","#a254a1","#b68924"][ids.length%4]!;
   m.addSource(key,{type:"vector",tiles:[`ovgu://v1/${opened.handle}/${id}/${catalog.revision}/{z}/{x}/{y}.mvt`],minzoom:0,maxzoom:22,promoteId:"stable_id"});
   const src={source:key,"source-layer":"local"} as const;
   m.addLayer({id:key+"-fill",...src,type:"fill",filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":color,"fill-opacity":.25}});
   m.addLayer({id:key+"-line",...src,type:"line",filter:["in",["geometry-type"],["literal",["Polygon","LineString"]]],paint:{"line-color":color,"line-width":2}});
   m.addLayer({id:key+"-point",...src,type:"circle",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":color,"circle-radius":5}});
  }
  return()=>{if(map.current!==m)return;for(const id of ids){for(const suffix of ["fill","line","point"])if(m.getLayer(id+"-"+suffix))m.removeLayer(id+"-"+suffix);if(m.getSource(id))m.removeSource(id)}};
 },[ready,tileMode,opened,catalog,visibleLayers]);
 useEffect(()=>{
  const m=map.current;if(!ready||!m)return;
  (m.getSource("assets") as GeoJSONSource|undefined)?.setData({type:"FeatureCollection",features:[]});
  if(!opened||asset?.kind!=="mbtiles")return;
  m.addSource("asset-raster",{type:"raster",tiles:[`ovgr://v1/${opened.handle}/${asset.id}/{z}/{x}/{y}.png`],tileSize:256,maxzoom:22});
  m.addLayer({id:"asset-raster",source:"asset-raster",type:"raster"},"fill");
  return()=>{if(map.current!==m)return;if(m.getLayer("asset-raster"))m.removeLayer("asset-raster");if(m.getSource("asset-raster"))m.removeSource("asset-raster")};
 },[opened,asset,ready]);
 const loadAsset=async()=>{
  if(!opened)return;
  const epsg=assetKind==="cad"?4326:["las","laz","copc"].includes(assetKind)?Number(assetEpsg):null;
  if(epsg!==null&&(!Number.isInteger(epsg)||epsg<=0||epsg>4294967295))throw Error("INVALID_CRS");
  if(assetKind==="cad"&&!window.confirm("Xác nhận DXF POINT/LINE XY dùng WGS84?"))return;
  if(asset){await unifiedAssets.close(opened.handle,asset.id);setAsset(undefined)}
  const a=await unifiedAssets.open(opened.handle,assetKind,epsg);if(a){setAsset(a);setAssetChunk("0");setAssetPreview(JSON.stringify(a,null,2));setRasterImage("");if(a.metadata)setAssetWindow("0,0,"+Math.min(256,a.metadata.width)+","+Math.min(256,a.metadata.height))}
 };
 const readAsset=async()=>{
  if(!opened||!asset||!map.current)return;
  const m=map.current;const b=m.getBounds();let q:AssetRead={};
  if(asset.kind==="cad")q={bbox:[Math.max(-180,b.getWest()),Math.max(-90,b.getSouth()),Math.min(180,b.getEast()),Math.min(90,b.getNorth())]};
  else if(asset.kind==="tiff"){
   const p=assetWindow.split(",").map(Number);if(p.length!==4||p.some(n=>!Number.isSafeInteger(n)||n<0)||p[2]===0||p[3]===0)throw Error("INVALID_WINDOW");
   q={pixel:p as [number,number,number,number],shape:[256,256],band:Number(band)};
  }else if(asset.kind==="copc"&&assetBounds.trim()){
   const a=assetBounds.split(",").map(Number);if(a.length!==6||a.some(n=>!Number.isFinite(n)))throw Error("INVALID_BOUNDS");q={bounds:a as [number,number,number,number,number,number],level:Number(assetLod)};
  }else{const n=Number(assetChunk);if(!Number.isInteger(n)||n<0||n>4294967295)throw Error("INVALID_CHUNK");q={chunk:n}}
  const raw=await unifiedAssets.read(opened.handle,asset.id,q);
  if(asset.kind==="cad"){
   const v=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(raw)) as GeoJSON.FeatureCollection;
   (m.getSource("assets") as GeoJSONSource).setData(v);setAssetPreview(v.features.length+" đối tượng CAD trong khung nhìn");
  }else if(["las","laz","copc"].includes(asset.kind)){
   if(raw.byteLength%24!==0)throw Error("INVALID_POINT_CHUNK");const d=new DataView(raw);const pts:GeoJSON.Feature[]=[];
   for(let i=0;i<raw.byteLength;i+=24){const x=d.getFloat64(i,true),y=d.getFloat64(i+8,true),z=d.getFloat64(i+16,true);
    if(asset.epsg===4326&&Math.abs(x)<=180&&Math.abs(y)<=90)pts.push({type:"Feature",geometry:{type:"Point",coordinates:[x,y]},properties:{z}})}
   (m.getSource("assets") as GeoJSONSource).setData({type:"FeatureCollection",features:pts});setAssetPreview(raw.byteLength/24+" điểm XYZ; chỉ hiển thị 2D khi EPSG:4326; "+pts.length+" điểm trên bản đồ");
  }else if(asset.kind==="tiff"){
   if(raw.byteLength!==256*256*4)throw Error("INVALID_RASTER");
   const d=new DataView(raw);let min=Infinity,max=-Infinity;for(let i=0;i<raw.byteLength;i+=4){const v=d.getFloat32(i,true);if(Number.isFinite(v)){min=Math.min(min,v);max=Math.max(max,v)}}
   const canvas=document.createElement("canvas");canvas.width=256;canvas.height=256;const ctx=canvas.getContext("2d");if(!ctx)throw Error("CANVAS_UNAVAILABLE");const img=ctx.createImageData(256,256);
   for(let i=0;i<raw.byteLength;i+=4){const v=d.getFloat32(i,true);const g=Number.isFinite(v)?Math.round(255*(v-min)/(max-min||1)):0;img.data[i]=g;img.data[i+1]=g;img.data[i+2]=g;img.data[i+3]=Number.isFinite(v)?255:0}
   ctx.putImageData(img,0,0);setRasterImage(canvas.toDataURL("image/png"));setAssetPreview("Window 256×256 · min "+min+" · max "+max+" (preview pixel, không phải lớp đã reproject)");
  }else setAssetPreview("GLB "+raw.byteLength+" byte; renderer 3D thuộc Wave 3");
 };
 const query=async(next:boolean)=>{
  if(!opened||!catalog||!layer||!map.current)return;
  const b=map.current.getBounds();const bbox:[number,number,number,number]=[Math.max(-180,b.getWest()),Math.max(-90,b.getSouth()),Math.min(180,b.getEast()),Math.min(90,b.getNorth())];
  const plan:UnifiedPlan={layer,bbox,filters:JSON.parse(filter),relation:relation?{relation,source:sourceId}:null};
  const r=await unified.query(opened.handle,plan,next?cursor:null,catalog.revision);
  setRows(r.data.features);setCursor(r.next);(map.current.getSource("features") as GeoJSONSource).setData(r.data);setMessage("Trang: "+r.data.features.length+" đối tượng; quét "+r.scanned+(r.next?" · còn trang":""));
 };
 const apply=async()=>{if(!opened||!catalog)return;const b=JSON.parse(batch) as Pick<UnifiedBatch,"edits"|"links">;const r=await unified.apply(opened.handle,{...b,operation:crypto.randomUUID(),expected_project:catalog.revision});await refresh(opened.handle);setMessage("Đã commit revision "+r.revision)};
 const option=(id:string)=>catalog?.layers.find(l=>l.id===id)?.name??id;
 return <section className="ovg-local-workbench">
 <div className="ovg-local-tools"><strong>Kho thống nhất · thử nghiệm (dự án mới)</strong>
 <button disabled={busy} onClick={()=>void run(async()=>{const p=await unified.create();setProjects(await unified.list());await open(p.id)})}>Tạo dự án ACID</button><button disabled={busy} onClick={()=>void run(async()=>{if(!window.confirm("Khôi phục hoặc nhập bản SQLite thống nhất vào dự án mới? Không ghi đè nguồn."))return;const p=await unified.restore();if(p){setProjects(await unified.list());await open(p.id)}})}>Khôi phục / nhập kho thống nhất</button>
 <select aria-label="Dự án ACID" value={opened?.id??""} disabled={busy} onChange={e=>{if(e.target.value)void run(()=>open(e.target.value))}}><option value="">Chọn dự án</option>{projects.map(p=><option key={p.id} value={p.id}>{p.id.slice(0,8)}</option>)}</select>
 {opened&&catalog&&<><span>Revision {catalog.revision}</span><button disabled={busy} onClick={()=>void run(async()=>{if(await unified.backup(opened.handle,catalog.revision))setMessage("Đã sao lưu kho thống nhất")})}>Sao lưu SQLite</button><button disabled={!ready||busy} aria-pressed={tileMode} onClick={()=>{if(!visibleLayers.length&&layer)setVisibleLayers([layer]);setTileMode(v=>!v)}}>MVT nhiều lớp</button>{tileMode&&catalog.layers.map(l=><label key={l.id}><input type="checkbox" checked={visibleLayers.includes(l.id)} onChange={()=>setVisibleLayers(v=>v.includes(l.id)?v.filter(x=>x!==l.id):[...v,l.id])}/>{l.name}</label>)}<button disabled={busy} onClick={()=>void run(()=>refresh(opened.handle))}>Đọc lại</button>
 <input aria-label="Tên lớp" value={name} onChange={e=>setName(e.target.value)}/><button disabled={busy||!name.trim()} onClick={()=>void run(async()=>{await unified.addLayer(opened.handle,name,catalog.revision);setName("");await refresh(opened.handle)})}>Tạo lớp</button>
 <select aria-label="Lớp truy vấn" value={layer} onChange={e=>{setLayer(e.target.value);setCursor(null)}}>{catalog.layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select>
 <button disabled={busy||!layer||!ready} onClick={()=>void run(()=>query(false))}>Truy vấn khung nhìn</button><button disabled={busy||!layer} onClick={()=>void run(async()=>{const r=await unified.importGeoJSON(opened.handle,layer,catalog.revision);if(r){await refresh(opened.handle);setMessage("Đã nhập GeoJSON, revision "+r.revision)}})}>Nhập GeoJSON (≤64)</button><button disabled={busy||!layer} onClick={()=>void run(async()=>{const n=await unified.exportGeoJSON(opened.handle,layer,catalog.revision);if(n!==null)setMessage("Đã xuất "+n+" đối tượng")})}>Xuất GeoJSON</button><button disabled={busy||!cursor} onClick={()=>void run(()=>query(true))}>Trang tiếp</button>
 <button disabled={busy} onClick={()=>void run(async()=>{if(!window.confirm("Bù trừ giao dịch feature/link cuối cùng?"))return;await unified.compensate(opened.handle,crypto.randomUUID(),catalog.revision);await refresh(opened.handle)})}>Undo / bù trừ</button></>}
 </div>
 <div ref={div} className="ovg-local-map" aria-label="Bản đồ kho thống nhất"/>
 {opened&&catalog&&<div className="ovg-local-table">
 <details><summary>Giao dịch nhiều lớp</summary><p>Layer ID: {catalog.layers.map(l=>l.name+": "+l.id).join(" · ")}</p><textarea aria-label="Batch edits links" rows={5} value={batch} onChange={e=>setBatch(e.target.value)}/><button disabled={busy} onClick={()=>void run(apply)}>Commit nguyên tử</button><button disabled={!layer||busy} onClick={()=>setBatch(JSON.stringify({edits:[{layer,feature:crypto.randomUUID(),expected:"0",geometry:{type:"Point",coordinates:[106.7,11]},attributes:{}}],links:[]},null,2))}>Mẫu thêm điểm</button></details>
 <details><summary>Quan hệ và Virtual Layer</summary><select aria-label="Lớp nguồn" value={source} onChange={e=>setSource(e.target.value)}><option value="">Nguồn</option>{catalog.layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select><select aria-label="Lớp đích" value={target} onChange={e=>setTarget(e.target.value)}><option value="">Đích</option>{catalog.layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select><select aria-label="Cardinality" value={kind} onChange={e=>setKind(e.target.value)}><option value="one_one">1–1</option><option value="one_many">1–N</option><option value="many_many">N–N</option></select><button disabled={busy||!source||!target} onClick={()=>void run(async()=>{await unified.addRelation(opened.handle,source,target,kind,catalog.revision);await refresh(opened.handle)})}>Tạo quan hệ</button>
 <select aria-label="Lọc theo quan hệ" value={relation} onChange={e=>{setRelation(e.target.value);setCursor(null)}}><option value="">Không lọc quan hệ</option>{catalog.relations.map(r=><option key={r.id} value={r.id}>{option(r.source)} → {option(r.target)} · {r.id}</option>)}</select><input aria-label="Feature ID nguồn" value={sourceId} onChange={e=>{setSourceId(e.target.value);setCursor(null)}}/><textarea aria-label="Bộ lọc JSON" rows={3} value={filter} onChange={e=>{setFilter(e.target.value);setCursor(null)}}/></details>
 <details><summary>Tài nguyên CAD / Point Cloud / Raster</summary>
 <select aria-label="Loại tài nguyên" value={assetKind} onChange={e=>setAssetKind(e.target.value as UnifiedAssetKind)}>{["cad","las","laz","copc","glb","mbtiles","tiff"].map(k=><option key={k} value={k} disabled={k==="tiff"&&!native?.ogr_linked}>{k.toUpperCase()}</option>)}</select>
 <label>EPSG điểm <input value={assetEpsg} onChange={e=>setAssetEpsg(e.target.value)} maxLength={10}/></label><button disabled={busy||!ready} onClick={()=>void run(loadAsset)}>Mở tài nguyên</button>
 {asset&&<><button disabled={busy} onClick={()=>void run(async()=>{await unifiedAssets.close(opened.handle,asset.id);setAsset(undefined);setAssetPreview("");setRasterImage("")})}>Đóng</button>
 {["las","laz","copc"].includes(asset.kind)&&<label>Khối điểm <input value={assetChunk} onChange={e=>setAssetChunk(e.target.value)} maxLength={10}/></label>}
 {asset.kind==="copc"&&<><label>BBox XYZ (trống: khối) <input value={assetBounds} onChange={e=>setAssetBounds(e.target.value)}/></label><label>LOD <input value={assetLod} onChange={e=>setAssetLod(e.target.value)} maxLength={2}/></label></>}
 {asset.kind==="tiff"&&<><label>Pixel x,y,w,h <input value={assetWindow} onChange={e=>setAssetWindow(e.target.value)}/></label><label>Band <input value={band} onChange={e=>setBand(e.target.value)} maxLength={3}/></label></>}
 {asset.kind!=="mbtiles"&&<button disabled={busy||!ready} onClick={()=>void run(readAsset)}>Đọc khung / khối</button>}
 <pre>{assetPreview}</pre>{rasterImage&&<img src={rasterImage} alt="Mẫu cửa sổ pixel GeoTIFF" width={256} height={256}/>}</>}
 </details>
 <details><summary>Native Data Providers (GDAL / PROJ / DuckDB)</summary>
 <p>GDAL/PROJ: {native?.ogr_linked?"đã liên kết · thử nghiệm":"chưa liên kết"} · DuckDB: {native?.analytics_linked?"đã liên kết · thử nghiệm":"chưa liên kết"}</p>
 <label>Chỉ số lớp OGR <input value={ogrIndex} onChange={e=>setOgrIndex(e.target.value)} maxLength={2}/></label><button disabled={busy||!native?.ogr_linked||!layer} onClick={()=>void run(async()=>{const i=Number(ogrIndex);if(!Number.isInteger(i)||i<0||i>=64)throw Error("INVALID_LAYER");const r=await nativeProviders.importOGR(opened.handle,layer,i,catalog.revision);if(r){await refresh(opened.handle);setMessage("Đã nhập OGR revision "+r.revision)}})}>Nhập SHP / FGB / GeoParquet</button><button disabled={busy||!native?.ogr_linked||!layer} onClick={()=>void run(async()=>{const n=await unifiedAssets.exportParquet(opened.handle,layer,catalog.revision);if(n!==null)setMessage("Đã xuất "+n+" đối tượng GeoParquet")})}>Xuất GeoParquet</button>
 <label>EPSG nguồn <input value={fromCrs} onChange={e=>setFromCrs(e.target.value)} maxLength={10}/></label><label>EPSG đích <input value={toCrs} onChange={e=>setToCrs(e.target.value)} maxLength={10}/></label><textarea aria-label="Tọa độ XY JSON" value={xyText} onChange={e=>setXyText(e.target.value)} rows={2}/><button disabled={busy||!native?.ogr_linked} onClick={()=>void run(async()=>{const a=Number(fromCrs),b=Number(toCrs),xy=JSON.parse(xyText);if(![a,b].every(n=>Number.isInteger(n)&&n>0&&n<=4294967295)||!Array.isArray(xy)||xy.length>4096)throw Error("INVALID_COORDINATES");setNativeOutput(JSON.stringify(await nativeProviders.transform(opened.handle,a,b,xy),null,2))})}>Chuyển CRS PROJ</button>
 <label>Band TIFF <input value={band} onChange={e=>setBand(e.target.value)} maxLength={3}/></label><button disabled={busy||!native?.ogr_linked} onClick={()=>void run(async()=>{const n=Number(band);if(!Number.isInteger(n)||n<1||n>256)throw Error("INVALID_BAND");const r=await nativeProviders.rasterPreview(opened.handle,n);if(r){const {values,...metadata}=r;setNativeOutput(JSON.stringify({...metadata,valid_samples:values.filter(x=>x!==null).length,sample_first_16:values.slice(0,16)},null,2))}})}>Đọc mẫu GeoTIFF</button>
 <button disabled={busy||!layer||!native?.analytics_linked} onClick={()=>void run(async()=>setNativeOutput(JSON.stringify(await nativeProviders.pointSummary(opened.handle,layer,catalog.revision,false),null,2)))}>DuckDB thống kê điểm</button>
 <button disabled={busy||!layer||!native?.analytics_linked} onClick={()=>void run(async()=>setNativeOutput(JSON.stringify(await nativeProviders.pointSummary(opened.handle,layer,catalog.revision,true),null,2)))}>DuckDB Spatial (extension cục bộ)</button>
 {nativeOutput&&<pre>{nativeOutput}</pre>}</details>
 <details><summary>Schema thuộc tính</summary><button disabled={!layer} onClick={()=>setSchema(JSON.stringify(catalog.layers.find(l=>l.id===layer)?.fields??[],null,2))}>Đọc schema</button><textarea aria-label="Schema JSON" rows={4} value={schema} onChange={e=>setSchema(e.target.value)}/><button disabled={busy||!layer} onClick={()=>void run(async()=>{await unified.schema(opened.handle,layer,JSON.parse(schema) as AttributeField[],catalog.revision);await refresh(opened.handle)})}>Áp dụng</button></details>
 <table><thead><tr><th>Feature ID</th><th>Revision / thuộc tính</th><th>Chỉnh sửa</th></tr></thead><tbody>{rows.map(r=><tr key={String(r.id)}><td>{String(r.id)}</td><td>{JSON.stringify(r.properties)}</td><td><button onClick={()=>setBatch(JSON.stringify({edits:[{layer,feature:r.id,expected:r.properties?.revision,geometry:r.geometry,attributes:r.properties?.attributes}],links:[]},null,2))}>Đưa vào batch</button></td></tr>)}</tbody></table>
 </div>}
 <div role="status">{message}</div>
 </section>
}
