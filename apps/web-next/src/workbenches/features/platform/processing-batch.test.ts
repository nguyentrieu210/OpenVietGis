import {describe,it,expect} from "vitest";
import type {ProcessingJob} from "@openvietgis/client-sdk";
import {appendBatch,readBatch,receivedBatch,nextBatchRow,type BatchRow} from "./processing-batch";
const request=(i:number)=>({algorithm_id:"buffer" as const,input_layer_id:"layer",overlay_layer_id:null,parameters:{distance_meters:10,quad_segs:8},output_name:"Output "+i,operation_id:"12345678-1234-1234-1234-"+String(i).padStart(12,"0")});
describe("bounded batch journal",()=>{
 it("keeps rows separate with stable operation IDs and rejects duplicate outputs",()=>{
  const rows=appendBatch(appendBatch([],request(1)),request(2));
  expect(readBatch(JSON.stringify(rows))).toEqual(rows);
  expect(()=>appendBatch(rows,request(1))).toThrow();
  expect(()=>appendBatch(Array.from({length:20},(_,i)=>({request:request(i),state:"draft" as const})),request(21))).toThrow();
 });
 it("resumes uncertain row first and never silently reruns a received row",()=>{
  const r:BatchRow={request:request(1),state:"uncertain"};
  const other:BatchRow={request:request(2),state:"draft"};
  expect(nextBatchRow([other,r])).toBe(1);
  const job={id:"job",operation_id:r.request.operation_id,algorithm_id:"buffer",output_name:r.request.output_name,status:"queued"} as ProcessingJob;
  const received=receivedBatch(r,job);
  expect(readBatch(JSON.stringify([received]))).toEqual([received]);
  expect(nextBatchRow([received])).toBe(-1);
  expect(()=>receivedBatch(r,{...job,operation_id:"wrong"})).toThrow();
  expect(()=>readBatch(JSON.stringify([{...r,state:"received"}]))).toThrow();
 });
});
