import type {PreparedEngine} from "./coordinator";
import type {FootprintView} from "./footprint3d";
import type {Map as MLMap} from "maplibre-gl";
import {customMeshLayer,terrainMesh,modelMesh,type PlacedModel} from "./mesh-layer";
import type {TerrainGrid} from "./terrain";
import "maplibre-gl/dist/maplibre-gl.css";
export type GeographicCamera=Readonly<{longitude:number;latitude:number;zoom:number;bearing:number}>;
export type SelectableFootprintEngine=PreparedEngine&{select(featureId:string):void};
export async function prepareFootprint3d(host:HTMLElement,view:FootprintView,camera:GeographicCamera,signal:AbortSignal,onPick:(featureId:string)=>void,extras?:{terrain?:TerrainGrid;model?:PlacedModel}):Promise<SelectableFootprintEngine>{
 if(signal.aborted)throw new DOMException("Aborted","AbortError");
 if(![camera.longitude,camera.latitude,camera.zoom,camera.bearing].every(Number.isFinite)||Math.abs(camera.longitude)>180||Math.abs(camera.latitude)>85||camera.zoom<0||camera.zoom>22)throw Error("Invalid camera");
 const {Map,NavigationControl}=await import("maplibre-gl");
 if(signal.aborted)throw new DOMException("Aborted","AbortError");
 const surface=document.createElement("div");surface.className="ovg-footprint-3d";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5";
 host.append(surface);
 let map:MLMap|null=null,disposed=false,active=false;
 const ids=new Set(view.features.map(f=>f.properties.featureId));
 const select=(featureId:string)=>{if(disposed||!map)return;const selected=ids.has(featureId)?featureId:"";map.setPaintProperty("building-extrusion","fill-extrusion-color",["case",["==",["get","featureId"],selected],"#f59e0b","#326dba"]);};
 const dispose=()=>{if(disposed)return;disposed=true;active=false;map?.remove();surface.remove()};
 try{
  map=new Map({container:surface,center:[camera.longitude,camera.latitude],zoom:camera.zoom,bearing:camera.bearing,pitch:58,maxPitch:75,attributionControl:false,style:{version:8,sources:{footprints:{type:"geojson",data:view}},layers:[
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
  if(extras?.terrain)map.addLayer(customMeshLayer("project-dem",terrainMesh(extras.terrain)),"footprint-ground");
  if(extras?.model)map.addLayer(customMeshLayer("local-model",modelMesh(extras.model)));
  if(extras?.terrain||extras?.model)map.triggerRepaint();
  map.on("click","building-extrusion",e=>{if(!active)return;const id=e.features?.[0]?.properties.featureId;if(typeof id==="string"&&ids.has(id)){select(id);onPick(id)}});
  return {select,activate(){if(disposed)throw Error("Disposed 3D surface");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";map?.resize()},dispose};
 }catch(error){dispose();throw error}
}
