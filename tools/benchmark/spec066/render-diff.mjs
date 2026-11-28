// Usage: node --expose-gc tools/benchmark/spec066/render-diff.mjs BASE_FILE CANDIDATE_FILE
import {pathToFileURL} from "node:url";
import {resolve} from "node:path";
import {deepStrictEqual} from "node:assert";
const [base,candidate]=await Promise.all(process.argv.slice(2).map(p=>import(pathToFileURL(resolve(p)).href)));
function measure(fn,before,after){
 const times=[];
 for(let i=0;i<22;i++){
  if(i%5===0)global.gc?.();
  const start=performance.now();const diff=fn(before,after);const duration=performance.now()-start;
  if(!diff||diff.update.length!==1||diff.add.length||diff.remove.length)throw Error("Incorrect diff");
  if(i>=2)times.push(duration);
 }
 times.sort((a,b)=>a-b);
 return {p50_ms:times[9],p95_ms:times[18],samples:times.length};
}
const rows=[];
for(const size of [30000,100000]){
 const before=Array.from({length:size},(_,i)=>({id:String(i),geometry:{type:"Point",coordinates:[105+i/1000000,21]},properties:{key:String(i),layerId:"points",color:"#aabbcc",selected:false,opacity:0.4,selectable:true,strokeWidth:2,width:2,radius:5}}));
 const after=before.map((r,i)=>({...r,properties:{...r.properties,selected:i===100}}));
 deepStrictEqual(base.renderDiff(before,after),candidate.renderDiff(before,after));
 // Alternate which implementation is measured first to reduce order bias.
 const results=size===30000?{baseline:measure(base.renderDiff,before,after),candidate:measure(candidate.renderDiff,before,after)}:(()=>{const c=measure(candidate.renderDiff,before,after);return {baseline:measure(base.renderDiff,before,after),candidate:c}})();
 rows.push({features:size,...results});
}
console.log(JSON.stringify({node:process.version,method:"20 timed iterations per implementation; regenerated scalar property objects, shared geometries, one selected point, isolated synthetic CPU benchmark; not MapLibre frame timing or network latency",rows},null,2));
