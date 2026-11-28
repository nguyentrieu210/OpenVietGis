import {useEffect,useRef,useState} from "react";
import {unified,type Opened,type Project,type UnifiedCatalog,type UnifiedPlan,type UnifiedCursor,type UnifiedBatch,type AttributeField} from "./transport";
import maplibregl,{type GeoJSONSource} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
export default function UnifiedWorkbench(){
 const [projects,setProjects]=useState<Project[]>([]),[opened,setOpened]=useState<Opened>(),[catalog,setCatalog]=useState<UnifiedCatalog>();
 const [layer,setLayer]=useState(""),[name,setName]=useState(""),[source,setSource]=useState(""),[target,setTarget]=useState(""),[kind,setKind]=useState("one_many");
 const [batch,setBatch]=useState('{"edits":[],"links":[]}'),[filter,setFilter]=useState("[]"),[relation,setRelation]=useState(""),[sourceId,setSourceId]=useState("");
 const [schema,setSchema]=useState("[]"),[cursor,setCursor]=useState<UnifiedCursor|null>(null),[rows,setRows]=useState<GeoJSON.Feature[]>([]);
 const [tileMode,setTileMode]=useState(false),[visibleLayers,setVisibleLayers]=useState<string[]>([]);
 const [message,setMessage]=useState(""),[busy,setBusy]=useState(false),[ready,setReady]=useState(false);
 const active=useRef<Opened|undefined>(undefined),locked=useRef(false),alive=useRef(true),div=useRef<HTMLDivElement>(null),map=useRef<maplibregl.Map|undefined>(undefined);
 const refresh=async(h:string)=>{const c=await unified.catalog(h);if(alive.current){setCatalog(c);setVisibleLayers(v=>v.filter(id=>c.layers.some(l=>l.id===id)));setLayer(l=>c.layers.some(x=>x.id===l)?l:c.layers[0]?.id??"");setCursor(null);setRows([]);(map.current?.getSource("features") as GeoJSONSource|undefined)?.setData({type:"FeatureCollection",features:[]})}};
 const run=async(f:()=>Promise<void>)=>{if(locked.current)return;locked.current=true;setBusy(true);setMessage("");try{await f()}catch(e){if(alive.current)setMessage(String(e))}finally{locked.current=false;if(alive.current)setBusy(false)}};
 const open=async(id:string)=>{if(active.current?.id===id)return;if(active.current){await unified.close(active.current.handle);active.current=undefined;setOpened(undefined);setCatalog(undefined)}const w=await unified.open(id);if(!alive.current){await unified.close(w.handle);return}active.current=w;setOpened(w);setVisibleLayers([]);await refresh(w.handle)};
 useEffect(()=>{alive.current=true;void run(async()=>setProjects(await unified.list()));return()=>{alive.current=false;if(active.current)void unified.close(active.current.handle).catch(()=>{})}},[]);
 useEffect(()=>{if(!div.current)return;
 maplibregl.addProtocol("ovgu",async(params,abort)=>{
  const m=/^ovgu:\/\/v1\/([0-9a-f-]{36})\/([0-9a-f-]{36})\/([0-9]+)\/([0-9]+)\/([0-9]+)\/([0-9]+)\.mvt$/.exec(params.url);
  if(!m)throw Error("INVALID_TILE_URI");
  return {data:await unified.tile(m[1]!,m[2]!,m[3]!,Number(m[4]),Number(m[5]),Number(m[6]),abort.signal)};
 });
 const m=new maplibregl.Map({container:div.current,style:{version:8,sources:{},layers:[{id:"background",type:"background",paint:{"background-color":"#eef2f4"}}]},center:[106.7,11],zoom:6,renderWorldCopies:false});map.current=m;m.on("load",()=>{m.addSource("features",{type:"geojson",data:{type:"FeatureCollection",features:[]}});m.addLayer({id:"fill",source:"features",type:"fill",filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":"#1976a3","fill-opacity":.25}});m.addLayer({id:"line",source:"features",type:"line",filter:["in",["geometry-type"],["literal",["Polygon","LineString"]]],paint:{"line-color":"#1976a3","line-width":2}});m.addLayer({id:"point",source:"features",type:"circle",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":"#1976a3","circle-radius":5}});setReady(true)});m.on("error",e=>{if(alive.current)setMessage(String(e.error))});m.addControl(new maplibregl.NavigationControl());return()=>{map.current=undefined;m.remove();maplibregl.removeProtocol("ovgu")}},[]);
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
 <button disabled={busy||!layer||!ready} onClick={()=>void run(()=>query(false))}>Truy vấn khung nhìn</button><button disabled={busy||!cursor} onClick={()=>void run(()=>query(true))}>Trang tiếp</button>
 <button disabled={busy} onClick={()=>void run(async()=>{if(!window.confirm("Bù trừ giao dịch feature/link cuối cùng?"))return;await unified.compensate(opened.handle,crypto.randomUUID(),catalog.revision);await refresh(opened.handle)})}>Undo / bù trừ</button></>}
 </div>
 <div ref={div} className="ovg-local-map" aria-label="Bản đồ kho thống nhất"/>
 {opened&&catalog&&<div className="ovg-local-table">
 <details><summary>Giao dịch nhiều lớp</summary><p>Layer ID: {catalog.layers.map(l=>l.name+": "+l.id).join(" · ")}</p><textarea aria-label="Batch edits links" rows={5} value={batch} onChange={e=>setBatch(e.target.value)}/><button disabled={busy} onClick={()=>void run(apply)}>Commit nguyên tử</button><button disabled={!layer||busy} onClick={()=>setBatch(JSON.stringify({edits:[{layer,feature:crypto.randomUUID(),expected:"0",geometry:{type:"Point",coordinates:[106.7,11]},attributes:{}}],links:[]},null,2))}>Mẫu thêm điểm</button></details>
 <details><summary>Quan hệ và Virtual Layer</summary><select aria-label="Lớp nguồn" value={source} onChange={e=>setSource(e.target.value)}><option value="">Nguồn</option>{catalog.layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select><select aria-label="Lớp đích" value={target} onChange={e=>setTarget(e.target.value)}><option value="">Đích</option>{catalog.layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select><select aria-label="Cardinality" value={kind} onChange={e=>setKind(e.target.value)}><option value="one_one">1–1</option><option value="one_many">1–N</option><option value="many_many">N–N</option></select><button disabled={busy||!source||!target} onClick={()=>void run(async()=>{await unified.addRelation(opened.handle,source,target,kind,catalog.revision);await refresh(opened.handle)})}>Tạo quan hệ</button>
 <select aria-label="Lọc theo quan hệ" value={relation} onChange={e=>{setRelation(e.target.value);setCursor(null)}}><option value="">Không lọc quan hệ</option>{catalog.relations.map(r=><option key={r.id} value={r.id}>{option(r.source)} → {option(r.target)} · {r.id}</option>)}</select><input aria-label="Feature ID nguồn" value={sourceId} onChange={e=>{setSourceId(e.target.value);setCursor(null)}}/><textarea aria-label="Bộ lọc JSON" rows={3} value={filter} onChange={e=>{setFilter(e.target.value);setCursor(null)}}/></details>
 <details><summary>Schema thuộc tính</summary><button disabled={!layer} onClick={()=>setSchema(JSON.stringify(catalog.layers.find(l=>l.id===layer)?.fields??[],null,2))}>Đọc schema</button><textarea aria-label="Schema JSON" rows={4} value={schema} onChange={e=>setSchema(e.target.value)}/><button disabled={busy||!layer} onClick={()=>void run(async()=>{await unified.schema(opened.handle,layer,JSON.parse(schema) as AttributeField[],catalog.revision);await refresh(opened.handle)})}>Áp dụng</button></details>
 <table><thead><tr><th>Feature ID</th><th>Revision / thuộc tính</th><th>Chỉnh sửa</th></tr></thead><tbody>{rows.map(r=><tr key={String(r.id)}><td>{String(r.id)}</td><td>{JSON.stringify(r.properties)}</td><td><button onClick={()=>setBatch(JSON.stringify({edits:[{layer,feature:r.id,expected:r.properties?.revision,geometry:r.geometry,attributes:r.properties?.attributes}],links:[]},null,2))}>Đưa vào batch</button></td></tr>)}</tbody></table>
 </div>}
 <div role="status">{message}</div>
 </section>
}
