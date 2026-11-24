import {test} from "node:test";
import assert from "node:assert/strict";
import {SpatialPerformance} from "./src/SpatialPerformance.ts";
test("bounded anonymous timings and percentiles",()=>{
 let now=0;const p=new SpatialPerformance(()=>now);
 for(let i=1;i<=65;i++){const m=p.start("open");m.counters.pages=i===65?0:1;m.counters.cacheHits=i===65?3:0;now+=i;m.finish();m.finish(false)}
 assert.equal(p.snapshot().length,60);
 assert.equal(p.summary("open").p50,35);
 assert.equal(p.summary("open").p95,62);
 assert.equal(p.summary("open").last.cacheHits,3);
 const m=p.start("sync");now+=50;m.finish(false);
 assert.equal(p.summary("sync").p95,null);
 assert.equal(p.summary("sync").failures,1);
});
