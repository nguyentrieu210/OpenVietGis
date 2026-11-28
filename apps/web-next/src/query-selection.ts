import {parseExpression,evaluateExpression} from "./label-expr";
import {tableTsv} from "./table-columns";
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

export const SELECTED_EXPORT_MAX_SOURCE=50000;
export const SELECTED_EXPORT_MAX_BYTES=16*1024*1024;
/** Export selected FIDs from the authorized source, not just the visible page.
 * Preserve the table text format and its formula-injection protections. */
export async function exportSelectedAttributes(layer:SpatialLayer,selected:ReadonlySet<string>,execute:(body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean,progress:(count:number,total:number)=>void){
 const prefix=layer.source_id+":",ids=new Set([...selected].filter(k=>k.startsWith(prefix)).map(k=>k.slice(prefix.length)));
 const expected=layer.source.feature_count;
 if(!ids.size||ids.size>QUERY_SELECTION_LIMIT||ids.has("")||expected===null||!Number.isSafeInteger(expected)||expected<ids.size||expected>SELECTED_EXPORT_MAX_SOURCE)throw Error("Vùng chọn hoặc nguồn vượt giới hạn xuất.");
 const fields=layer.source.schema.fields;
 let bytes=0,scanned=0;const records:string[]=[];const encoder=new TextEncoder();
 const push=(line:string)=>{bytes+=encoder.encode(line).byteLength+1;if(bytes>SELECTED_EXPORT_MAX_BYTES)throw Error("Kết quả vượt 16 MiB.");records.push(line)};
 push(tableTsv(["FID","geometry_state",...fields.map(f=>f.name)],[]));
 let fingerprint:string|undefined;const seen=new Set<string>();
 const ensure=()=>{if(!current())throw Error("Nguồn, quyền hoặc vùng chọn đã thay đổi; không xuất tệp.")};
 for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
  ensure();const body=tableQueryBody(layer,{field:"",op:"eq",value:"",negate:false},"","asc",offset,fingerprint);
  const page=verifyTableQuery(layer,await execute(body),offset,fingerprint);ensure();
  if(page.total_count!==expected)throw Error("Số lượng nguồn không khớp.");
  fingerprint=page.query_fingerprint;
  for(const row of page.items){
   if(seen.has(row.feature_id))throw Error("Trùng FID giữa các trang.");seen.add(row.feature_id);
   if(ids.has(row.feature_id)){push(tableTsv([row.feature_id,row.geometry_state??"",...fields.map(f=>row.attributes[f.field_id]??"")],[]));ids.delete(row.feature_id)}
  }
  scanned+=page.items.length;progress(scanned,expected);
  if(!page.has_more)break;
  // Stop only after every selected FID has been verified in this pinned query.
  if(!ids.size)break;
 }
 ensure();if(ids.size)throw Error("Không tìm thấy đầy đủ FID đã chọn trong nguồn hiện tại.");
 return {tsv:records.join("\n"),count:records.length-1};
}

export type SpatialSelectionPredicate="intersects"|"within"|"contains";
export type SpatialSelectionInput={reference:SpatialLayer}|{bounds:[number,number,number,number]};
export async function collectSpatialSelection(layer:SpatialLayer,input:SpatialSelectionInput,predicate:SpatialSelectionPredicate,execute:(body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean,progress:(count:number,total:number)=>void){
 if(!["intersects","within","contains"].includes(predicate))throw Error("Quan hệ không gian không hợp lệ.");
 let spatial:Record<string,unknown>;
 if("reference" in input){
  const r=input.reference;
  if(!r.id||!selectionLayerCurrent(r,r))throw Error("Lớp tham chiếu không đủ quyền.");
  spatial={predicate,reference_layer_id:r.id};
 }else{
  const [w,s,e,n]=input.bounds;
  if(![w,s,e,n].every(Number.isFinite)||w < -180||e>180||s < -85.05112878||n>85.05112878||w>=e||s>=n||e-w>180)throw Error("Khung tọa độ WGS84 không hợp lệ.");
  spatial={predicate,crs_code:4326,geometry:{type:"Polygon",coordinates:[[[w,s],[e,s],[e,n],[w,n],[w,s]]]}};
 }
 const keys=new Set<string>();let fp:string|undefined,total:number|undefined;
 const ensure=()=>{if(!current())throw Error("Nguồn, tham chiếu hoặc vùng chọn đã thay đổi.")};
 for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
  ensure();
  const page=await execute({spatial_filter:spatial,expected_content_revision:layer.source.content_revision,expected_schema_revision:layer.source.schema_revision,limit:TABLE_QUERY_LIMIT,offset,sort_direction:"asc",...(fp?{expected_query_fingerprint:fp}:{})});ensure();
  if(!page||page.attribute_only||page.layer_id!==layer.id||page.source_id!==layer.source_id||page.content_revision!==layer.source.content_revision||page.schema_revision!==layer.source.schema_revision||page.offset!==offset||page.limit!==TABLE_QUERY_LIMIT||!Number.isSafeInteger(page.total_count)||page.total_count<0||page.total_count>QUERY_SELECTION_LIMIT||!page.query_fingerprint||typeof page.query_fingerprint!=="string"||page.query_fingerprint.length>512||fp&&fp!==page.query_fingerprint||total!==undefined&&total!==page.total_count||!Array.isArray(page.selection_feature_ids)||!Array.isArray(page.items)||page.items.length!==Math.min(TABLE_QUERY_LIMIT,Math.max(0,page.total_count-offset))||page.selection_feature_ids.length!==page.items.length||page.has_more!==(offset+page.items.length<page.total_count))throw Error("Kết quả không gian không khớp phiên bản hoặc vượt 5.000 FID.");
  const pin=page.reference_pin;
  if("reference" in input){const r=input.reference;if(!pin||pin.layer_id!==r.id||pin.source_id!==r.source_id||pin.content_revision!==r.source.content_revision||pin.schema_revision!==r.source.schema_revision)throw Error("Lớp tham chiếu đổi revision.");}
  else if(pin!=null)throw Error("Máy chủ trả về tham chiếu ngoài yêu cầu.");
  fp=page.query_fingerprint;total=page.total_count;
  for(let i=0;i<page.items.length;i++){
   const id=page.selection_feature_ids[i],item=page.items[i];
   if(typeof id!=="string"||!id||id.length>512||!item||item.feature_id!==id||!item.geometry)throw Error("FID hoặc hình học không hợp lệ.");
   const key=layer.source_id+":"+id;if(keys.has(key))throw Error("Trùng FID giữa các trang.");keys.add(key);
  }
  progress(keys.size,total);if(!page.has_more)break;
 }
 ensure();if(keys.size!==total)throw Error("Kết quả không đầy đủ.");
 return {keys,missingGeometry:0};
}
