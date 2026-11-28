import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {categoryColor,categoriesFor,matchesLayerView,type LayerView} from "./LayerProperties";
import {ruleLegend} from "./rule-style";
import {gradLegend,validBreaks} from "./graduated";
export type LegendEntry={id:string;text:string;color?:string;family?:string;heading?:boolean};
export function layoutLegend(layers:SpatialLayer[],rows:StudioRow[],views:Record<string,LayerView>,color:(l:SpatialLayer)=>string,zoom?:number):LegendEntry[]{
 const entries:LegendEntry[]=[];
 for(const l of layers){
  if(l.lifecycle!=="active"||!l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized))continue;
  const v=views[l.id];if(v&&zoom!==undefined&&(zoom<v.minZoom||zoom>v.maxZoom))continue;
  const family=l.source.schema.geometry_family,loaded=rows.filter(r=>r.layer.id===l.id),visible=loaded.filter(r=>matchesLayerView(r,v));
  const partial=loaded.length!==l.source.feature_count;
  if(v?.renderer==="categorized"||v?.renderer==="graduated"||v?.renderer==="rules"){
   entries.push({id:l.id,text:l.name+(partial?" (mẫu đã tải)":""),heading:true});
   const cats=v.renderer==="rules"?ruleLegend(visible,v.rules??[],l.source.schema.fields,color(l)):v.renderer==="categorized"?categoriesFor(visible,v.categoryField).map(([key])=>({key,color:categoryColor(key,v)})):validBreaks(v.gradBreaks)?gradLegend(visible,v):[];
   for(const [index,c] of cats.entries())if(!v.hiddenCats.includes(c.key))entries.push({id:l.id+":"+index,text:"label" in c?String(c.label):c.key,color:c.color,family});
   if(!cats.length)entries.push({id:l.id+":empty",text:"Không có lớp ký hiệu đã tải"});
  }else entries.push({id:l.id,text:l.name,color:color(l),family});
 }
 return entries;
}
