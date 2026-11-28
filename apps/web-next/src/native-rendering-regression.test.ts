import {it,expect} from "vitest";
import {virtualTableRange} from "./virtual-table";
import {renderBudget} from "../../../packages/spatial-workspace/src/render-budget";
import {createSnapIndex} from "./snap-spatial-index";
it("limits rendered rows even when the logical table has one million rows",()=>{
 const r=virtualTableRange(1000000,15000000,300);
 expect(r.start).toBe(499995);expect(r.end-r.start).toBeLessThanOrEqual(21);
 expect(r.top+r.bottom+(r.end-r.start)*30).toBe(30000000);
});
it("keeps 3D cache bounded and relaxes LOD while moving",()=>{
 for(const [cpu,ram] of [[2,2],[8,8],[16,16]]){const b=renderBudget(cpu,ram);expect(b.cacheBytes).toBeLessThanOrEqual(128*1048576);expect(b.movingError).toBeGreaterThan(b.idleError)}
 expect(renderBudget(2,2).cacheBytes).toBeLessThan(renderBudget(16,16).cacheBytes);
});
it("queries a cached index without scanning 100000 vertices",()=>{
 const coords=Array.from({length:100000},(_,i)=>[i,0]);
 const index=createSnapIndex({type:"LineString",coordinates:coords},p=>[p[0],p[1]],false);
 const near=index([50000,0],3);expect(near.vertices).toHaveLength(7);expect(near.segments).toHaveLength(8);
});

import {prepareSnapRecords,projectSnapRecords,queryPreparedSnapIndex,querySnapRecords} from "./snap-index-core";
it("packed index and worker cold fallback return identical candidates and order",()=>{
 const geometry={type:"LineString" as const,coordinates:Array.from({length:10000},(_,i)=>[Math.sin(i*.13)*1000,Math.cos(i*.17)*1000])};
 const records=projectSnapRecords(geometry,p=>[p[0],p[1]],false),index=prepareSnapRecords(records);
 for(let i=0;i<50;i++){const p:[number,number]=[Math.sin(i)*900,Math.cos(i)*900];expect(queryPreparedSnapIndex(index,p,25)).toEqual(querySnapRecords(records,p,25))}
});
