import type {Feature,FeatureCollection,Geometry,Position} from "geojson";
import type {Map as MLMap, GeoJSONSource, SymbolLayerSpecification} from "./map-surface";
import type {LayerView} from "./LayerProperties";
import {overrideFor,type LabelOverrides} from "./label-overrides";
import {labelVariants} from "./label-rules";
import {visiblePolygonAnchor} from "./label-viewport";
import {polygonLabelAnchor} from "./polygon-label-anchor";
import type {StudioRow} from "./studio-contract";

const SOURCE="ovg-gpu-labels",CALLOUT_SOURCE="ovg-gpu-label-callouts";
const PREFIX=SOURCE+"-";
type Kind="point"|"polygon"|"line";
type Bounds=[number,number,number,number];
type LabelProps={text:string;layerId:string;group:string;key:string;unique:boolean;rotation?:number};
type LabelFeature=Feature<Geometry,LabelProps>;
type LabelGroup={id:string;layerId:string;kind:Kind;view:LayerView};
type LabelPlan={callouts:FeatureCollection;data:FeatureCollection<Geometry,LabelProps>;groups:LabelGroup[];bounds:Map<string,Bounds>;polygons:Map<string,{geometry:Extract<Geometry,{type:"Polygon"|"MultiPolygon"}>;bounds:Bounds;view:LayerView}>};

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
export function planGpuLabels(rows:StudioRow[],views:Record<string,LayerView>,text:(r:StudioRow,v:LayerView)=>string,anchor:(g:Geometry)=>[number,number]|null,overrides?:LabelOverrides):LabelPlan{
 const groups=new Map<string,LabelGroup>(),features:LabelFeature[]=[],callouts:Feature[]=[],variants=new Map<string,ReturnType<typeof labelVariants>>(),bounds=new Map<string,Bounds>(),polygons:LabelPlan["polygons"]=new Map(),seen=new Set<string>();
 for(const r of rows){
  const layerId=r.layer.id,baseView=views[layerId],g=r.geometry;
  if(!baseView||baseView.labelHidden||!g)continue;
  const override=overrides&&overrideFor(r,overrides);if(override?.hidden)continue;
  let layerVariants=variants.get(layerId);if(!layerVariants){layerVariants=labelVariants(baseView);variants.set(layerId,layerVariants)}
  for(const variant of layerVariants){const v=variant.view;
  if(!v?.labelField||v.labelHidden||!g||Math.max(v.labelMinZoom,v.minZoom)>=Math.min(v.labelMaxZoom,v.maxZoom))continue;
  const value=override?.text??text(r,v);if(!value)continue;
  let geometry:Geometry=g,kind:Kind;
  if(override?.position){geometry={type:"Point",coordinates:override.position};kind="point"}
  else if(g.type==="Polygon"||g.type==="MultiPolygon"){const p=polygonLabelAnchor(g);if(!p)continue;geometry={type:"Point",coordinates:p};kind="polygon";}
  else if((g.type==="LineString"||g.type==="MultiLineString")&&!override?.rotation&&(v.labelPlacement==="auto"||v.labelPlacement==="line"))kind="line";
  else if(g.type==="Point"||g.type==="MultiPoint")kind="point";
  else{
   const p=g.type==="MultiLineString"?midpoint(g.coordinates.reduce((a,b)=>a.length>=b.length?a:b,[] as Position[])):anchor(g);
   if(!p)continue;geometry={type:"Point",coordinates:p};kind="point";
  }
  const group=layerId+":"+kind+variant.id+(override?.position?":pinned":""),key=layerId+":"+r.key+variant.id;
  if(seen.has(key))continue;seen.add(key);
  if((g.type==="Polygon"||g.type==="MultiPolygon")&&v.labelPartial!==false&&!override?.position)polygons.set(key,{geometry:g,bounds:geometryBounds(g),view:v});
  if(!groups.has(group))groups.set(group,{id:group,layerId,kind,view:override?.position?{...v,labelPlacement:"center"}:v});
  features.push({type:"Feature",id:key,geometry,properties:{text:value,layerId,group,key,unique:v.labelUnique,rotation:override?.rotation??0}});
  if(v.labelUnique)bounds.set(key,geometryBounds(geometry));
  if(override?.position&&override.callout){const from=g.type==="Polygon"||g.type==="MultiPolygon"?polygonLabelAnchor(g):g.type==="MultiLineString"?midpoint(g.coordinates.reduce((a,b)=>a.length>=b.length?a:b,[] as Position[])):anchor(g);if(from&&(from[0]!==override.position[0]||from[1]!==override.position[1]))callouts.push({type:"Feature",id:key,geometry:{type:"LineString",coordinates:[from,override.position]},properties:{group}})}
  }
 }
 return {callouts:{type:"FeatureCollection",features:callouts},data:{type:"FeatureCollection",features},groups:[...groups.values()].sort((a,b)=>a.view.labelPriority-b.view.labelPriority||a.id.localeCompare(b.id)),bounds,polygons};
}
function colorImageId(v:LayerView){return PREFIX+"bg-"+v.labelHalo.slice(1)}
export function labelLayer(group:LabelGroup):SymbolLayerSpecification{
 const v=group.view,line=group.kind==="line",auto=v.labelPlacement==="auto",offset=v.labelOffset/v.labelSize;
 const anchors={above:"bottom",below:"top",left:"right",right:"left",center:"center",line:"center",auto:"center"} as const;
 const layout:NonNullable<SymbolLayerSpecification["layout"]>={
  "symbol-placement":line?"line-center":"point","text-field":["get","text"],
  "text-font":[v.labelBold?"Arial Bold":"Arial Regular"],"text-size":v.labelSize,
  "text-rotation-alignment":line?"map":"viewport","text-pitch-alignment":"viewport",
  "symbol-sort-key":10-v.labelPriority,"text-justify":"auto","text-padding":2,"text-max-width":24,"text-keep-upright":true,
  "text-allow-overlap":v.labelOverlap,"text-ignore-placement":false,
  "text-anchor":anchors[v.labelPlacement],
  ...(!line?{"text-rotate":["coalesce",["get","rotation"],0]} as NonNullable<SymbolLayerSpecification["layout"]>:{}),
 };
 if(!line&&auto){
  layout["text-variable-anchor"]=group.kind==="point"?["bottom","left","right","top","center"]:["center","bottom","top"];
  layout["text-radial-offset"]=group.kind==="point"?offset:0;
 }else if(!line&&v.labelPlacement!=="center"&&v.labelPlacement!=="line")layout["text-radial-offset"]=offset;
 if(v.labelBackground){
  layout["icon-image"]=colorImageId(v);layout["icon-text-fit"]="both";layout["icon-text-fit-padding"]=[2,4,2,4];
  layout["icon-allow-overlap"]=v.labelOverlap;layout["icon-ignore-placement"]=false;
  layout["icon-rotation-alignment"]=line?"map":"viewport";if(!line)layout["icon-rotate"]=["coalesce",["get","rotation"],0];layout["icon-pitch-alignment"]="viewport";
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
export function uniqueVisibleLabels(plan:LabelPlan,viewport:Bounds,zoom?:number):Map<string,string[]>{
 const byGroup=new Map<string,string[]>(),seen=new Set<string>(),active=new Set(plan.groups.filter(g=>zoom===undefined||(zoom>=Math.max(g.view.labelMinZoom,g.view.minZoom)&&zoom<Math.min(g.view.labelMaxZoom,g.view.maxZoom))).map(g=>g.id));
 for(const f of plan.data.features){const p=f.properties;if(!p.unique||!active.has(p.group))continue;
  const b=plan.bounds.get(p.key);if(!b||b[0]>viewport[2]||b[2]<viewport[0]||b[1]>viewport[3]||b[3]<viewport[1])continue;
  const uid=p.layerId+":"+p.text;if(seen.has(uid))continue;seen.add(uid);
  const keys=byGroup.get(p.group)??[];keys.push(p.key);byGroup.set(p.group,keys);
 }
 return byGroup;
}
export function viewportLabelPlan(plan:LabelPlan,m:Pick<MLMap,"project"|"unproject"|"getBounds"|"getCanvas"|"getZoom">):LabelPlan{
 if(!plan.polygons.size)return plan;
 const canvas=m.getCanvas(),width=canvas.clientWidth,height=canvas.clientHeight,b=m.getBounds(),view:[number,number,number,number]=[b.getWest(),b.getSouth(),b.getEast(),b.getNorth()];
 if(width<=0||height<=0)return plan;
 let remaining=150000,processed=0,changed=false;const bounds=new Map(plan.bounds);
 const features=plan.data.features.map(f=>{
  const p=plan.polygons.get(f.properties.key);if(!p||f.geometry.type!=="Point"||processed>=200)return f;
  if(m.getZoom()<Math.max(p.view.minZoom,p.view.labelMinZoom)||m.getZoom()>=Math.min(p.view.maxZoom,p.view.labelMaxZoom))return f;
  const [x0,y0,x1,y1]=p.bounds;if(x0>view[2]||x1<view[0]||y0>view[3]||y1<view[1])return f;
  const anchor=m.project([f.geometry.coordinates[0],f.geometry.coordinates[1]]),padX=Math.min(width/3,Math.max(8,p.view.labelSize*Math.min(f.properties.text.length,30)*.29+4)),padY=p.view.labelSize*.8+4;
  if(anchor.x>=padX&&anchor.x<=width-padX&&anchor.y>=padY&&anchor.y<=height-padY)return f;
  const polygons=p.geometry.type==="Polygon"?[p.geometry.coordinates]:p.geometry.coordinates;
  const count=polygons.reduce((n,r)=>n+r.reduce((a,ring)=>a+ring.length,0),0);if(count>remaining)return f;remaining-=count;processed++;
  let best:{x:number;y:number}|null=null;
  for(const rings of polygons){const screen=rings.map(r=>r.map(v=>m.project([v[0],v[1]])));const candidate=visiblePolygonAnchor(screen,width,height,padX,padY);if(candidate){best=candidate;break}}
  if(!best)return f;const coord=m.unproject([best.x,best.y]);if(!Number.isFinite(coord.lng)||!Number.isFinite(coord.lat))return f;
  const geometry={type:"Point" as const,coordinates:[coord.lng,coord.lat]};changed=true;if(f.properties.unique)bounds.set(f.properties.key,geometryBounds(geometry));return {...f,geometry};
 });
 return changed?{...plan,data:{type:"FeatureCollection",features},bounds}:plan;
}
export function createGpuLabelRenderer(m:MLMap){
 let base:LabelPlan|null=null,plan:LabelPlan|null=null,appliedData="",calloutData="",calloutLayers=new Set<string>(),layerSpecs=new Map<string,string>(),filters=new Map<string,string>(),images=new Set<string>(),disposed=false;
 const refreshUnique=()=>{
  if(!plan||disposed||!plan.bounds.size)return;
  const b=m.getBounds(),selected=uniqueVisibleLabels(plan,[b.getWest(),b.getSouth(),b.getEast(),b.getNorth()],m.getZoom());
  for(const g of plan.groups){if(!g.view.labelUnique)continue;const id=PREFIX+g.id,keys=selected.get(g.id)??[],signature=JSON.stringify(keys);
   if(filters.get(id)===signature||!m.getLayer(id))continue;
   m.setFilter(id,["all",["==",["get","group"],g.id],["in",["get","key"],["literal",keys]]]);filters.set(id,signature);
  }
 };
 const render=(next:LabelPlan)=>{
  if(disposed)return;plan=next;if(!m.getStyle())return;
  const data=JSON.stringify(next.data),source=m.getSource(SOURCE) as GeoJSONSource|undefined;
  if(!source){m.addSource(SOURCE,{type:"geojson",data:next.data,maxzoom:22,tolerance:.2,buffer:128});appliedData=data}
  else if(data!==appliedData){source.setData(next.data);appliedData=data}
  const calloutJSON=JSON.stringify(next.callouts),calloutSource=m.getSource(CALLOUT_SOURCE) as GeoJSONSource|undefined;
  if(next.callouts.features.length||calloutSource){if(!calloutSource)m.addSource(CALLOUT_SOURCE,{type:"geojson",data:next.callouts});else if(calloutJSON!==calloutData)calloutSource.setData(next.callouts);calloutData=calloutJSON}
  const groupsWithCallouts=new Set(next.callouts.features.map(f=>String(f.properties?.group))),wantedCallouts=new Set<string>();
  for(const g of next.groups){if(!groupsWithCallouts.has(g.id))continue;const id=CALLOUT_SOURCE+"-"+g.id;wantedCallouts.add(id);if(!m.getLayer(id))m.addLayer({id,source:CALLOUT_SOURCE,type:"line",filter:["==",["get","group"],g.id],minzoom:Math.max(g.view.labelMinZoom,g.view.minZoom),maxzoom:Math.min(g.view.labelMaxZoom,g.view.maxZoom),paint:{"line-color":g.view.labelColor,"line-width":1.2}},m.getLayer(PREFIX+g.id)?PREFIX+g.id:undefined);else{m.setLayerZoomRange(id,Math.max(g.view.labelMinZoom,g.view.minZoom),Math.min(g.view.labelMaxZoom,g.view.maxZoom));m.setPaintProperty(id,"line-color",g.view.labelColor)}}
  for(const id of calloutLayers)if(!wantedCallouts.has(id)&&m.getLayer(id))m.removeLayer(id);calloutLayers=wantedCallouts;
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
 // Camera animation does not touch source data; bounded partial polygon anchors refresh only after movement settles.
 const apply=(next:LabelPlan)=>{base=next;render(viewportLabelPlan(next,m))};
 const refresh=()=>{if(base&&!disposed)render(viewportLabelPlan(base,m))};
 m.on("moveend",refresh);m.on("resize",refresh);
 const dispose=()=>{disposed=true;m.off("moveend",refresh);m.off("resize",refresh);
  if(!m.getStyle())return;for(const id of layerSpecs.keys())if(m.getLayer(id))m.removeLayer(id);
  if(m.getSource(SOURCE))m.removeSource(SOURCE);for(const id of calloutLayers)if(m.getLayer(id))m.removeLayer(id);if(m.getSource(CALLOUT_SOURCE))m.removeSource(CALLOUT_SOURCE);for(const id of images)if(m.hasImage(id))m.removeImage(id);
 };
 return {apply,dispose};
}
