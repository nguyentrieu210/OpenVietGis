import type {FeatureCollection,Point} from "geojson";
import type {Map,GeoJSONSource} from "./map-surface";
import type {diagramValues} from "./feature-diagram";
type Candidate={key:string;p:[number,number];size:number;minZoom:number;maxZoom:number;values:ReturnType<typeof diagramValues>};
const SOURCE="ovg-gpu-diagrams",LAYER=SOURCE+"-symbol";
export function diagramTextureKey(r:Candidate){return SOURCE+":"+JSON.stringify([r.size,r.values.map(v=>[v.color,v.fraction])])}
function texture(r:Candidate){
 const canvas=document.createElement("canvas");canvas.width=canvas.height=Math.ceil(r.size*2);
 const c=canvas.getContext("2d");if(!c)return null;
 c.scale(canvas.width/100,canvas.height/100);c.fillStyle="#ffffffd9";c.beginPath();c.roundRect(0,0,100,100,8);c.fill();
 c.strokeStyle="#475569";c.lineWidth=1;c.beginPath();c.moveTo(5,50);c.lineTo(95,50);c.stroke();
 const w=86/r.values.length;for(const [i,v] of r.values.entries()){const h=Math.abs(v.fraction)*42;c.fillStyle=v.color;c.fillRect(7+i*w,v.fraction>=0?50-h:50,Math.max(2,w-3),h)}
 return c.getImageData(0,0,canvas.width,canvas.height);
}
export function attachGpuDiagrams(m:Map,candidates:Candidate[]){
 const empty:FeatureCollection<Point>={type:"FeatureCollection",features:[]};
 m.addSource(SOURCE,{type:"geojson",data:empty});
 m.addLayer({id:LAYER,type:"symbol",source:SOURCE,layout:{"icon-image":["get","image"],"icon-allow-overlap":false,"icon-ignore-placement":false,"icon-rotation-alignment":"viewport","icon-pitch-alignment":"viewport"}});
 let images=new Set<string>(),allocated=new Set<string>(),disposed=false;
 const refresh=()=>{if(disposed)return;const bounds=m.getBounds(),zoom=m.getZoom(),next=new Set<string>(),features:FeatureCollection<Point>["features"]=[];
  for(const r of candidates){if(zoom<r.minZoom||zoom>=r.maxZoom||!bounds.contains(r.p))continue;const id=diagramTextureKey(r);
   if(!m.hasImage(id)){const data=texture(r);if(!data)continue;m.addImage(id,data,{pixelRatio:2});allocated.add(id)}
   next.add(id);features.push({type:"Feature",id:r.key,geometry:{type:"Point",coordinates:r.p},properties:{image:id}});if(features.length>=150)break;
  }
  const old=images;images=next;void Promise.resolve((m.getSource(SOURCE) as GeoJSONSource).setData({type:"FeatureCollection",features})).then(()=>{if(disposed)return;for(const id of old)if(!images.has(id)&&m.hasImage(id)){m.removeImage(id);allocated.delete(id)}}).catch(()=>{});
 };
 refresh();m.on("moveend",refresh);m.on("resize",refresh);
 return()=>{disposed=true;m.off("moveend",refresh);m.off("resize",refresh);if(m.getLayer(LAYER))m.removeLayer(LAYER);if(m.getSource(SOURCE))m.removeSource(SOURCE);for(const id of allocated)if(m.hasImage(id))m.removeImage(id)};
}
