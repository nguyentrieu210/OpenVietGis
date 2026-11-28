import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {compareTableValue} from "./table-columns";
export type RelationSpec={target:string;localField:string;targetField:string;cardinality:"one"|"many"};
export const RELATION_LIMIT=5000;
export const RELATION_MATCH_LIMIT=200;
export const readableRelationLayer=(l:SpatialLayer)=>l.lifecycle==="active"&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.technical_supported&&c.actor_authorized);
const field=(l:SpatialLayer,id:string)=>l.source.schema.fields.find(f=>f.field_id===id);
const samePin=(a:SpatialLayer,b:SpatialLayer)=>a.id===b.id&&a.source_id===b.source_id&&a.source.content_revision===b.source.content_revision&&a.source.schema_revision===b.source.schema_revision;
export function readRelationSpec(raw:unknown):RelationSpec|null{
 if(!raw||typeof raw!=="object"||Array.isArray(raw))return null;
 const v=raw as Partial<RelationSpec>;
 if(![v.target,v.localField,v.targetField].every(x=>typeof x==="string"&&x.length>0&&x.length<=256)||!(v.cardinality==="one"||v.cardinality==="many"))return null;
 return v as RelationSpec;
}
function scalar(v:unknown,type:string){
 if(v===null||v===undefined)return null;
 if(type==="integer"){if(typeof v==="number"&&Number.isSafeInteger(v)||typeof v==="string"&&/^-?\d{1,38}$/.test(v))return v}
 else if(type==="decimal"){if(typeof v==="number"&&Number.isSafeInteger(v)||typeof v==="string"&&/^-?\d{1,38}(?:\.\d{1,38})?$/.test(v))return v}
 else if(type==="boolean"){if(typeof v==="boolean")return v}
 else if(type==="text"||type==="timestamp"){if(typeof v==="string"&&v.length<=16384)return v}
 throw Error("Giá trị khóa liên kết không hợp lệ.");
}
export function relationMatches(source:StudioRow,layers:readonly SpatialLayer[],rows:readonly StudioRow[],spec:RelationSpec,restricted:boolean){
 if(restricted)throw Error("Liên kết cục bộ cần quyền đọc toàn nguồn.");
 const current=layers.find(l=>l.id===source.layer.id),target=layers.find(l=>l.id===spec.target);
 if(!current||!target||!samePin(current,source.layer)||!readableRelationLayer(current)||!readableRelationLayer(target))throw Error("Lớp hoặc quyền đã thay đổi.");
 const a=field(current,spec.localField),b=field(target,spec.targetField);
 if(!a||!b||a.type!==b.type)throw Error("Hai trường liên kết phải cùng kiểu.");
 const count=target.source.feature_count;
 if(count===null||!Number.isSafeInteger(count)||count<0||count>RELATION_LIMIT)throw Error("Lớp đích phải có tối đa 5.000 đối tượng đã xác minh.");
 const all=rows.filter(r=>r.layer.id===target.id);
 if(all.length!==count||new Set(all.map(r=>r.feature.feature_id)).size!==count||all.some(r=>!samePin(r.layer,target)||!r.feature.feature_id))throw Error("Lớp đích chưa được tải đủ đúng revision.");
 const value=scalar(Object.hasOwn(source.feature.attributes,spec.localField)?source.feature.attributes[spec.localField]:null,a.type);
 if(value===null)return [];
 const matches=all.filter(r=>{const candidate=scalar(Object.hasOwn(r.feature.attributes,spec.targetField)?r.feature.attributes[spec.targetField]:null,b.type);return candidate!==null&&(a.type==="integer"||a.type==="decimal"?compareTableValue(value,candidate,a.type,"asc")===0:value===candidate)});
 if(spec.cardinality==="one"&&matches.length>1)throw Error("Khóa đích không duy nhất đối với giá trị đang chọn.");
 if(matches.length>RELATION_MATCH_LIMIT)throw Error("Kết quả vượt 200 đối tượng; chọn khóa cụ thể hơn.");
 return matches;
}
