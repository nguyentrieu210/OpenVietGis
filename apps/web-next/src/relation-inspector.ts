import type {SpatialLayer,LayerQueryResponse} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {compareTableValue} from "./table-columns";
import {tableQueryBody,verifyTableQuery,TABLE_QUERY_LIMIT} from "./table-query";
export type RelationSpec={target:string;localField:string;targetField:string;cardinality:"one"|"many"};
export const RELATION_LIMIT=5000;
export const RELATION_MATCH_LIMIT=200;
export const readableRelationLayer=(l:SpatialLayer)=>l.lifecycle==="active"&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.technical_supported&&c.actor_authorized);
export const queryableRelationLayer=(l:SpatialLayer)=>readableRelationLayer(l)&&l.capabilities.some(c=>c.name==="query"&&c.available&&c.technical_supported&&c.actor_authorized);
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

export type RelatedRecord={feature_id:string;attributes:Record<string,unknown>};
export async function queryRelation(source:StudioRow,target:SpatialLayer,spec:RelationSpec,execute:(body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean):Promise<RelatedRecord[]>{
 if(target.id!==spec.target||!readableRelationLayer(source.layer)||!queryableRelationLayer(target))throw Error("Không có quyền đọc lớp liên kết.");
 const a=field(source.layer,spec.localField),b=field(target,spec.targetField);
 if(!a||!b||a.type!==b.type)throw Error("Hai trường liên kết phải cùng kiểu.");
 const value=scalar(Object.hasOwn(source.feature.attributes,spec.localField)?source.feature.attributes[spec.localField]:null,a.type);
 if(!current())throw Error("Nguồn hoặc quan hệ đã thay đổi.");
 if(value===null)return [];
 const matches:RelatedRecord[]=[],seen=new Set<string>();let fingerprint:string|undefined,total:number|undefined;
 for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
  if(!current())throw Error("Nguồn hoặc quan hệ đã thay đổi.");
  const body=tableQueryBody(target,{field:b.field_id,op:"eq",value:String(value),negate:false},"","asc",offset,fingerprint);
  const page=verifyTableQuery(target,await execute(body),offset,fingerprint);
  if(!current()||total!==undefined&&page.total_count!==total)throw Error("Nguồn hoặc quan hệ đã thay đổi.");
  fingerprint=page.query_fingerprint;total=page.total_count;
  if(total>RELATION_MATCH_LIMIT||spec.cardinality==="one"&&total>1)throw Error(spec.cardinality==="one"?"Khóa đích không duy nhất.":"Kết quả vượt 200 đối tượng.");
  for(const row of page.items){
   if(seen.has(row.feature_id))throw Error("Trùng FID giữa các trang.");
   const candidate=scalar(row.attributes[b.field_id],b.type);
   // Timestamp keys are server-normalized; compare canonical wire values rather than
   // millisecond-rounded JavaScript dates (server retains sub-ms precision).
   if(candidate===null||(a.type==="decimal"||a.type==="integer"?compareTableValue(value,candidate,a.type,"asc")!==0:value!==candidate))throw Error("Khóa trả về không khớp.");
   seen.add(row.feature_id);matches.push({feature_id:row.feature_id,attributes:row.attributes});
  }
  if(!page.has_more)break;
 }
 if(!current()||matches.length!==total)throw Error("Kết quả liên kết không đầy đủ.");
 return matches;
}

export type RelationAudit={sourceRows:number;targetRows:number;nullSource:number;nullTarget:number;matched:number;missing:number;ambiguous:number;duplicateTargetKeys:number;examples:{missing:string[];ambiguous:string[];duplicate:string[]}};
/** Exact scalar key identity, preserving large decimals and sub-ms timestamp strings. */
function auditKey(value:unknown,type:string):string|null{
 const v=scalar(value,type);if(v===null)return null;
 if(type==="decimal"||type==="integer"){
  const s=String(v),[w,f=""]=s.replace(/^-/,"").split("."),whole=w!.replace(/^0+/,"")||"0",fraction=f.replace(/0+$/,"");
  return (s.startsWith("-")&&(whole!=="0"||fraction)?"-":"")+whole+(fraction?"."+fraction:"");
 }
 return JSON.stringify(v);
}
/** Read-only whole-source audit. Both populations must remain revision-pinned.
 * NULL never refers to a target. A duplicate target key is a cardinality violation
 * for many-to-one even when no source row currently references it. */
