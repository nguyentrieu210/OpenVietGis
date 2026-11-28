import type {ProcessingJob} from "@openvietgis/client-sdk";
import {readPendingProcessing,verifyProcessingReceipt,type ProcessingRequest} from "./processing-request";
export const MAX_BATCH=20;
export type BatchRow={request:ProcessingRequest;state:"draft"|"uncertain"|"received"|"rejected";jobId?:string;status?:ProcessingJob["status"];error?:string};
export function readBatch(raw:string|null):BatchRow[]{
 if(raw===null)return [];
 const x=JSON.parse(raw);
 if(!Array.isArray(x)||x.length>MAX_BATCH)throw Error("Nhật ký batch không hợp lệ.");
 const ids=new Set<string>();
 return x.map(r=>{
  const request=readPendingProcessing(JSON.stringify(r?.request));
  if(!request||ids.has(request.operation_id)||!["draft","uncertain","received","rejected"].includes(r.state))throw Error("Nhật ký batch không hợp lệ.");
  ids.add(request.operation_id);
  if(r.state==="received"&&(typeof r.jobId!=="string"||!r.jobId||!["queued","running","succeeded","failed","cancelled"].includes(r.status)))throw Error("Biên nhận batch không hợp lệ.");
  return {request,state:r.state,...(r.state==="received"?{jobId:r.jobId,status:r.status}:{}),...(typeof r.error==="string"?{error:r.error.slice(0,500)}:{})};
 });
}
export function appendBatch(rows:BatchRow[],request:ProcessingRequest):BatchRow[]{
 if(rows.length>=MAX_BATCH)throw Error("Tối đa "+MAX_BATCH+" tác vụ mỗi batch.");
 if(rows.some(r=>r.request.operation_id===request.operation_id||r.request.output_name===request.output_name))throw Error("Tên đầu ra hoặc mã thao tác bị trùng trong batch.");
 return [...rows,{request,state:"draft"}];
}
export function receivedBatch(row:BatchRow,job:ProcessingJob):BatchRow{
 verifyProcessingReceipt(job,row.request);
 return {request:row.request,state:"received",jobId:job.id,status:job.status};
}
export function nextBatchRow(rows:BatchRow[]):number{
 const uncertain=rows.findIndex(r=>r.state==="uncertain");return uncertain>=0?uncertain:rows.findIndex(r=>r.state==="draft");
}
