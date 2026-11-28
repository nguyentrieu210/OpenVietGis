import {renderDiff} from "../../../apps/web-next/src/SpatialSyncCore.ts";
import {performance} from "node:perf_hooks";

const geometry={type:"Point",coordinates:[106.7,10.8]};
const geometrySnapshot=JSON.stringify(geometry);
function percentile(values,q){const a=[...values].sort((x,y)=>x-y);return a[Math.min(a.length-1,Math.ceil(q*a.length)-1)]}
const results=[];
for(const size of [10_000,100_000,1_000_000]){
 const before=Array.from({length:size},(_,i)=>({id:String(i),geometry,properties:{key:String(i),selected:false}}));
 const after=before.slice();
 after[100]={...before[100],properties:{...before[100].properties,selected:true}};
 const samples=[],runs=size===1_000_000?5:12;
 for(let i=0;i<runs;i++){
  globalThis.gc?.();
  const start=performance.now(),diff=renderDiff(before,after),elapsed=performance.now()-start;
  if(diff.add.length||diff.remove.length||diff.update.length!==1||diff.update[0].id!=="100")throw Error("incremental diff contract failed");
  if(JSON.stringify(geometry)!==geometrySnapshot||before.some((row,index)=>index!==100&&row.geometry!==geometry))throw Error("source geometry mutated");
  samples.push(elapsed);
 }
 results.push({features:size,samples_ms:samples,median_ms:percentile(samples,.5),p95_ms:percentile(samples,.95),
  update_count:1,source_geometry_unchanged:true});
}
console.log(JSON.stringify({node:process.version,method:"Candidate renderDiff; unchanged rows retain object identity, one selection changes; explicit GC before each sample; shared immutable point geometry; 12 samples at 10k/100k and 5 at 1m.",results},null,2));