export async function auditRelation(source:SpatialLayer,target:SpatialLayer,spec:RelationSpec,execute:(layer:SpatialLayer,body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean,progress:(scanned:number,total:number)=>void):Promise<RelationAudit>{
 if(target.id!==spec.target||!queryableRelationLayer(source)||!queryableRelationLayer(target))throw Error("Không đủ quyền truy vấn hai nguồn.");
 const a=field(source,spec.localField),b=field(target,spec.targetField);
 if(!a||!b||a.type!==b.type)throw Error("Hai trường liên kết phải cùng kiểu.");
 const size=(l:SpatialLayer)=>{const n=l.source.feature_count;if(n===null||!Number.isSafeInteger(n)||n<0||n>RELATION_LIMIT)throw Error("Kiểm tra quan hệ tối đa 5.000 bản ghi mỗi nguồn.");return n};
 const sourceRows=size(source),targetRows=size(target),total=sourceRows+targetRows;
 const ensure=()=>{if(!current())throw Error("Nguồn, quyền hoặc quan hệ đã thay đổi.")};
 const result:RelationAudit={sourceRows,targetRows,nullSource:0,nullTarget:0,matched:0,missing:0,ambiguous:0,duplicateTargetKeys:0,examples:{missing:[],ambiguous:[],duplicate:[]}};
 const targets=new Map<string,{count:number;first:string}>();let scanned=0,keyBytes=0;
 const scan=async(l:SpatialLayer,id:string,type:string,expected:number,visit:(key:string|null,fid:string)=>void)=>{
  const seen=new Set<string>();let fp:string|undefined;
  for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
   ensure();const body=tableQueryBody(l,{field:"",op:"eq",value:"",negate:false},"","asc",offset,fp);
   const page=verifyTableQuery(l,await execute(l,body),offset,fp);ensure();
   if(page.total_count!==expected)throw Error("Số lượng nguồn đã đổi.");
   fp=page.query_fingerprint;
   for(const r of page.items){
    if(seen.has(r.feature_id))throw Error("Trùng FID giữa các trang.");
    seen.add(r.feature_id);const key=auditKey(Object.hasOwn(r.attributes,id)?r.attributes[id]:null,type);
    keyBytes+=(key?.length??0)*2;if(keyBytes>16*1024*1024)throw Error("Khóa quan hệ vượt 16 MiB.");
    visit(key,r.feature_id);
   }
   scanned+=page.items.length;progress(scanned,total);if(!page.has_more)break;
  }
  if(seen.size!==expected)throw Error("Chưa đọc đủ nguồn.");ensure();
 };
 await scan(target,b.field_id,b.type,targetRows,(key,fid)=>{
  if(key===null){result.nullTarget++;return}
  const found=targets.get(key);if(!found){targets.set(key,{count:1,first:fid});return}
  found.count++;if(found.count===2){result.duplicateTargetKeys++;if(result.examples.duplicate.length<20)result.examples.duplicate.push(found.first)}
 });
 await scan(source,a.field_id,a.type,sourceRows,(key,fid)=>{
  if(key===null){result.nullSource++;return}
  const n=targets.get(key)?.count??0;
  if(n===0){result.missing++;if(result.examples.missing.length<20)result.examples.missing.push(fid)}
  else if(spec.cardinality==="one"&&n>1){result.ambiguous++;if(result.examples.ambiguous.length<20)result.examples.ambiguous.push(fid)}
  else result.matched++;
 });
 ensure();return result;
}
