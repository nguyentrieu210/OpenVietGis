import type {SpatialLayer} from "@openvietgis/client-sdk";
import {attributeValue} from "./attribute-value";

export type ProjectRelation={
 id:string;organization_id:string;project_id:string;source_id:string;local_field:string;
 target_source_id:string;target_field:string;cardinality:"one"|"many";field_type:string
};
const fields=new Set(["text","integer","decimal","boolean","timestamp"]);
const fieldId=(v:unknown):v is string=>typeof v==="string"&&/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(v);
const id=(v:unknown):v is string=>typeof v==="string"&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v);
export function relationRules(raw:unknown,org:string,project:string,source:string):ProjectRelation[]{
 if(!raw||typeof raw!=="object"||!Array.isArray((raw as {items?:unknown}).items))throw Error("Phản hồi ràng buộc không hợp lệ.");
 const items=(raw as {items:unknown[]}).items;
 if(items.length>100)throw Error("Vượt giới hạn ràng buộc dự án.");
 const seen=new Set<string>();
 return items.map(item=>{
  if(!item||typeof item!=="object"||Array.isArray(item))throw Error("Ràng buộc không hợp lệ.");
  const v=item as ProjectRelation;
  if(v.organization_id!==org||v.project_id!==project||!id(v.id)||seen.has(v.id)||!id(v.source_id)||!id(v.target_source_id)||!fieldId(v.local_field)||!fieldId(v.target_field)||!fields.has(v.field_type)||!["one","many"].includes(v.cardinality))throw Error("Ràng buộc không khớp phạm vi hoặc schema.");
  seen.add(v.id);return v;
 }).filter(r=>r.source_id===source);
}
export const relationLayerPin=(l:SpatialLayer)=>JSON.stringify([l.id,l.source_id,l.source.schema_revision,l.source.content_revision,l.lifecycle,l.source.lifecycle,l.source.validation_state,l.capabilities]);
export function relationOptions(rule:ProjectRelation,target:SpatialLayer,rows:readonly {feature_id:string;attributes:Record<string,unknown>}[]){
 const field=target.source.schema.fields.find(f=>f.field_id===rule.target_field);
 if(target.source_id!==rule.target_source_id||!field||field.type!==rule.field_type)throw Error("Trường đích đã thay đổi.");
 const options=new Map<string,{value:string;count:number;first:string}>();
 for(const row of rows){
  const v=row.attributes[rule.target_field];if(v===null||v===undefined)continue;
  if(typeof v!=="string"&&typeof v!=="number"&&typeof v!=="boolean")throw Error("Khóa đích không phải giá trị vô hướng.");
  const value=String(attributeValue(field,String(v)));
  const existing=options.get(value);
  if(existing){existing.count++;if(rule.cardinality==="one")throw Error("Khóa đích không duy nhất.");}
  else options.set(value,{value,count:1,first:row.feature_id});
 }
 return [...options.values()];
}
