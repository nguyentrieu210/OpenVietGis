import {parseExpression,evaluateExpression} from "./label-expr";
import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import type {FilterRow} from "./workbenches/features/platform/query-filters";
import {tableQueryBody,verifyTableQuery,TABLE_QUERY_LIMIT} from "./table-query";
export const QUERY_SELECTION_LIMIT=5000;
export type QuerySelectionMode="replace"|"add"|"remove"|"intersect"|"invert";
export const sameKeys=(a:ReadonlySet<string>,b:ReadonlySet<string>)=>a.size===b.size&&[...a].every(k=>b.has(k));
export function selectionLayerCurrent(current:SpatialLayer|undefined,base:SpatialLayer){
 return !!current&&current.lifecycle==="active"&&current.id===base.id&&current.source_id===base.source_id&&current.source.content_revision===base.source.content_revision&&current.source.schema_revision===base.source.schema_revision&&["query","feature_read"].every(name=>current.capabilities.some(c=>c.name===name&&c.available&&c.technical_supported&&c.actor_authorized));
}
export function combineQuerySelection(before:ReadonlySet<string>,source:string,matches:ReadonlySet<string>,mode:QuerySelectionMode){
 if(!source||source.includes(":"))throw Error("Nguồn không hợp lệ.");
 const prefix=source+":",next=new Set(before),belongs=(k:string)=>k.startsWith(prefix);
 if([...matches].some(k=>!belongs(k)||k.length===prefix.length))throw Error("Vùng chọn không khớp nguồn.");
 if(mode==="replace"||mode==="intersect"||mode==="invert")for(const k of before)if(belongs(k))next.delete(k);
 for(const k of matches){
  if(mode==="remove")next.delete(k);
  else if(mode==="intersect"?before.has(k):mode==="invert"?!before.has(k):true)next.add(k);
 }
 if(next.size>QUERY_SELECTION_LIMIT)throw Error("Tổng vùng chọn vượt 5.000 đối tượng.");
 return next;
}
export async function collectQuerySelection(layer:SpatialLayer,preview:LayerQueryResponse,filters:FilterRow[],sort:string,direction:"asc"|"desc",group:"and"|"or",mode:QuerySelectionMode,execute:(body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean,progress:(count:number,total:number)=>void){
 const invert=mode==="invert",expected=invert?layer.source.feature_count:preview.total_count;
 if(expected===null||!Number.isSafeInteger(expected)||expected<0||expected>QUERY_SELECTION_LIMIT)throw Error("Chọn tối đa 5.000 đối tượng đã xác minh mỗi lần.");
 const rows=invert?[{field:"",op:"eq" as const,value:"",negate:false}]:filters;
 const keys=new Set<string>();let fp=invert?undefined:preview.query_fingerprint,missingGeometry=0;
 if(!invert&&!fp)throw Error("Chưa có fingerprint truy vấn.");
 const ensure=()=>{if(!current())throw Error("Nguồn, bộ lọc hoặc vùng chọn đã thay đổi; không áp dụng kết quả.")};
 for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
  ensure();const body=tableQueryBody(layer,rows,invert?"":sort,invert?"asc":direction,offset,fp,invert?"and":group);
  const r=verifyTableQuery(layer,await execute(body),offset,fp);ensure();
  if(r.total_count!==expected)throw Error("Số lượng truy vấn đã thay đổi.");
  fp=r.query_fingerprint;
  for(const row of r.items){
   if(typeof row.feature_id!=="string"||!row.feature_id||row.feature_id.length>512)throw Error("FID không hợp lệ.");
   const key=layer.source_id+":"+row.feature_id;
   if(keys.has(key))throw Error("Trùng FID giữa các trang.");
   keys.add(key);if(row.geometry_state!=="present")missingGeometry++;
  }
  progress(keys.size,expected);if(!r.has_more)break;
 }
 if(keys.size!==expected)throw Error("Vùng chọn chưa đầy đủ.");
 return {keys,missingGeometry};
}

/** A local typed predicate over a complete revision-pinned server population.
 * Never sends expression source to SQL or mutates selection on partial failure. */
export async function collectExpressionSelection(layer:SpatialLayer,expression:string,execute:(body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean,progress:(count:number,total:number)=>void){
 const expected=layer.source.feature_count;
 if(expected===null||!Number.isSafeInteger(expected)||expected<0||expected>QUERY_SELECTION_LIMIT)throw Error("Biểu thức chỉ hỗ trợ nguồn đã xác minh tối đa 5.000 đối tượng.");
 const ast=parseExpression(expression,layer.source.schema.fields.map(f=>f.field_id));
 const keys=new Set<string>(),seen=new Set<string>();let fingerprint:string|undefined,missingGeometry=0;
 const ensure=()=>{if(!current())throw Error("Nguồn hoặc vùng chọn đã thay đổi; không áp dụng kết quả.")};
 for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
  ensure();
  const body=tableQueryBody(layer,{field:"",op:"eq",value:"",negate:false},"","asc",offset,fingerprint);
  const page=verifyTableQuery(layer,await execute(body),offset,fingerprint);ensure();
  if(page.total_count!==expected)throw Error("Số lượng nguồn đã thay đổi.");
  fingerprint=page.query_fingerprint;
  for(const row of page.items){
   if(seen.has(row.feature_id))throw Error("Trùng FID giữa các trang.");
   seen.add(row.feature_id);
   const result=evaluateExpression(ast,row.attributes);
   if(result!==null&&typeof result!=="boolean")throw Error("Biểu thức chọn phải trả về Boolean hoặc NULL.");
   if(result===true){keys.add(layer.source_id+":"+row.feature_id);if(row.geometry_state!=="present")missingGeometry++}
  }
  progress(seen.size,expected);if(!page.has_more)break;
 }
 ensure();if(seen.size!==expected)throw Error("Nguồn chưa được tải đủ.");
 return {keys,missingGeometry};
}
