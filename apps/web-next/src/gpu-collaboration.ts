import type {FeatureCollection,Point} from "geojson";
import type {Map,GeoJSONSource} from "./map-surface";
export const COMMENT_SOURCE="ovg-comment-pins",COMMENT_LAYER=COMMENT_SOURCE+"-circle";
export const PRESENCE_SOURCE="ovg-peer-cursors",LEASE_SOURCE="ovg-lease-labels";
type PointLabel={id:string;point:[number,number];label:string;color:string};
export function pointLabels(items:PointLabel[]):FeatureCollection<Point>{
 return {type:"FeatureCollection",features:items.filter(x=>x.point.every(Number.isFinite)&&Math.abs(x.point[0])<=180&&Math.abs(x.point[1])<=90).map(x=>({type:"Feature",id:x.id,geometry:{type:"Point",coordinates:x.point},properties:{label:x.label,color:/^#[0-9a-f]{6}$/i.test(x.color)?x.color:"#087d76",key:x.id}}))};
}
export function syncPointLabels(m:Map,id:string,items:PointLabel[],circle=true){
 const data=pointLabels(items),source=m.getSource(id) as GeoJSONSource|undefined;
 if(source)void source.setData(data);else m.addSource(id,{type:"geojson",data});
 if(circle&&!m.getLayer(id+"-circle"))m.addLayer({id:id+"-circle",source:id,type:"circle",paint:{"circle-radius":8,"circle-color":["get","color"],"circle-stroke-color":"#fff","circle-stroke-width":1.5}});
 if(!m.getLayer(id+"-text"))m.addLayer({id:id+"-text",source:id,type:"symbol",layout:{"text-field":["get","label"],"text-size":12,"text-offset":[0,circle?1.5:0],"text-allow-overlap":true},paint:{"text-color":["get","color"],"text-halo-color":"#fff","text-halo-width":2}});
}
export function removePointLabels(m:Map,id:string){
 if(!m.getStyle())return;
 for(const layer of [id+"-text",id+"-circle"])if(m.getLayer(layer))m.removeLayer(layer);
 if(m.getSource(id))m.removeSource(id);
}
