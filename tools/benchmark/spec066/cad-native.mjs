import {performance} from "node:perf_hooks";
import {visibleDxfShapes} from "../../../apps/web-next/src/native-dxf.ts";
import {findScreenSnap} from "../../../packages/spatial-workspace/src/snap-math.ts";

function percentile(values,q){const a=[...values].sort((x,y)=>x-y);return a[Math.min(a.length-1,Math.ceil(q*a.length)-1)]}
function measure(fn,samples){const times=[];let result;for(let i=0;i<samples+2;i++){global.gc?.();const t=performance.now();result=fn();const d=performance.now()-t;if(i>=2)times.push(d)}return {samples_ms:times,median_ms:percentile(times,.5),p95_ms:percentile(times,.95),result}}
const culling=[];
for(const size of [10_000,100_000,1_000_000]){
 const shapes=Array.from({length:size},(_,i)=>({id:i,kind:"LINE",layer:"0",points:[[i,0],[i+.5,.5]],fields:[],editable:false}));
 const m=measure(()=>visibleDxfShapes(shapes,{x:500,y:-1,w:100,h:2}),size===1_000_000?5:10);
 if(m.result.length!==101||m.result[0].id!==500||m.result.at(-1).id!==600)throw Error("culling parity failure");
 culling.push({shapes:size,visible:101,median_ms:m.median_ms,p95_ms:m.p95_ms,samples_ms:m.samples_ms});
}
const segments=Array.from({length:100_000},(_,i)=>({a:[i*2,0],b:[i*2+1,1]}));
const snap=measure(()=>findScreenSnap([50000.5,.25],4,"smart",[],segments),10);
if(!snap.result||snap.result.distance>4)throw Error("snap result missing");
console.log(JSON.stringify({node:process.version,method:"Synthetic immutable CAD lines; explicit GC when available; culling exact-result assertion at 10k/100k/1m; 100k-segment smart screen-snap query. CPU lab benchmark only, not browser frame timing.",culling,snap:{segments:100000,kind:snap.result.kind,point:snap.result.point,median_ms:snap.median_ms,p95_ms:snap.p95_ms,samples_ms:snap.samples_ms}},null,2));
