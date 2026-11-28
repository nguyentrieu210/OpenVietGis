import type {ProcessingAlgorithm,ProcessingJob,SpatialLayer} from "@openvietgis/client-sdk";
export type Algorithm=ProcessingAlgorithm["id"];
export type ProcessingSourcePin={source_id:string;content_revision:string;schema_revision:string};
export type ProcessingRequest={algorithm_id:Algorithm;input_layer_id:string;overlay_layer_id:string|null;parameters:Record<string,unknown>;output_name:string;operation_id:string;expected_input?:ProcessingSourcePin;expected_overlay?:ProcessingSourcePin};
export const supportedAlgorithm=(id:unknown):id is Algorithm=>id==="buffer"||id==="clip"||id==="dissolve"||id==="difference"||id==="intersection"||id==="union"||id==="reproject"||id==="spatial_join";
export const NATIVE_VN2000_CRSS=[4756,3405,3406,5896,5897,5898,5899,9205,9206,9207,9208,9209,9210,9211,9212,9213,9214,9215,9216,9217,9218] as const;
export const REPROJECT_CRSS=[4326,3857,...NATIVE_VN2000_CRSS] as const;
export function reprojectTargets(native:number|undefined):number[]{
 if(native===4326)return [4326,3857,4756,5899];if(native===3857)return [4326,3857];if(native===4756||native===5899)return [4326,...NATIVE_VN2000_CRSS.filter(c=>c!==native)];if(NATIVE_VN2000_CRSS.some(c=>c===native))return NATIVE_VN2000_CRSS.filter(c=>c!==native);return [];
}
export const overlayAlgorithm=(id:Algorithm)=>id==="clip"||id==="difference"||id==="intersection"||id==="union"||id==="spatial_join";
export const processingLayer=(l:SpatialLayer)=>l.lifecycle==="active"&&l.source.lifecycle==="active"&&l.source.validation_state==="ready"&&!!l.source.stored_crs?.code&&l.source.feature_count!=null&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized);
const revision=(v:unknown):v is string=>typeof v==="string"&&/^[a-f0-9]{64}$/.test(v);
export function sourcePin(l:SpatialLayer):ProcessingSourcePin{
 if(!l.source_id||!revision(l.source.content_revision)||!revision(l.source.schema_revision))throw Error("Thiếu phiên bản nguồn; tải lại danh mục.");
 return {source_id:l.source_id,content_revision:l.source.content_revision,schema_revision:l.source.schema_revision};
}
function readPin(raw:unknown):ProcessingSourcePin|null{
 if(!raw||typeof raw!=="object")return null;
 const p=raw as ProcessingSourcePin;
 return typeof p.source_id==="string"&&!!p.source_id&&revision(p.content_revision)&&revision(p.schema_revision)?{source_id:p.source_id,content_revision:p.content_revision,schema_revision:p.schema_revision}:null;
}
export function processingRequest(algo:Algorithm,layer:SpatialLayer|undefined,overlay:SpatialLayer|undefined,distance:string,segments:string,group:string,name:string,operation:string,options:{targetSrid?:number;predicate?:string;joinType?:string;matchMode?:string}={}):ProcessingRequest{
 if(!layer||!processingLayer(layer))throw Error("Chọn lớp đầu vào đang hoạt động và dữ liệu sẵn sàng.");
 if(!name.trim()||Array.from(name.trim()).length>200)throw Error("Tên lớp kết quả phải có 1–200 ký tự.");
 let parameters:Record<string,unknown>={};
 if(overlayAlgorithm(algo)&&(!overlay||!processingLayer(overlay)||overlay.id===layer.id||overlay.source_id===layer.source_id||(algo!=="spatial_join"&&overlay.source.schema.geometry_family!=="Polygon")))throw Error("Chọn lớp phủ Polygon có nguồn khác đầu vào.");
 if(algo==="union"&&layer.source.schema.geometry_family!=="Polygon")throw Error("Union cần hai lớp Polygon.");
 if(algo==="reproject"){
  if(!reprojectTargets(layer.source.stored_crs?.code).includes(options.targetSrid??0))throw Error("Chọn cặp CRS có phép chuyển đổi được duyệt. Đổi datum cần dữ liệu kiểm chứng; đổi phép chiếu trong cùng datum phải nằm trong vùng EPSG.");
  parameters={target_srid:options.targetSrid};
 }
 if(algo==="spatial_join"){
  const predicate=options.predicate??"intersects",join_type=options.joinType??"left",match_mode=options.matchMode??"all";
  if(!["intersects","within","contains","touches","overlaps","crosses","equals"].includes(predicate)||!["left","inner"].includes(join_type)||!["all","first"].includes(match_mode))throw Error("Tham số nối không gian không hợp lệ.");
  parameters={predicate,join_type,match_mode};
 }
 if(algo==="buffer"){
  const d=Number(distance),s=Number(segments);
  if(!distance.trim()||!Number.isFinite(d)||d<.1||d>500000||!/^\d+$/.test(segments)||s<2||s>16)throw Error("Khoảng cách 0,1–500.000 m; số đoạn nguyên 2–16.");
  parameters={distance_meters:d,quad_segs:s};
 }
 if(algo==="dissolve"){
  if(group&&!layer.source.schema.fields.some(f=>f.field_id===group))throw Error("Trường gom nhóm không thuộc schema đầu vào.");
  parameters={group_by_field:group||null};
 }
 return {algorithm_id:algo,input_layer_id:layer.id,overlay_layer_id:overlayAlgorithm(algo)?overlay!.id:null,parameters,output_name:name.trim(),operation_id:operation,expected_input:sourcePin(layer),...(overlayAlgorithm(algo)?{expected_overlay:sourcePin(overlay!)}:{})};
}
export function verifyProcessingReceipt(job:ProcessingJob,request:ProcessingRequest){
 if(!job.id||job.operation_id!==request.operation_id||job.algorithm_id!==request.algorithm_id||job.output_name!==request.output_name||!["queued","running","succeeded","failed","cancelled"].includes(job.status))throw Error("Biên nhận tác vụ không khớp. Thử lại cùng mã thao tác.");
 const matches=(p:ProcessingSourcePin|undefined,prefix:string)=>!p||!!job.input_refs&&job.input_refs[prefix+"_source_id"]===p.source_id&&job.input_refs[prefix+"_content_revision"]===p.content_revision&&job.input_refs[prefix+"_schema_revision"]===p.schema_revision;
 if(!matches(request.expected_input,"input")||!matches(request.expected_overlay,"overlay"))throw Error("Biên nhận không khớp revision nguồn.");
 if(request.algorithm_id==="reproject"&&job.output_crs!==request.parameters.target_srid)throw Error("Biên nhận không khớp CRS đích.");
 return job;
}
export function readPendingProcessing(raw:string|null):ProcessingRequest|null{
 try{
  const x=JSON.parse(raw??"null");if(!x||!supportedAlgorithm(x.algorithm_id)||typeof x.input_layer_id!=="string"||!x.input_layer_id||typeof x.operation_id!=="string"||!/^[0-9a-f-]{36}$/i.test(x.operation_id)||typeof x.output_name!=="string"||!x.output_name.trim()||Array.from(x.output_name).length>200||!x.parameters||typeof x.parameters!=="object"||Array.isArray(x.parameters))return null;
  let parameters:Record<string,unknown>={};const p=x.parameters;
  if(x.algorithm_id==="buffer"){if(typeof p.distance_meters!=="number"||!Number.isFinite(p.distance_meters)||p.distance_meters<.1||p.distance_meters>500000||!Number.isInteger(p.quad_segs)||p.quad_segs<2||p.quad_segs>16)return null;parameters={distance_meters:p.distance_meters,quad_segs:p.quad_segs}}
  if(x.algorithm_id==="dissolve"){if(p.group_by_field!==null&&typeof p.group_by_field!=="string")return null;parameters={group_by_field:p.group_by_field}}
  if(x.algorithm_id==="reproject"){if(!REPROJECT_CRSS.some(code=>code===p.target_srid))return null;parameters={target_srid:p.target_srid}}
  if(x.algorithm_id==="spatial_join"){if(!["intersects","within","contains","touches","overlaps","crosses","equals"].includes(p.predicate)||!["left","inner"].includes(p.join_type)||!["all","first"].includes(p.match_mode))return null;parameters={predicate:p.predicate,join_type:p.join_type,match_mode:p.match_mode}}
  if(Object.keys(p).length!==Object.keys(parameters).length||Object.keys(p).some(k=>!Object.hasOwn(parameters,k)))return null;
  if(overlayAlgorithm(x.algorithm_id)&&(typeof x.overlay_layer_id!=="string"||!x.overlay_layer_id||x.overlay_layer_id===x.input_layer_id))return null;
  if(!overlayAlgorithm(x.algorithm_id)&&x.overlay_layer_id!==null)return null;
  const input=readPin(x.expected_input),overlay=readPin(x.expected_overlay);
  if(x.expected_input!==undefined&&!input||x.expected_overlay!==undefined&&!overlay||overlay&&(!input||!overlayAlgorithm(x.algorithm_id)||overlay.source_id===input.source_id)||input&&overlayAlgorithm(x.algorithm_id)&&!overlay)return null;
  return {algorithm_id:x.algorithm_id,input_layer_id:x.input_layer_id,overlay_layer_id:overlayAlgorithm(x.algorithm_id)?x.overlay_layer_id:null,parameters,output_name:x.output_name,operation_id:x.operation_id,...(input?{expected_input:input}:{}),...(overlay?{expected_overlay:overlay}:{})};
 }catch{return null}
}
