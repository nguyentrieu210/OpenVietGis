import type {ProcessingAlgorithm,ProcessingJob,SpatialLayer} from "@openvietgis/client-sdk";
export type Algorithm=ProcessingAlgorithm["id"];
export type ProcessingRequest={algorithm_id:Algorithm;input_layer_id:string;overlay_layer_id:string|null;parameters:Record<string,unknown>;output_name:string;operation_id:string};
export const supportedAlgorithm=(id:unknown):id is Algorithm=>id==="buffer"||id==="clip"||id==="dissolve";
export const processingLayer=(l:SpatialLayer)=>l.lifecycle==="active"&&l.source.lifecycle==="active"&&l.source.validation_state==="ready"&&!!l.source.stored_crs?.code&&l.source.feature_count!=null&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized);
export function processingRequest(algo:Algorithm,layer:SpatialLayer|undefined,overlay:SpatialLayer|undefined,distance:string,segments:string,group:string,name:string,operation:string):ProcessingRequest{
 if(!layer||!processingLayer(layer))throw Error("Chọn lớp đầu vào đang hoạt động và dữ liệu sẵn sàng.");
 if(!name.trim()||Array.from(name.trim()).length>200)throw Error("Tên lớp kết quả phải có 1–200 ký tự.");
 let parameters:Record<string,unknown>={};
 if(algo==="clip"&&(!overlay||!processingLayer(overlay)||overlay.id===layer.id||overlay.source_id===layer.source_id||overlay.source.schema.geometry_family!=="Polygon"))throw Error("Chọn lớp phủ Polygon có nguồn khác đầu vào.");
 if(algo==="buffer"){
  const d=Number(distance),s=Number(segments);
  if(!distance.trim()||!Number.isFinite(d)||d<.1||d>500000||!/^\d+$/.test(segments)||s<2||s>16)throw Error("Khoảng cách 0,1–500.000 m; số đoạn nguyên 2–16.");
  parameters={distance_meters:d,quad_segs:s};
 }
 if(algo==="dissolve"){
  if(group&&!layer.source.schema.fields.some(f=>f.field_id===group))throw Error("Trường gom nhóm không thuộc schema đầu vào.");
  parameters={group_by_field:group||null};
 }
 return {algorithm_id:algo,input_layer_id:layer.id,overlay_layer_id:algo==="clip"?overlay!.id:null,parameters,output_name:name.trim(),operation_id:operation};
}
export function verifyProcessingReceipt(job:ProcessingJob,request:ProcessingRequest){
 if(!job.id||job.operation_id!==request.operation_id||job.algorithm_id!==request.algorithm_id||job.output_name!==request.output_name||!["queued","running","succeeded","failed","cancelled"].includes(job.status))throw Error("Biên nhận tác vụ không khớp. Thử lại cùng mã thao tác.");
 return job;
}
export function readPendingProcessing(raw:string|null):ProcessingRequest|null{
 try{
  const x=JSON.parse(raw??"null");if(!x||!supportedAlgorithm(x.algorithm_id)||typeof x.input_layer_id!=="string"||!x.input_layer_id||typeof x.operation_id!=="string"||!/^[0-9a-f-]{36}$/i.test(x.operation_id)||typeof x.output_name!=="string"||!x.output_name.trim()||Array.from(x.output_name).length>200||!x.parameters||typeof x.parameters!=="object"||Array.isArray(x.parameters))return null;
  let parameters:Record<string,unknown>={};const p=x.parameters;
  if(x.algorithm_id==="buffer"){if(typeof p.distance_meters!=="number"||!Number.isFinite(p.distance_meters)||p.distance_meters<.1||p.distance_meters>500000||!Number.isInteger(p.quad_segs)||p.quad_segs<2||p.quad_segs>16)return null;parameters={distance_meters:p.distance_meters,quad_segs:p.quad_segs}}
  if(x.algorithm_id==="dissolve"){if(p.group_by_field!==null&&typeof p.group_by_field!=="string")return null;parameters={group_by_field:p.group_by_field}}
  if(x.algorithm_id==="clip"&&(typeof x.overlay_layer_id!=="string"||!x.overlay_layer_id||x.overlay_layer_id===x.input_layer_id))return null;
  return {algorithm_id:x.algorithm_id,input_layer_id:x.input_layer_id,overlay_layer_id:x.algorithm_id==="clip"?x.overlay_layer_id:null,parameters,output_name:x.output_name,operation_id:x.operation_id};
 }catch{return null}
}
