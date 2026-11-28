import type {PreparedEngine} from "./coordinator";
import type {FootprintView} from "./footprint3d";
import type {Map as MLMap} from "maplibre-gl";
import {customMeshLayer,terrainMesh,modelMesh,type PlacedModel,type InteractiveMeshLayer} from "./mesh-layer";
import {checkSceneBudget} from "./mesh-picking";
import type {TerrainGrid} from "./terrain";
import "maplibre-gl/dist/maplibre-gl.css";
export type GeographicCamera=Readonly<{longitude:number;latitude:number;zoom:number;bearing:number;pitch?:number}>;
export type NamedModel=Readonly<{id:string;placement:PlacedModel}>;
export type SharedVector={id:string;geometry:import("geojson").Geometry;color:string};
export type SelectableFootprintEngine=PreparedEngine&{select(featureId:string):void;selectModel(id:string):void;setLayerVisible(id:string,visible:boolean):void;getCamera():GeographicCamera;replaceVectors?(rows:readonly SharedVector[],selected:string):void;setMeshFrame?(index:number):void};
export async function prepareFootprint3d(host:HTMLElement,view:FootprintView,camera:GeographicCamera,signal:AbortSignal,onPick:(featureId:string)=>void,extras?:{terrain?:TerrainGrid;model?:PlacedModel;models?:readonly NamedModel[];onModelPick?:(id:string)=>void}):Promise<SelectableFootprintEngine>{
 if(signal.aborted)throw new DOMException("Aborted","AbortError");
 if(![camera.longitude,camera.latitude,camera.zoom,camera.bearing].every(Number.isFinite)||Math.abs(camera.longitude)>180||Math.abs(camera.latitude)>85||camera.zoom<0||camera.zoom>22||(camera.pitch!==undefined&&(!Number.isFinite(camera.pitch)||camera.pitch<0||camera.pitch>75)))throw Error("Invalid camera");
 const models=extras?.models??(extras?.model?[{id:"local",placement:extras.model}]:[]);checkSceneBudget(models.map(v=>v.placement.model));
 if(new Set(models.map(v=>v.id)).size!==models.length||models.some(v=>!v.id||v.id==="terrain"||v.id==="footprints"))throw Error("Invalid model identities");
 const {Map,NavigationControl}=await import("maplibre-gl");
 if(signal.aborted)throw new DOMException("Aborted","AbortError");
 const surface=document.createElement("div");surface.className="ovg-footprint-3d";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5";
 host.append(surface);
 let map:MLMap|null=null,disposed=false,active=false;
 const ids=new Set(view.features.map(f=>f.properties.featureId));
 const layers=new globalThis.Map<string,InteractiveMeshLayer>();let footprintVisible=true;
 const selectModel=(id:string)=>{if(disposed||!map)return;for(const [key,layer] of layers)if(key!=="terrain")layer.setSelected(key===id);map.triggerRepaint()};
 const setLayerVisible=(id:string,visible:boolean)=>{if(disposed||!map)return;if(id==="footprints"){footprintVisible=visible;for(const layer of ["footprint-ground","building-extrusion"])map.setLayoutProperty(layer,"visibility",visible?"visible":"none")}else layers.get(id)?.setVisible(visible);map.triggerRepaint()};
 const select=(featureId:string)=>{if(disposed||!map)return;const selected=ids.has(featureId)?featureId:"";map.setPaintProperty("building-extrusion","fill-extrusion-color",["case",["==",["get","featureId"],selected],"#f59e0b","#326dba"]);};
 const dispose=()=>{if(disposed)return;disposed=true;active=false;map?.remove();surface.remove()};
 try{
  map=new Map({container:surface,center:[camera.longitude,camera.latitude],zoom:camera.zoom,bearing:camera.bearing,pitch:camera.pitch??58,maxPitch:75,attributionControl:false,style:{version:8,sources:{footprints:{type:"geojson",data:view}},layers:[
   {id:"background",type:"background",paint:{"background-color":"#dce6e8"}},
   {id:"footprint-ground",type:"fill",source:"footprints",paint:{"fill-color":"#607d8b","fill-opacity":.35}},
   {id:"building-extrusion",type:"fill-extrusion",source:"footprints",paint:{"fill-extrusion-color":"#326dba","fill-extrusion-base":["get","base"],"fill-extrusion-height":["get","top"],"fill-extrusion-opacity":.9}}
  ]}});
  map.addControl(new NavigationControl({showCompass:true}),"top-right");
  await new Promise<void>((resolve,reject)=>{
   const m=map!;const finish=(error?:Error)=>{clearTimeout(timer);signal.removeEventListener("abort",abort);m.off("load",loaded);m.off("error",failed);error?reject(error):resolve()};
   const abort=()=>finish(new DOMException("Aborted","AbortError")),loaded=()=>finish(),failed=(event:{error?:{message?:string}})=>finish(Error(event.error?.message??"3D renderer failed"));
   const timer=setTimeout(()=>finish(Error("3D renderer timeout")),20000);
   signal.addEventListener("abort",abort,{once:true});m.on("load",loaded);m.on("error",failed);if(signal.aborted)abort();
  });
  if(signal.aborted)throw new DOMException("Aborted","AbortError");
  if(extras?.terrain){const terrain=customMeshLayer("project-dem",terrainMesh(extras.terrain));layers.set("terrain",terrain);map.addLayer(terrain,"footprint-ground")}
  models.forEach((m,index)=>{const layer=customMeshLayer("scene-model-"+index,modelMesh(m.placement));layers.set(m.id,layer);map!.addLayer(layer)});
  if(layers.size)map.triggerRepaint();
  map.on("click",e=>{
   if(!active||!map)return;const canvas=map.getCanvas(),x=e.point.x/canvas.clientWidth*2-1,y=1-e.point.y/canvas.clientHeight*2;
   let best=Infinity,hit="";for(const [id,layer] of layers){if(id==="terrain")continue;const depth=layer.pick(x,y);if(depth!==null&&depth<best){hit=id;best=depth}}
   const ground=layers.get("terrain")?.pick(x,y);
   if(hit&&(ground==null||best<=ground+1e-7)){selectModel(hit);extras?.onModelPick?.(hit);return}
   if(footprintVisible){const id=map.queryRenderedFeatures(e.point,{layers:["building-extrusion"]})[0]?.properties.featureId;if(typeof id==="string"&&ids.has(id)){selectModel("");select(id);onPick(id)}}
  });
  return {select,selectModel,setLayerVisible,getCamera(){if(!map||disposed)throw Error("Disposed 3D surface");const p=map.getCenter().wrap();return {longitude:p.lng,latitude:p.lat,zoom:map.getZoom(),bearing:map.getBearing(),pitch:map.getPitch()}},activate(){if(disposed)throw Error("Disposed 3D surface");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";map?.resize()},dispose};
 }catch(error){dispose();throw error}
}
