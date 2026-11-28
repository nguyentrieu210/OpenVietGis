import {useCallback,useEffect,useRef,useState} from "react";
import maplibregl,{type GeoJSONSource} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import {desktop,type FeatureRecord,type FeatureRow} from "./transport";

type Kind="point"|"line"|"polygon";
const empty=():GeoJSON.FeatureCollection=>({type:"FeatureCollection",features:[]});
export default function LocalWorkbench({handle}:{handle:string}){
  const canvas=useRef<HTMLDivElement>(null);
  const map=useRef<maplibregl.Map|undefined>(undefined);
  const [ready,setReady]=useState(false),[busy,setBusy]=useState(false),[notice,setNotice]=useState("");
  const [rows,setRows]=useState<FeatureRow[]>([]),[cursor,setCursor]=useState("0"),[filter,setFilter]=useState("");
  const [selected,setSelected]=useState<FeatureRecord>(),[attrs,setAttrs]=useState("{}"),[geometryText,setGeometryText]=useState("");
  const [tiles,setTiles]=useState(false);const tileMode=useRef(false);
  const [raster,setRaster]=useState<{id:string;attribution:string}>();
  const styleReady=useRef(false);
  const [kind,setKind]=useState<Kind>(),[count,setCount]=useState(0);
  const draft=useRef<number[]>([]),mode=useRef<Kind|undefined>(undefined),alive=useRef(true);
  const sequence=useRef(0),pending=useRef(false),dirty=useRef(false),working=useRef(false);
  const dataSource=()=>map.current?.getSource("local") as GeoJSONSource|undefined;
  const draftSource=()=>map.current?.getSource("draft") as GeoJSONSource|undefined;
  const refreshMap=useCallback(async()=>{
    dirty.current=true;
    if(pending.current)return;
    pending.current=true;
    try{
      while(dirty.current&&alive.current){
        dirty.current=false;const id=sequence.current;
        const m=map.current;if(!m||!styleReady.current)return;
        if(tileMode.current)return;
        const b=m.getBounds();
        const west=Math.max(-180,b.getWest()),east=Math.min(180,b.getEast());
        const south=Math.max(-90,b.getSouth()),north=Math.min(90,b.getNorth());
        if(west>east||south>north||b.getWest() < -180||b.getEast()>180){
          dataSource()?.setData(empty());setNotice("Khung nhìn vượt phạm vi kinh độ hỗ trợ; hãy thu phóng vào vùng dữ liệu.");continue;
        }
        const v=await desktop.viewport(handle,[west,south,east,north]);
        if(!alive.current||id!==sequence.current||dirty.current)continue;
        dataSource()?.setData(v.data);
        setNotice(v.truncated?"Khung nhìn vượt giới hạn 512 đối tượng/1 MiB; hãy phóng to.":"Dữ liệu cục bộ · EPSG:4326");
      }
    }catch(e){if(alive.current)setNotice(String(e))}
    finally{pending.current=false}
  },[handle]);
  const invalidate=useCallback(()=>{sequence.current++;void refreshMap()},[refreshMap]);
  const page=useCallback(async(after="0",term="")=>{
    const v=await desktop.attributes(handle,after,100,term);
    if(alive.current){setRows(v.rows);setCursor(v.rows.at(-1)?.fid??after)}
  },[handle]);
  const select=useCallback(async(id:string)=>{
    const f=await desktop.feature(handle,id);
    if(alive.current){setSelected(f);setAttrs(JSON.stringify(f.attributes,null,2));setGeometryText(JSON.stringify(f.geometry,null,2))}
  },[handle]);
  const run=useCallback(async(f:()=>Promise<void>)=>{
    if(working.current)return;
    working.current=true;setBusy(true);
    try{await f()}catch(e){if(alive.current)setNotice(String(e))}
    finally{working.current=false;if(alive.current)setBusy(false)}
  },[]);
  const clearDraft=()=>{draft.current=[];mode.current=undefined;setKind(undefined);setCount(0);draftSource()?.setData(empty())};
  const begin=(k:Kind)=>{clearDraft();mode.current=k;setKind(k)};
  const setVectorTiles=async(on:boolean)=>{
    const m=map.current;if(!m||!styleReady.current)return;
    for(const suffix of ["fill","line","point"])if(m.getLayer("mvt-"+suffix))m.removeLayer("mvt-"+suffix);
    if(m.getSource("mvt"))m.removeSource("mvt");
    tileMode.current=on;setTiles(on);sequence.current++;
    for(const suffix of ["fill","line","point"])m.setLayoutProperty("local-"+suffix,"visibility",on?"none":"visible");
    if(!on){invalidate();return}
    const rev=await desktop.generation(handle);
    if(!alive.current||!tileMode.current)return;
    m.addSource("mvt",{type:"vector",tiles:[`ovg://v1/workspaces/${handle}/layers/${handle}/revisions/${rev}/tiles/{z}/{x}/{y}.mvt`],minzoom:0,maxzoom:22,promoteId:"stable_id"});
    const layer={"source":"mvt","source-layer":"local"} as const;
    m.addLayer({id:"mvt-fill",...layer,type:"fill",filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":"#1976a3","fill-opacity":0.25}},"draft-fill");
    m.addLayer({id:"mvt-line",...layer,type:"line",filter:["in",["geometry-type"],["literal",["LineString","Polygon"]]],paint:{"line-color":"#1976a3","line-width":2}},"draft-fill");
    m.addLayer({id:"mvt-point",...layer,type:"circle",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":"#1976a3","circle-radius":5}},"draft-fill");
    setNotice("MVT · Tile theo revision "+rev);
  };
  const afterMutation=async()=>{setSelected(undefined);setAttrs("{}");setGeometryText("");await page("0",filter);if(tileMode.current)await setVectorTiles(true);else invalidate()};
  useEffect(()=>{
    alive.current=true;styleReady.current=false;
    maplibregl.addProtocol("ovg",async(params,abort)=>{
      if(abort.signal.aborted)throw new DOMException("Aborted","AbortError");
      const data=await desktop.resource(params.url);
      if(abort.signal.aborted)throw new DOMException("Aborted","AbortError");
      return {data};
    });
    if(!canvas.current)return;
    let m:maplibregl.Map|undefined;
    try{
      m=new maplibregl.Map({container:canvas.current,style:{version:8,sources:{},layers:[{id:"background",type:"background",paint:{"background-color":"#e8edf0"}}]},center:[106.7,11],zoom:5,renderWorldCopies:false,attributionControl:false});
      map.current=m;m.addControl(new maplibregl.NavigationControl(),"top-right");
      m.on("load",()=>{
        if(!m)return;
        m.addSource("local",{type:"geojson",data:empty()});m.addSource("draft",{type:"geojson",data:empty()});
        for(const src of ["local","draft"]){
          const color=src==="local"?"#1976a3":"#d77718";
          m.addLayer({id:src+"-fill",source:src,type:"fill",filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":color,"fill-opacity":0.25}});
          m.addLayer({id:src+"-line",source:src,type:"line",filter:["in",["geometry-type"],["literal",["LineString","Polygon"]]],paint:{"line-color":color,"line-width":2}});
          m.addLayer({id:src+"-point",source:src,type:"circle",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":color,"circle-radius":5,"circle-stroke-color":"#fff","circle-stroke-width":1}});
        }
        styleReady.current=true;setReady(true);invalidate();
      });
      m.on("moveend",invalidate);
      m.on("click",e=>{
        if(!m||!styleReady.current||working.current)return;
        const k=mode.current;
        if(k){
          if(draft.current.length>=8190||(k==="point"&&draft.current.length))return;
          draft.current.push(e.lngLat.lng,e.lngLat.lat);setCount(draft.current.length/2);
          const xy:number[][]=[];for(let i=0;i<draft.current.length;i+=2)xy.push([draft.current[i]!,draft.current[i+1]!]);
          const geometry:GeoJSON.Geometry=xy.length===1?{type:"Point",coordinates:xy[0]!}:k==="polygon"&&xy.length>=3?{type:"Polygon",coordinates:[[...xy,xy[0]!]]}:{type:"LineString",coordinates:xy};
          draftSource()?.setData({type:"FeatureCollection",features:[{type:"Feature",geometry,properties:{}}]});
          return;
        }
        const layers=(tileMode.current?["mvt-point","mvt-line","mvt-fill"]:["local-point","local-line","local-fill"]).filter(l=>!!m?.getLayer(l));
        if(!layers.length)return;
        const hits=m.queryRenderedFeatures(e.point,{layers});
        if(typeof hits[0]?.id==="string")void run(()=>select(hits[0]!.id as string));
      });
      m.on("error",e=>{if(alive.current)setNotice(String(e.error))});
      void run(()=>page());
    }catch(e){setNotice(String(e))}
    return()=>{alive.current=false;sequence.current++;dirty.current=false;styleReady.current=false;map.current=undefined;m?.remove();maplibregl.removeProtocol("ovg")};
  },[handle,invalidate,page,run,select]);
  const openRaster=()=>run(async()=>{
    const m=map.current;if(!m||!styleReady.current)return;
    const a=await desktop.openRaster(handle);if(!a)return;
    if(m.getLayer("offline-raster"))m.removeLayer("offline-raster");
    if(m.getSource("offline-raster"))m.removeSource("offline-raster");
    m.addSource("offline-raster",{type:"raster",tiles:[`ovg://v1/workspaces/${handle}/rasters/${a.id}/revisions/0/tiles/{z}/{x}/{y}.png`],tileSize:256,minzoom:0,maxzoom:22});
    m.addLayer({id:"offline-raster",type:"raster",source:"offline-raster"},"local-fill");setRaster(a);
  });
  const closeRaster=()=>run(async()=>{
    const m=map.current;
    if(m?.getLayer("offline-raster"))m.removeLayer("offline-raster");
    if(m?.getSource("offline-raster"))m.removeSource("offline-raster");
    await desktop.closeRaster(handle);setRaster(undefined);
  });
  const commit=()=>run(async()=>{
    const k=mode.current;if(!k)return;
    await desktop.edit(handle,{command:crypto.randomUUID(),id:crypto.randomUUID(),revision:"0",kind:k,xy:[...draft.current],attributes:{}});
    clearDraft();await afterMutation();
  });
  const update=()=>run(async()=>{
    if(!selected)return;const a:unknown=JSON.parse(attrs);
    if(typeof a!=="object"||a===null||Array.isArray(a))throw Error("Thuộc tính phải là JSON object");
    await desktop.updateAttributes(handle,selected.id,selected.revision,a as Record<string,unknown>);
    await afterMutation();
  });
  const reshape=()=>run(async()=>{
    if(!selected)return;
    const g:unknown=JSON.parse(geometryText);
    if(!g||typeof g!=="object")throw Error("Hình học không hợp lệ");
    await desktop.reshape(handle,selected.id,selected.revision,g);
    await afterMutation();
  });
  const remove=()=>run(async()=>{
    if(!selected||!window.confirm("Xóa đối tượng đã chọn? Có thể hoàn tác."))return;
    await desktop.edit(handle,{command:crypto.randomUUID(),id:selected.id,revision:selected.revision,kind:null,xy:[],attributes:{}});
    await afterMutation();
  });
  return <section className="ovg-local-workbench">
    <div className="ovg-local-tools" role="toolbar" aria-label="Công cụ GIS cục bộ">
      <button disabled={busy||!ready} aria-pressed={tiles} onClick={()=>void run(()=>setVectorTiles(!tiles))}>MVT vector tile</button>
      <button disabled={busy} onClick={()=>void run(async()=>{const n=await desktop.importGPKG(handle);if(n!==null)await afterMutation()})}>Nhập GPKG XY</button>
      <button disabled={busy||!ready} onClick={()=>void openRaster()}>Mở MBTiles PNG</button>
      <button disabled={busy||!raster} onClick={()=>void closeRaster()}>Đóng raster</button>
      <button disabled={busy} onClick={()=>void run(async()=>{const n=await desktop.importGeoJSON(handle);if(n!==null)await afterMutation()})}>Nhập GeoJSON</button>
      <button disabled={busy} onClick={()=>void run(async()=>{await desktop.exportFile(handle,"gpkg")})}>Lưu bản sao GPKG</button>
      <button disabled={busy} onClick={()=>void run(async()=>{await desktop.exportFile(handle,"geojson")})}>Xuất GeoJSON</button>
      <button disabled={busy} onClick={()=>void run(async()=>{await desktop.checkpoint(handle);setNotice("Đã checkpoint")})}>Lưu</button>
      {(["point","line","polygon"] as const).map(k=><button key={k} aria-pressed={kind===k} disabled={!ready||busy} onClick={()=>begin(k)}>{{point:"Điểm",line:"Đường",polygon:"Vùng"}[k]}</button>)}
      <button disabled={busy||!kind||count<(kind==="polygon"?3:kind==="line"?2:1)} onClick={()=>void commit()}>Ghi hình học</button>
      <button disabled={busy||!kind} onClick={clearDraft}>Hủy vẽ</button>
      <button disabled={busy} onClick={()=>void run(async()=>{await desktop.history(handle,false);await afterMutation()})}>Undo</button>
      <button disabled={busy} onClick={()=>void run(async()=>{await desktop.history(handle,true);await afterMutation()})}>Redo</button>
    </div>
    <div ref={canvas} className="ovg-local-map" aria-label="Bản đồ dữ liệu cục bộ"/>
    <div role="status" className="ovg-local-status">{notice}{raster&&" · Nguồn: "+raster.attribution}{kind&&" · Nhấp bản đồ để vẽ: "+count+" đỉnh"}</div>
    <div className="ovg-local-table">
      <div><label>Tìm thuộc tính <input value={filter} maxLength={128} onChange={e=>setFilter(e.target.value)}/></label>
      <button disabled={busy} onClick={()=>void run(()=>page("0",filter))}>Trang đầu</button>
      <button disabled={busy||!rows.length} onClick={()=>void run(()=>page(cursor,filter))}>Trang tiếp</button></div>
      <div className="ovg-local-rows"><table><thead><tr><th>ID</th><th>Revision</th><th>Thuộc tính</th></tr></thead><tbody>{rows.map(r=><tr key={r.id}><td><button disabled={busy} onClick={()=>void run(()=>select(r.id))}>{r.id.slice(0,8)}</button></td><td>{r.revision}</td><td>{JSON.stringify(r.attributes).slice(0,200)}</td></tr>)}</tbody></table></div>
      {selected&&<div><label>Thuộc tính {selected.id}<textarea rows={3} value={attrs} onChange={e=>setAttrs(e.target.value)}/></label><button disabled={busy} onClick={()=>void update()}>Ghi thuộc tính</button><label>Tọa độ hình học GeoJSON<textarea rows={4} value={geometryText} onChange={e=>setGeometryText(e.target.value)}/></label><button disabled={busy} onClick={()=>void reshape()}>Ghi sửa hình học</button><button disabled={busy} onClick={()=>void remove()}>Xóa đối tượng</button></div>}
    </div>
  </section>;
}
