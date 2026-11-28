import {describe,it,expect} from "vitest";
import {SpatialPerformance} from "./SpatialPerformance";

describe("SpatialPerformance aggregate telemetry",()=>{
 it("keeps bounded ID-free cache counters and percentile summaries",()=>{
  let now=0;const p=new SpatialPerformance(()=>now);
  for(let i=0;i<65;i++){
   const m=p.start("open");m.counters.cacheHits=2;m.counters.cacheMisses=1;m.counters.cacheBytes=100;m.counters.invalidations=i%2;now+=i+1;m.finish(true);
  }
  expect(p.snapshot()).toHaveLength(60);
  const s=p.summary("open");
  expect(s.count).toBe(60);expect(s.cacheHitRate).toBeCloseTo(2/3);
  expect(s.cacheBytes).toBe(6000);expect(s.p95).not.toBeNull();expect(s.p99).toBe(65);
  expect(JSON.stringify(p.snapshot())).not.toContain("project");
 });
});
