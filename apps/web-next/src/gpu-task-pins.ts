import type {FeatureCollection,Point} from "geojson";
import type {Map,GeoJSONSource} from "./map-surface";
import {isLate,type ProjectTask} from "./project-tasks";
export const TASK_PIN_SOURCE="ovg-task-pins";
export const TASK_PIN_LAYER=TASK_PIN_SOURCE+"-circle";
export function taskPinData(tasks:ProjectTask[]):FeatureCollection<Point>{
 return {type:"FeatureCollection",features:tasks.flatMap(t=>t.status==="done"||t.longitude===null||t.latitude===null||!Number.isFinite(t.longitude)||!Number.isFinite(t.latitude)?[]:[{type:"Feature",id:t.id,geometry:{type:"Point",coordinates:[t.longitude,t.latitude]},properties:{taskId:t.id,color:isLate(t)?"#dc2626":t.status==="todo"?"#2b7bd0":"#f59e0b"}}])};
}
export function syncTaskPins(m:Map,data:FeatureCollection<Point>){
 const source=m.getSource(TASK_PIN_SOURCE) as GeoJSONSource|undefined;
 if(source)void source.setData(data);else m.addSource(TASK_PIN_SOURCE,{type:"geojson",data});
 if(!m.getLayer(TASK_PIN_LAYER))m.addLayer({id:TASK_PIN_LAYER,type:"circle",source:TASK_PIN_SOURCE,paint:{"circle-radius":10,"circle-color":["get","color"],"circle-stroke-width":2,"circle-stroke-color":"#fff"}});
}
