import type {Map as MLMap} from "maplibre-gl";
import type {SelectableFootprintEngine} from "./maplibre3d";
import {customMeshLayer,type InteractiveMeshLayer,type SceneMesh} from "./mesh-layer";
import {parseStaticModel,MODEL_MAX_BYTES,type StaticModel} from "./gltf";
import {selectTiles,tileVertex,TRIANGLE_LIMIT,type Tile} from "./tileset";
import {MercatorCoordinate} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
const deg=180/Math.PI;
export function tileMesh(tile:Tile,model:StaticModel):SceneMesh{
 const r=tile.region,o=MercatorCoordinate.fromLngLat([(r[0]+r[2])/2*deg,(r[1]+r[3])/2*deg],r[4]),v=model.positions,out=new Float32Array(v.length);
 for(let i=0;i<v.length;i+=3){const [lon,lat,h]=tileVertex(tile,v[i]!,v[i+1]!,v[i+2]!),p=MercatorCoordinate.fromLngLat([lon,lat],h);out[i]=p.x-o.x;out[i+1]=p.y-o.y;out[i+2]=p.z-o.z}
 return {origin:[o.x,o.y,o.z],vertices:out,colors:model.colors};
}
export type TileHit={uri:string;triangle:number};
export type SelectableTileEngine=SelectableFootprintEngine&{highlightFeature(hit:TileHit,first:number,count:number):boolean;clearFeature():void};
export async function prepareTiles3d(host:HTMLElement,root:Tile,signal:AbortSignal,load:(uri:string,signal:AbortSignal)=>Promise<ArrayBuffer>,status:(v:string)=>void,onPick?:(hit:TileHit|null)=>void):Promise<SelectableTileEngine>{
 const {Map,NavigationControl}=await import("maplibre-gl");if(signal.aborted)throw new DOMException("Aborted","AbortError");
 const surface=document.createElement("div");surface.className="ovg-footprint-3d";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5";host.append(surface);
 let map:MLMap|null=null,disposed=false,active=false,visible=true,request:AbortController|null=null,serial=0;
 const layers=new globalThis.Map<string,{layer:InteractiveMeshLayer;triangles:number}>();
 let picked:TileHit|null=null;
 const resetHighlight=()=>{for(const item of layers.values())item.layer.setSelectedRange(0,0);map?.triggerRepaint()};
 const clearFeature=()=>{picked=null;resetHighlight();onPick?.(null)};
 const dispose=()=>{if(disposed)return;disposed=true;active=false;request?.abort();map?.remove();surface.remove();layers.clear()};
 const clear=(uri:string)=>{if(picked?.uri===uri)clearFeature();const item=layers.get(uri);if(item){if(map?.getLayer(item.layer.id))map.removeLayer(item.layer.id);layers.delete(uri)}};
 const update=async()=>{
  if(!map||!active||disposed)return;const m=map;request?.abort();const run=new AbortController();request=run;
  const bounds=m.getBounds(),c=m.getCenter().wrap();
  const selection=selectTiles(root,{west:bounds.getWest(),south:bounds.getSouth(),east:bounds.getEast(),north:bounds.getNorth(),longitude:c.lng,latitude:c.lat,zoom:m.getZoom()});
  const desired=new Set(selection.tiles.map(t=>t.uri!));for(const uri of layers.keys())if(!desired.has(uri))clear(uri);
  let triangles=[...layers.values()].reduce((n,v)=>n+v.triangles,0),skipped=selection.omitted,failed=0;
  const valid=()=>!run.signal.aborted&&!disposed&&active;
  status("Đang tải 3D Tiles…");
  for(const tile of selection.tiles){
   if(!valid())return;const uri=tile.uri!;if(layers.has(uri))continue;
   try{
    const data=await load(uri,run.signal);if(!valid())return;if(data.byteLength>MODEL_MAX_BYTES)throw Error("Tile size");
    const model=parseStaticModel(data,"glb");if(triangles+model.triangles>TRIANGLE_LIMIT){skipped++;continue}
    const mesh=tileMesh(tile,model),layer=customMeshLayer("project-tile-"+(++serial),mesh);layer.setVisible(visible);m.addLayer(layer);
    layers.set(uri,{layer,triangles:model.triangles});triangles+=model.triangles;
   }catch(e){if(!valid())return;failed++}
  }
  if(valid()){m.triggerRepaint();status(layers.size+" tile · "+triangles.toLocaleString("vi-VN")+" tam giác"+(skipped?" · "+skipped+" tile ngoài ngân sách":"")+(failed?" · "+failed+" tile lỗi/không hỗ trợ":"")+(layers.size===0&&!failed?" · Không có tile trong vùng nhìn":""))}
 };
 try{
  const r=root.region,lon=(r[0]+r[2])/2*deg,lat=(r[1]+r[3])/2*deg,span=Math.max(.0001,(r[2]-r[0])*deg,(r[3]-r[1])*deg/Math.max(.08,Math.cos(lat/deg)));
  map=new Map({container:surface,center:[lon,lat],zoom:Math.max(0,Math.min(19,Math.log2(180/span))),pitch:45,maxPitch:75,renderWorldCopies:false,attributionControl:false,style:{version:8,sources:{},layers:[{id:"background",type:"background",paint:{"background-color":"#dce6e8"}}]}});
  map.addControl(new NavigationControl({showCompass:true}),"top-right");
  await new Promise<void>((resolve,reject)=>{const m=map!;const finish=(e?:Error)=>{clearTimeout(timer);signal.removeEventListener("abort",abort);m.off("load",loaded);m.off("error",failed);e?reject(e):resolve()};
   const abort=()=>finish(new DOMException("Aborted","AbortError")),loaded=()=>finish(),failed=()=>finish(Error("3D Tiles renderer failed")),timer=setTimeout(()=>finish(Error("3D Tiles renderer timeout")),20000);
   signal.addEventListener("abort",abort,{once:true});m.on("load",loaded);m.on("error",failed);if(signal.aborted)abort();
  });
  map.on("movestart",()=>request?.abort());map.on("moveend",()=>void update());map.on("resize",()=>void update());
  map.on("click",e=>{if(!onPick||!active||!visible||!map)return;const canvas=map.getCanvas(),x=e.point.x/canvas.clientWidth*2-1,y=1-e.point.y/canvas.clientHeight*2;let best=Infinity;let hit:TileHit|null=null;for(const [uri,item] of layers){const p=item.layer.pickTriangle(x,y);if(p&&p.depth<best){best=p.depth;hit={uri,triangle:p.triangle}}}picked=hit;resetHighlight();onPick(hit)});
  return {select(){},selectModel(){},highlightFeature(hit,first,count){if(!active||disposed||!visible||picked?.uri!==hit.uri||picked.triangle!==hit.triangle||hit.triangle<first||hit.triangle>=first+count)return false;const layer=layers.get(hit.uri)?.layer;if(!layer||!layer.setSelectedRange(first,count))return false;map?.triggerRepaint();return true},clearFeature,setLayerVisible(_id,value){visible=value;if(!value)clearFeature();for(const item of layers.values())item.layer.setVisible(value);map?.triggerRepaint()},getCamera(){if(!map||disposed)throw Error("Disposed");const c=map.getCenter().wrap();return {longitude:c.lng,latitude:c.lat,zoom:map.getZoom(),bearing:map.getBearing(),pitch:map.getPitch()}},activate(){if(disposed)throw Error("Disposed");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";map?.resize();void update()},dispose};
 }catch(e){dispose();throw e}
}
