import type {FeatureCollection,Point} from "geojson";
import type {Map,GeoJSONSource,MapMouseEvent} from "./map-surface";
type XY=[number,number];
const SOURCE="ovg-vertex-handles",LAYER=SOURCE+"-circle",DRAG=SOURCE+"-drag",DRAG_LAYER=DRAG+"-circle";
export function vertexData(points:XY[]):FeatureCollection<Point>{
 return {type:"FeatureCollection",features:points.map((coordinates,id)=>({type:"Feature",id,geometry:{type:"Point",coordinates},properties:{index:id}}))};
}
export function attachVertexHandles(m:Map,points:XY[],selected:number,editable:boolean,select:(i:number)=>void,commit:(i:number,p:XY)=>void){
 const empty=vertexData([]);
 m.addSource(SOURCE,{type:"geojson",data:vertexData(points)});
 m.addLayer({id:LAYER,source:SOURCE,type:"circle",paint:{"circle-radius":["case",["==",["get","index"],selected],7,5],"circle-color":["case",["==",["get","index"],selected],"#f97316","#fff"],"circle-stroke-color":"#ea580c","circle-stroke-width":2,"circle-opacity":["case",["boolean",["feature-state","dragging"],false],0,1],"circle-stroke-opacity":["case",["boolean",["feature-state","dragging"],false],0,1]}});
 m.addSource(DRAG,{type:"geojson",data:empty});
 m.addLayer({id:DRAG_LAYER,source:DRAG,type:"circle",paint:{"circle-radius":7,"circle-color":"#f97316","circle-stroke-color":"#fff","circle-stroke-width":2}});
 let index:number|null=null,latest:XY|null=null,start:{x:number;y:number}|null=null,frame=0,pan=false;
 const paint=()=>{frame=0;if(latest&&index!==null)void (m.getSource(DRAG) as GeoJSONSource|undefined)?.setData(vertexData([latest]))};
 const reset=()=>{if(frame)cancelAnimationFrame(frame);frame=0;if(index!==null&&m.getSource(SOURCE))m.removeFeatureState({source:SOURCE,id:index},"dragging");if(m.getSource(DRAG))void (m.getSource(DRAG) as GeoJSONSource).setData(empty);if(pan)m.dragPan.enable();pan=false;index=null;latest=null;start=null};
 const down=(e:MapMouseEvent)=>{if(e.originalEvent.button!==0||document.body.dataset.ovgSpacePan)return;const hit=m.queryRenderedFeatures(e.point,{layers:[LAYER]})[0],i=Number(hit?.properties?.index);if(!Number.isInteger(i)||!points[i])return;e.preventDefault();if(!editable){select(i);return}index=i;start=e.point;latest=points[i];pan=m.dragPan.isEnabled();if(pan)m.dragPan.disable();m.setFeatureState({source:SOURCE,id:i},{dragging:true});paint()};
 const move=(e:MapMouseEvent)=>{if(index===null)return;latest=[e.lngLat.lng,e.lngLat.lat];if(!frame)frame=requestAnimationFrame(paint)};
 const up=(e:MapMouseEvent)=>{if(index===null)return;const i=index,p:XY=[e.lngLat.lng,e.lngLat.lat],moved=start!==null&&Math.hypot(e.point.x-start.x,e.point.y-start.y)>2;reset();if(moved)commit(i,p);else select(i)};
 const cancel=()=>reset();
 m.on("mousedown",LAYER,down);m.on("mousemove",move);m.on("mouseup",up);window.addEventListener("ovg:space-pan-start",cancel);window.addEventListener("blur",cancel);window.addEventListener("mouseup",cancel);
 return ()=>{m.off("mousedown",LAYER,down);m.off("mousemove",move);m.off("mouseup",up);window.removeEventListener("ovg:space-pan-start",cancel);window.removeEventListener("blur",cancel);window.removeEventListener("mouseup",cancel);reset();for(const id of [DRAG_LAYER,LAYER])if(m.getLayer(id))m.removeLayer(id);for(const id of [DRAG,SOURCE])if(m.getSource(id))m.removeSource(id)};
}
