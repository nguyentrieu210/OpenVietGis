import type {Feature,FeatureCollection,Geometry,Position} from "geojson";
import type {Map as MLMap, GeoJSONSource, SymbolLayerSpecification} from "./map-surface";
import type {LayerView} from "./LayerProperties";
import type {StudioRow} from "./studio-contract";

const SOURCE="ovg-gpu-labels";
const PREFIX=SOURCE+"-";
type Kind="point"|"polygon"|"line";
type Bounds=[number,number,number,number];
type LabelProps={text:string;layerId:string;group:string;key:string;unique:boolean};
type LabelFeature=Feature<Geometry,LabelProps>;
type LabelGroup={id:string;layerId:string;kind:Kind;view:LayerView};
type LabelPlan={data:FeatureCollection<Geometry,LabelProps>;groups:LabelGroup[];bounds:Map<string,Bounds>};

function geometryBounds(g:Geometry):Bounds{
 const b:Bounds=[Infinity,Infinity,-Infinity,-Infinity];
 const walk=(a:unknown):void=>{if(!Array.isArray(a))return;if(typeof a[0]==="number"&&typeof a[1]==="number"){b[0]=Math.min(b[0],a[0]);b[1]=Math.min(b[1],a[1]);b[2]=Math.max(b[2],a[0]);b[3]=Math.max(b[3],a[1])}else for(const c of a)walk(c)};
 if(g.type!=="GeometryCollection")walk(g.coordinates);
 return b;
}
function midpoint(a:Position[]):Position|null{
 const d=a.slice(1).map((p,i)=>Math.hypot(p[0]!-a[i]![0]!,p[1]!-a[i]![1]!));let h=d.reduce((x,y)=>x+y,0)/2;
 for(let i=0;i<d.length;i++){const l=d[i]!;if(l>0&&h<=l){const p=a[i]!,q=a[i+1]!,t=h/l;return [p[0]!+(q[0]!-p[0]!)*t,p[1]!+(q[1]!-p[1]!)*t]}h-=l}return a[0]??null;
}
export function planGpuLabels(rows:StudioRow[],views:Record<string,LayerView>,text:(r:StudioRow,v:LayerView)=>string,anchor:(g:Geometry)=>[number,number]|null):LabelPlan{
 const groups=new Map<string,LabelGroup>(),features:LabelFeature[]=[],bounds=new Map<string,Bounds>();
 for(const r of rows){
  const layerId=r.layer.id,v=views[layerId],g=r.geometry;
  if(!v?.labelField||v.labelHidden||!g||Math.max(v.labelMinZoom,v.minZoom)>=Math.min(v.labelMaxZoom,v.maxZoom))continue;
  const value=text(r,v);if(!value)continue;
  let geometry:Geometry=g,kind:Kind;
  if(g.type==="Polygon"||g.type==="MultiPolygon")kind="polygon";
  else if((g.type==="LineString"||g.type==="MultiLineString")&&(v.labelPlacement==="auto"||v.labelPlacement==="line"))kind="line";
  else if(g.type==="Point"||g.type==="MultiPoint")kind="point";
  else{
   const p=g.type==="MultiLineString"?midpoint(g.coordinates.reduce((a,b)=>a.length>=b.length?a:b,[] as Position[])):anchor(g);
   if(!p)continue;geometry={type:"Point",coordinates:p};kind="point";
  }
  const group=layerId+":"+kind,key=layerId+":"+r.key;
  if(!groups.has(group))groups.set(group,{id:group,layerId,kind,view:v});
  features.push({type:"Feature",id:key,geometry,properties:{text:value,layerId,group,key,unique:v.labelUnique}});
  if(v.labelUnique)bounds.set(key,geometryBounds(geometry));
 }
 return {data:{type:"FeatureCollection",features},groups:[...groups.values()].sort((a,b)=>a.view.labelPriority-b.view.labelPriority||a.id.localeCompare(b.id)),bounds};
}
function colorImageId(v:LayerView){return PREFIX+"bg-"+v.labelHalo.slice(1)}
export function labelLayer(group:LabelGroup):SymbolLayerSpecification{
 const v=group.view,line=group.kind==="line",auto=v.labelPlacement==="auto",offset=v.labelOffset/v.labelSize;
 const anchors={above:"bottom",below:"top",left:"right",right:"left",center:"center",line:"center",auto:"center"} as const;
 const layout:NonNullable<SymbolLayerSpecification["layout"]>={
  "symbol-placement":line?"line-center":"point","text-field":["get","text"],
  "text-font":[v.labelBold?"Arial Bold":"Arial Regular"],"text-size":v.labelSize,
  "text-rotation-alignment":line?"map":"viewport","text-pitch-alignment":"viewport",
  "text-justify":"auto","text-padding":2,"text-max-width":24,"text-keep-upright":true,
  "text-allow-overlap":v.labelOverlap,"text-ignore-placement":false,
  "text-anchor":anchors[v.labelPlacement],
 };
 if(!line&&auto){
  layout["text-variable-anchor"]=group.kind==="point"?["bottom","left","right","top","center"]:["center","bottom","top"];
  layout["text-radial-offset"]=group.kind==="point"?offset:0;
 }else if(!line&&v.labelPlacement!=="center"&&v.labelPlacement!=="line")layout["text-radial-offset"]=offset;
 if(v.labelBackground){
  layout["icon-image"]=colorImageId(v);layout["icon-text-fit"]="both";layout["icon-text-fit-padding"]=[2,4,2,4];
  layout["icon-allow-overlap"]=v.labelOverlap;layout["icon-ignore-placement"]=false;
  layout["icon-rotation-alignment"]=line?"map":"viewport";layout["icon-pitch-alignment"]="viewport";
 }
 return {id:PREFIX+group.id,type:"symbol",source:SOURCE,minzoom:Math.max(v.labelMinZoom,v.minZoom),maxzoom:Math.min(v.labelMaxZoom,v.maxZoom),filter:["==",["get","group"],group.id],layout,paint:{"text-color":v.labelColor,"text-halo-color":v.labelHalo,"text-halo-width":Math.min(v.labelHaloWidth,v.labelSize/4),"text-halo-blur":.25}};
}
function addBackground(m:MLMap,v:LayerView){
 const id=colorImageId(v);if(m.hasImage(id))return;
 const canvas=document.createElement("canvas");canvas.width=canvas.height=12;
 const ctx=canvas.getContext("2d");if(!ctx)return;
 ctx.fillStyle=v.labelHalo;ctx.beginPath();ctx.roundRect(0,0,12,12,3);ctx.fill();
 m.addImage(id,ctx.getImageData(0,0,12,12),{stretchX:[[4,8]],stretchY:[[4,8]],content:[3,3,9,9]});
}
export function uniqueVisibleLabels(plan:LabelPlan,viewport:Bounds):Map<string,string[]>{
 const byGroup=new Map<string,string[]>(),seen=new Set<string>();
 for(const f of plan.data.features){const p=f.properties;if(!p.unique)continue;
  const b=plan.bounds.get(p.key);if(!b||b[0]>viewport[2]||b[2]<viewport[0]||b[1]>viewport[3]||b[3]<viewport[1])continue;
  const uid=p.layerId+":"+p.text;if(seen.has(uid))continue;seen.add(uid);
  const keys=byGroup.get(p.group)??[];keys.push(p.key);byGroup.set(p.group,keys);
 }
 return byGroup;
}
export function createGpuLabelRenderer(m:MLMap){
 let plan:LabelPlan|null=null,appliedData="",layerSpecs=new Map<string,string>(),filters=new Map<string,string>(),images=new Set<string>(),disposed=false;
 const refreshUnique=()=>{
  if(!plan||disposed||!plan.bounds.size)return;
  const b=m.getBounds(),selected=uniqueVisibleLabels(plan,[b.getWest(),b.getSouth(),b.getEast(),b.getNorth()]);
  for(const g of plan.groups){if(!g.view.labelUnique)continue;const id=PREFIX+g.id,keys=selected.get(g.id)??[],signature=JSON.stringify(keys);
   if(filters.get(id)===signature||!m.getLayer(id))continue;
   m.setFilter(id,["all",["==",["get","group"],g.id],["in",["get","key"],["literal",keys]]]);filters.set(id,signature);
  }
 };
 const apply=(next:LabelPlan)=>{
  if(disposed)return;plan=next;if(!m.getStyle())return;
  const data=JSON.stringify(next.data),source=m.getSource(SOURCE) as GeoJSONSource|undefined;
  if(!source){m.addSource(SOURCE,{type:"geojson",data:next.data,maxzoom:22,tolerance:.2,buffer:128});appliedData=data}
  else if(data!==appliedData){source.setData(next.data);appliedData=data}
  const wanted=new Set(next.groups.map(g=>PREFIX+g.id)),wantedImages=new Set<string>();
  for(const id of layerSpecs.keys())if(!wanted.has(id)){if(m.getLayer(id))m.removeLayer(id);layerSpecs.delete(id);filters.delete(id)}
  let reordered=false;
  for(const g of next.groups){if(g.view.labelBackground){addBackground(m,g.view);wantedImages.add(colorImageId(g.view))}
   const spec=labelLayer(g),sig=JSON.stringify(spec);
   if(!m.getLayer(spec.id)||layerSpecs.get(spec.id)!==sig){
    if(m.getLayer(spec.id))m.removeLayer(spec.id);
    m.addLayer(spec);layerSpecs.set(spec.id,sig);filters.delete(spec.id);reordered=true;
   }
  }
  if(reordered)for(const g of next.groups)m.moveLayer(PREFIX+g.id);
  for(const id of images)if(!wantedImages.has(id)&&m.hasImage(id))m.removeImage(id);
  images=wantedImages;refreshUnique();
 };
 // Camera animation never touches GeoJSON or React state. Only optional uniqueness
 // is recalculated after movement; MapLibre owns placement and interpolation.
 m.on("moveend",refreshUnique);m.on("resize",refreshUnique);
 const dispose=()=>{disposed=true;m.off("moveend",refreshUnique);m.off("resize",refreshUnique);
  if(!m.getStyle())return;for(const id of layerSpecs.keys())if(m.getLayer(id))m.removeLayer(id);
  if(m.getSource(SOURCE))m.removeSource(SOURCE);for(const id of images)if(m.hasImage(id))m.removeImage(id);
 };
 return {apply,dispose};
}
