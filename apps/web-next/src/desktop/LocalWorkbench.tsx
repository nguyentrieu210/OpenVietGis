import {useCallback,useEffect,useRef,useState} from "react";
import maplibregl,{type GeoJSONSource} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import {desktop,type FeatureRecord,type FeatureRow,type AssetInfo} from "./transport";

import {vertices,moveVertex} from "./vertices";
type Kind="point"|"line"|"polygon";
const empty=():GeoJSON.FeatureCollection=>({type:"FeatureCollection",features:[]});
type Overlay={id:string;handle:string;name:string};
export default function LocalWorkbench({handle,overlays=[]}:{handle:string;overlays?:Overlay[]}){
  const canvas=useRef<HTMLDivElement>(null);
  const map=useRef<maplibregl.Map|undefined>(undefined);
  const [ready,setReady]=useState(false),[busy,setBusy]=useState(false),[notice,setNotice]=useState("");
  const [rows,setRows]=useState<FeatureRow[]>([]),[cursor,setCursor]=useState<string|null>("0"),[filter,setFilter]=useState("");
  const [selected,setSelected]=useState<FeatureRecord>(),[attrs,setAttrs]=useState("{}"),[geometryText,setGeometryText]=useState("");
  const [catalog,setCatalog]=useState<Awaited<ReturnType<typeof desktop.inspectGPKG>>>(null),[table,setTable]=useState("");
  const [vertexMode,setVertexMode]=useState(false);
  const vertexMarkers=useRef<maplibregl.Marker[]>([]);
  const vertexGeometry=useRef<Exclude<GeoJSON.Geometry,GeoJSON.GeometryCollection>|undefined>(undefined);
  const [tiles,setTiles]=useState(false);const tileMode=useRef(false);
  const [raster,setRaster]=useState<{id:string;attribution:string}>();
  const styleReady=useRef(false);
  const [asset,setAsset]=useState<AssetInfo>(),[assetPreview,setAssetPreview]=useState("");
  const cad=useRef<AssetInfo|undefined>(undefined);
  const [lasEpsg,setLasEpsg]=useState("4326");
  const [copcBounds,setCopcBounds]=useState("0,0,0,1,1,1"),[copcLevel,setCopcLevel]=useState("0"),[pointPage,setPointPage]=useState(0);
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
        const b=m.getBounds();
        const west=Math.max(-180,b.getWest()),east=Math.min(180,b.getEast());
        const south=Math.max(-90,b.getSouth()),north=Math.min(90,b.getNorth());
        if(west>east||south>north||b.getWest() < -180||b.getEast()>180){
          dataSource()?.setData(empty());setNotice("Khung nhìn vượt phạm vi kinh độ hỗ trợ; hãy thu phóng vào vùng dữ liệu.");continue;
        }
        const currentCad=cad.current;
        if(currentCad){
          const v=await desktop.cadViewport(handle,currentCad.id,[west,south,east,north]);
          if(!alive.current||id!==sequence.current||dirty.current)continue;
          (m.getSource("cad") as GeoJSONSource|undefined)?.setData(v);
        }
        if(tileMode.current)continue;
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
    if(alive.current){setRows(v.rows);setCursor(v.next)}
  },[handle]);
  const select=useCallback(async(id:string)=>{
    const f=await desktop.feature(handle,id);
    if(alive.current){setVertexMode(false);setSelected(f);setAttrs(JSON.stringify(f.attributes,null,2));setGeometryText(JSON.stringify(f.geometry,null,2))}
  },[handle]);
  const run=useCallback(async(f:()=>Promise<void>)=>{
    if(working.current)return;
    working.current=true;setBusy(true);
    try{await f()}catch(e){if(alive.current)setNotice(String(e))}
    finally{working.current=false;if(alive.current)setBusy(false)}
  },[]);
  const clearDraft=()=>{draft.current=[];mode.current=undefined;setKind(undefined);setCount(0);draftSource()?.setData(empty())};
  const begin=(k:Kind)=>{setVertexMode(false);clearDraft();mode.current=k;setKind(k)};
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
  const afterMutation=async()=>{setVertexMode(false);setSelected(undefined);setAttrs("{}");setGeometryText("");await page("0",filter);if(tileMode.current)await setVectorTiles(true);else invalidate()};
  useEffect(()=>{
    alive.current=true;styleReady.current=false;
    maplibregl.addProtocol("ovg",async(params,abort)=>{
      if(abort.signal.aborted)throw new DOMException("Aborted","AbortError");
      const data=await desktop.resource(params.url,abort.signal);
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
        m.addSource("local",{type:"geojson",data:empty()});m.addSource("draft",{type:"geojson",data:empty()});m.addSource("cad",{type:"geojson",data:empty()});
        for(const src of ["local","cad","draft"]){
          const color=src==="local"?"#1976a3":src==="cad"?"#7048ab":"#d77718";
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
  useEffect(()=>{
    const m=map.current;
    if(!m||!ready||!selected||!vertexMode)return;
    try{
      const g=JSON.parse(geometryText) as Exclude<GeoJSON.Geometry,GeoJSON.GeometryCollection>;
      vertexGeometry.current=g;
      const draw=()=>{if(vertexGeometry.current) (m.getSource("draft") as GeoJSONSource|undefined)?.setData({type:"FeatureCollection",features:[{type:"Feature",geometry:vertexGeometry.current,properties:{}}]})};
      for(const v of vertices(g)){
        const element=document.createElement("button");element.type="button";element.className="ovg-vertex";element.title="Kéo đỉnh";element.setAttribute("aria-label","Kéo đỉnh "+v.path.join("."));
        element.addEventListener("click",e=>e.stopPropagation());
        const marker=new maplibregl.Marker({element,draggable:true}).setLngLat(v.xy).addTo(m);
        marker.on("drag",()=>{try{const p=marker.getLngLat();if(vertexGeometry.current){vertexGeometry.current=moveVertex(vertexGeometry.current,v,[p.lng,p.lat]);draw()}}catch(e){setNotice(String(e))}});
        marker.on("dragend",()=>{if(vertexGeometry.current)setGeometryText(JSON.stringify(vertexGeometry.current,null,2))});
        vertexMarkers.current.push(marker);
      }
      draw();
    }catch(e){setNotice(String(e));setVertexMode(false)}
    return()=>{for(const marker of vertexMarkers.current)marker.remove();vertexMarkers.current=[];vertexGeometry.current=undefined;if(map.current===m)(m.getSource("draft") as GeoJSONSource|undefined)?.setData(empty())};
    // Geometry text updates on drag end, rebuilding marker coordinates.
  },[vertexMode,selected,geometryText,ready]);
  useEffect(()=>{
    const m=map.current;
    if(!ready||!m||!styleReady.current)return;
    let disposed=false;
    const installed:string[]=[];
    void (async()=>{
      for(const [i,o] of overlays.entries()){
        const generation=await desktop.generation(o.handle);
        if(disposed||!alive.current)return;
        const id="overlay-"+o.handle;
        const url=`ovg://v1/workspaces/${o.handle}/layers/${o.handle}/revisions/${generation}/tiles/{z}/{x}/{y}.mvt`;
        m.addSource(id,{type:"vector",tiles:[url],minzoom:0,maxzoom:22,promoteId:"stable_id"});
        installed.push(id);
        const color=["#6b7280","#22875e","#a254a1","#b68924"][i%4]!;
        const l={source:id,"source-layer":"local"} as const;
        m.addLayer({id:id+"-fill",...l,type:"fill",filter:["==",["geometry-type"],"Polygon"],paint:{"fill-color":color,"fill-opacity":0.16}},"local-fill");
        m.addLayer({id:id+"-line",...l,type:"line",filter:["in",["geometry-type"],["literal",["LineString","Polygon"]]],paint:{"line-color":color,"line-width":1.5}},"local-fill");
        m.addLayer({id:id+"-point",...l,type:"circle",filter:["==",["geometry-type"],"Point"],paint:{"circle-color":color,"circle-radius":4}},"local-fill");
      }
    })().catch(e=>{if(!disposed&&alive.current)setNotice(String(e))});
    return()=>{
      disposed=true;
      if(map.current!==m)return;
      for(const id of installed){
        for(const suffix of ["fill","line","point"])if(m.getLayer(id+"-"+suffix))m.removeLayer(id+"-"+suffix);
        if(m.getSource(id))m.removeSource(id);
      }
    };
  },[ready,overlays]);
  const openAsset=(kind:"cad"|"las"|"laz"|"copc"|"glb")=>run(async()=>{
    if(asset)throw Error("Đóng tài nguyên hiện tại trước khi mở tài nguyên khác");
    const epsg=kind==="cad"?4326:kind==="glb"?null:Number(lasEpsg);
    if(kind==="cad"&&!window.confirm("Xác nhận DXF POINT/LINE sử dụng tọa độ XY WGS84, không phải hệ mét CAD?"))return;
    if(kind!=="cad"&&kind!=="glb"&&(!Number.isSafeInteger(epsg)||Number(epsg)<=0||Number(epsg)>4294967295))throw Error("EPSG không hợp lệ");
    const a=await desktop.openAsset(handle,kind,epsg);if(!a)return;
    setAsset(a);setPointPage(0);
    const prefix=`ovg://v1/workspaces/${handle}`;
    if(a.kind==="cad"){cad.current=a;invalidate();setAssetPreview(a.count+" đối tượng DXF")}
    else if(a.kind==="las"||a.kind==="laz"||a.kind==="copc"){
      const b=await desktop.resource(`${prefix}/pointcloud/${a.id}/revisions/0/nodes/0`);
      if(b.byteLength%24!==0)throw Error("INVALID_LAS_CHUNK");
      const d=new DataView(b);const first=b.byteLength>=24?[d.getFloat64(0,true),d.getFloat64(8,true),d.getFloat64(16,true)]:[];
      setAssetPreview(`${a.count} điểm, EPSG:${a.epsg}; khối 0: ${b.byteLength/24} điểm; XYZ đầu: ${first.join(", ")}`);
    }else{
      const b=await desktop.resource(`${prefix}/assets/${a.id}/revisions/0/content`);
      setAssetPreview(`GLB tự chứa: ${b.byteLength} byte; renderer 3D ở Wave 3`);
    }
  });
  const readPointPage=(n:number)=>run(async()=>{
    if(!asset||asset.kind==="cad"||asset.kind==="glb"||n<0||n*2048>=asset.count)return;
    const b=await desktop.resource(`ovg://v1/workspaces/${handle}/pointcloud/${asset.id}/revisions/0/nodes/${n}`);
    if(b.byteLength%24)throw Error("INVALID_POINT_CHUNK");
    setPointPage(n);setAssetPreview(`Khối ${n}: ${b.byteLength/24} / ${asset.count} điểm; EPSG:${asset.epsg}`);
  });
  const queryCopc=()=>run(async()=>{
    if(asset?.kind!=="copc")return;
    const b=copcBounds.split(",").map(Number),level=Number(copcLevel);
    if(b.length!==6||b.some(n=>!Number.isFinite(n))||!Number.isInteger(level))throw Error("INVALID_QUERY");
    const data=await desktop.copcQuery(handle,asset.id,b as [number,number,number,number,number,number],level);
    setAssetPreview(`COPC LOD ${level}: ${data.byteLength/24} điểm trong bbox`);
  });
  const closeAsset=()=>run(async()=>{
    if(!asset)return;
    await desktop.closeAsset(handle,asset.id);cad.current=undefined;sequence.current++;
    (map.current?.getSource("cad") as GeoJSONSource|undefined)?.setData(empty());setAsset(undefined);setAssetPreview("");
  });
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
      <button disabled={busy} onClick={()=>void run(async()=>{const c=await desktop.inspectGPKG(handle);if(c){setCatalog(c);setTable(c.layers[0]?.table??"")}})}>Mở danh sách lớp GPKG</button>
      {catalog&&<><select aria-label="Lớp GeoPackage" value={table} onChange={e=>setTable(e.target.value)}>{catalog.layers.map(l=><option key={l.table} value={l.table}>{l.table} · EPSG:{l.epsg}</option>)}</select><button disabled={busy||!table} onClick={()=>void run(async()=>{await desktop.importLayer(handle,catalog.id,table);await afterMutation()})}>Nhập lớp vào dự án hiện tại</button></>}
      <button disabled={busy||!ready||!!asset} onClick={()=>void openAsset("cad")}>Mở DXF XY</button><label>EPSG LAS <input aria-label="EPSG LAS" value={lasEpsg} maxLength={10} onChange={e=>setLasEpsg(e.target.value)}/></label><button disabled={busy||!!asset} onClick={()=>void openAsset("las")}>Đọc LAS</button><button disabled={busy||!!asset} onClick={()=>void openAsset("laz")}>Đọc LAZ</button><button disabled={busy||!!asset} onClick={()=>void openAsset("copc")}>Đọc COPC</button><button disabled={busy||!!asset} onClick={()=>void openAsset("glb")}>Đọc GLB</button><button disabled={busy||!asset} onClick={()=>void closeAsset()}>Đóng tài nguyên</button>
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
    {asset&&["las","laz","copc"].includes(asset.kind)&&<div className="ovg-local-tools"><button disabled={busy||pointPage===0} onClick={()=>void readPointPage(pointPage-1)}>Khối điểm trước</button><button disabled={busy||(pointPage+1)*2048>=asset.count} onClick={()=>void readPointPage(pointPage+1)}>Khối điểm tiếp</button>{asset.kind==="copc"&&<><input aria-label="COPC bbox XYZ min max" value={copcBounds} onChange={e=>setCopcBounds(e.target.value)}/><input aria-label="LOD" value={copcLevel} onChange={e=>setCopcLevel(e.target.value)}/><button disabled={busy} onClick={()=>void queryCopc()}>Truy vấn COPC LOD</button></>}</div>}
    <div role="status" className="ovg-local-status">{notice}{assetPreview&&" · "+assetPreview}{raster&&" · Nguồn: "+raster.attribution}{kind&&" · Nhấp bản đồ để vẽ: "+count+" đỉnh"}</div>
    <div className="ovg-local-table">
      <div><label>Tìm thuộc tính <input value={filter} maxLength={128} onChange={e=>setFilter(e.target.value)}/></label>
      <button disabled={busy} onClick={()=>void run(()=>page("0",filter))}>Trang đầu</button>
      <button disabled={busy||!cursor} onClick={()=>void run(()=>page(cursor??"0",filter))}>Trang tiếp</button></div>
      <div className="ovg-local-rows"><table><thead><tr><th>ID</th><th>Revision</th><th>Thuộc tính</th></tr></thead><tbody>{rows.map(r=><tr key={r.id}><td><button disabled={busy} onClick={()=>void run(()=>select(r.id))}>{r.id.slice(0,8)}</button></td><td>{r.revision}</td><td>{JSON.stringify(r.attributes).slice(0,200)}</td></tr>)}</tbody></table></div>
      {selected&&<div><label>Thuộc tính {selected.id}<textarea rows={3} value={attrs} onChange={e=>setAttrs(e.target.value)}/></label><button disabled={busy} onClick={()=>void update()}>Ghi thuộc tính</button><label>Tọa độ hình học GeoJSON<textarea rows={4} value={geometryText} onChange={e=>setGeometryText(e.target.value)}/></label><button disabled={busy||!!kind} aria-pressed={vertexMode} onClick={()=>setVertexMode(!vertexMode)}>Kéo đỉnh trên bản đồ</button><button disabled={busy} onClick={()=>void reshape()}>Ghi sửa hình học</button><button disabled={busy} onClick={()=>void remove()}>Xóa đối tượng</button></div>}
    </div>
  </section>;
}
