import type {FeatureCollection} from "geojson";
import {readCamera,type CameraResume} from "./map-render-plan";
export type ComparisonLayer={id:string;name:string;min:number;max:number};
export type ComparisonPrefs={order:string[];hidden:string[];camera:CameraResume|null;follow:boolean};
export const COMPARISON_LIMITS={layers:100,features:5000,vertices:200000};
export function comparisonPrefs(raw:unknown,layers:readonly ComparisonLayer[]):ComparisonPrefs{
 const v=raw&&typeof raw==="object"?raw as Partial<ComparisonPrefs>:{};
 const ids=layers.map(l=>l.id),allowed=new Set(ids);
 const clean=(x:unknown)=>Array.isArray(x)?[...new Set(x.filter((id):id is string=>typeof id==="string"&&allowed.has(id)))]:[];
 const order=clean(v.order);
 return {order:[...order,...ids.filter(id=>!order.includes(id))],hidden:clean(v.hidden),camera:readCamera(v.camera),follow:v.follow===true};
}
export function moveComparisonLayer(order:readonly string[],id:string,step:-1|1){
 const next=[...order],i=next.indexOf(id),j=i+step;if(i>=0&&j>=0&&j<next.length){[next[i],next[j]]=[next[j]!,next[i]!]}return next;
}
/** Reject over-budget populations; do not silently clip. Only current authorized
 * rendered vector features enter the comparison source; no network sources. */
export function comparisonCollection(input:FeatureCollection,layers:readonly ComparisonLayer[]):FeatureCollection{
 if(layers.length>COMPARISON_LIMITS.layers)throw Error("Khung so sánh hỗ trợ tối đa 100 lớp vector.");
 const allowed=new Set(layers.map(l=>l.id));let vertices=0;
 const features=input.features.filter(f=>typeof f.properties?.layerId==="string"&&allowed.has(f.properties.layerId));
 if(features.length>COMPARISON_LIMITS.features)throw Error("Khung so sánh hỗ trợ tối đa 5.000 đối tượng đang hiển thị.");
 const visit=(x:unknown,depth=0):void=>{
  if(depth>8||!Array.isArray(x))throw Error("Hình học không hợp lệ.");
  if(typeof x[0]==="number"){
   if(x.length<2||!Number.isFinite(x[0])||typeof x[1]!=="number"||!Number.isFinite(x[1]))throw Error("Tọa độ không hợp lệ.");
   if(++vertices>COMPARISON_LIMITS.vertices)throw Error("Khung so sánh vượt 200.000 đỉnh.");
  }else for(const item of x)visit(item,depth+1);
 };
 for(const f of features){if(!f.geometry||!("coordinates" in f.geometry))throw Error("Kiểu hình học khung so sánh chưa hỗ trợ.");visit(f.geometry.coordinates)}
 return {type:"FeatureCollection",features};
}
