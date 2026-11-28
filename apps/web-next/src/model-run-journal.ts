import type {ProcessingJob,SpatialLayer} from "@openvietgis/client-sdk";
import {processingRequest,readPendingProcessing,sourcePin,overlayAlgorithm,verifyProcessingReceipt,type Algorithm,type ProcessingRequest,type ProcessingSourcePin} from "./workbenches/features/platform/processing-request";
export type ModelRef={type:"layer"|"step";id:string};
export type VectorStep={id:string;algorithm:Algorithm;name:string;a:ModelRef|null;b:ModelRef|null;parameters:Record<string,unknown>};
type Entry={step:string;request:ProcessingRequest;jobId?:string};
export type ModelRun={version:1;graph:string;runId:string;sources:Record<string,ProcessingSourcePin>;entries:Entry[]};
const uuid=(x:unknown):x is string=>typeof x==="string"&&/^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i.test(x);
const object=(x:unknown):x is Record<string,unknown>=>!!x&&typeof x==="object"&&!Array.isArray(x);
export function graphPin(steps:readonly VectorStep[]){
 if(!steps.length||steps.length>12||new Set(steps.map(n=>n.id)).size!==steps.length)throw Error("Graph không hợp lệ.");
 return JSON.stringify(steps.map(({id,algorithm,name,a,b,parameters})=>({id,algorithm,name,a,b,parameters})));
}
export function newModelRun(steps:readonly VectorStep[],layers:readonly SpatialLayer[],runId:string):ModelRun{
 if(!uuid(runId))throw Error("Mã lần chạy không hợp lệ.");
 const sources:Record<string,ProcessingSourcePin>=Object.create(null);
 for(const n of steps)for(const ref of [n.a,overlayAlgorithm(n.algorithm)?n.b:null])if(ref?.type==="layer"){
  const layer=layers.find(l=>l.id===ref.id);if(!layer)throw Error("Thiếu layer đầu vào.");
  sources[ref.id]=sourcePin(layer);
 }
 return {version:1,graph:graphPin(steps),runId,sources,entries:[]};
}
export function readModelRun(raw:string|null):ModelRun|null{
 if(raw===null)return null;
 if(raw.length>400000)throw Error("Nhật ký Modeler vượt giới hạn.");
 const r:unknown=JSON.parse(raw);
 if(!object(r)||r.version!==1||typeof r.graph!=="string"||r.graph.length>200000||!uuid(r.runId)||!object(r.sources)||Object.keys(r.sources).length>24||!Array.isArray(r.entries)||r.entries.length>12)throw Error("Nhật ký Modeler không hợp lệ.");
 const sources:Record<string,ProcessingSourcePin>=Object.create(null);
 for(const [id,p] of Object.entries(r.sources)){if(!id||!object(p)||typeof p.source_id!=="string"||!p.source_id||typeof p.content_revision!=="string"||!/^[a-f0-9]{64}$/.test(p.content_revision)||typeof p.schema_revision!=="string"||!/^[a-f0-9]{64}$/.test(p.schema_revision))throw Error("Pin nguồn không hợp lệ.");sources[id]={source_id:p.source_id,content_revision:p.content_revision,schema_revision:p.schema_revision}}
 const entries:Entry[]=[],seen=new Set<string>(),ops=new Set<string>();
 for(const e of r.entries){if(!object(e)||typeof e.step!=="string"||!e.step||seen.has(e.step)||e.jobId!==undefined&&(typeof e.jobId!=="string"||!e.jobId))throw Error("Bước nhật ký không hợp lệ.");const request=readPendingProcessing(JSON.stringify(e.request));if(!request||!request.expected_input||overlayAlgorithm(request.algorithm_id)&&!request.expected_overlay||ops.has(request.operation_id))throw Error("Yêu cầu nhật ký không hợp lệ.");seen.add(e.step);ops.add(request.operation_id);entries.push({step:e.step,request,...(typeof e.jobId==="string"?{jobId:e.jobId}:{})})}
 return {version:1,graph:r.graph,runId:r.runId,sources,entries};
}
type Runner={current:()=>Promise<void>;layer:(id:string)=>Promise<SpatialLayer>;submit:(r:ProcessingRequest)=>Promise<ProcessingJob>;job:(id:string)=>Promise<ProcessingJob>;persist:(r:ModelRun)=>void;progress:(step:string,job?:ProcessingJob)=>void;stopped:()=>boolean;operation:()=>string};
const equalPin=(a:ProcessingSourcePin,b:ProcessingSourcePin)=>a.source_id===b.source_id&&a.content_revision===b.content_revision&&a.schema_revision===b.schema_revision;
export async function executeVectorModel(initial:ModelRun,steps:readonly VectorStep[],io:Runner){
 if(initial.graph!==graphPin(steps)||initial.entries.some((e,i)=>e.step!==steps[i]?.id))throw Error("Graph đã đổi; mở đúng mô hình để tiếp tục.");
 let run=initial,finished=0;
 const outputs=new Map<string,string>(),resolve=(r:ModelRef|null)=>r?(r.type==="layer"?r.id:outputs.get(r.id)):undefined;
 for(const n of steps){
  if(io.stopped())break;await io.current();if(io.stopped())break;io.progress(n.id);
  const a=resolve(n.a),b=overlayAlgorithm(n.algorithm)?resolve(n.b):undefined;
  if(!a||overlayAlgorithm(n.algorithm)&&!b)throw Error("Không có đầu vào "+n.name);
  const outputName=n.name.trim()+" · "+run.runId.slice(0,8);
  let entry=run.entries.find(e=>e.step===n.id);
  if(!entry){
   const load=async(id:string,ref:ModelRef|null)=>{const layer=await io.layer(id);if(layer.id!==id)throw Error("Layer trả về không khớp.");if(ref?.type==="layer"&&(!run.sources[id]||!equalPin(sourcePin(layer),run.sources[id]!)))throw Error("Nguồn đã thay đổi kể từ lúc lập mô hình.");return layer};
   const input=await load(a,n.a),overlay=b?await load(b,n.b):undefined;
   const request=processingRequest(n.algorithm,input,overlay,String(n.parameters.distance_meters??""),String(n.parameters.quad_segs??"8"),String(n.parameters.group_by_field??""),outputName,io.operation());
   entry={step:n.id,request};run={...run,entries:[...run.entries,entry]};io.persist(run);
  }else if(entry.request.input_layer_id!==a||entry.request.overlay_layer_id!==(b??null)||entry.request.algorithm_id!==n.algorithm||entry.request.output_name!==outputName)throw Error("Yêu cầu nhật ký không khớp graph.");
  await io.current();if(io.stopped())break;
  const job=verifyProcessingReceipt(entry.jobId?await io.job(entry.jobId):await io.submit(entry.request),entry.request);
  if(entry.jobId&&entry.jobId!==job.id)throw Error("Job đã đổi ID.");
  if(!entry.jobId){run={...run,entries:run.entries.map(e=>e.step===n.id?{...e,jobId:job.id}:e)};io.persist(run)}
  await io.current();io.progress(n.id,job);
  if(job.status!=="succeeded"||!job.output_layer_id)throw Error("Job "+job.id+" · "+job.status+". Kiểm tra hoặc retry trong Processing, sau đó tiếp tục cùng nhật ký.");
  outputs.set(n.id,job.output_layer_id);finished++;
 }
 return {run,finished};
}
