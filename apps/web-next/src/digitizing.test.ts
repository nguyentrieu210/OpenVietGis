import {describe,it,expect} from "vitest";
import {parseXY,parseSketchPoints,draftError,orthoPoint,snapLayerAllowed,cleanSnapTolerances,snapToleranceFor} from "./digitizing";
describe("digitizing safeguards",()=>{
 it("strict coordinates do not interpret missing fields as zero",()=>{
  expect(parseXY("106.5, 10.5")).toEqual([106.5,10.5]);expect(parseXY("0 0")).toEqual([0,0]);expect(parseSketchPoints("1,2 3,4")).toEqual([[1,2],[3,4]]);expect(parseSketchPoints("1,,2 3,4")).toBeNull();
  for(const s of ["1,","1,,2","1e2,4","0,90","181,0"])expect(parseXY(s)).toBeNull();
 });
 it("rejects degenerate drafts without changing coordinates",()=>{
  expect(draftError({type:"LineString",coordinates:[[1,2],[1,2]]})).toBeTruthy();
  expect(draftError({type:"Polygon",coordinates:[[[1,1],[2,2],[3,3],[1,1]]]})).toBeTruthy();
  expect(draftError({type:"Polygon",coordinates:[[[1,1],[2,1],[2,2],[1,1]]]})).toBeNull();
  expect(draftError({type:"Polygon",coordinates:[[[0,0],[3,3],[3,0],[0,2],[0,0]]]})).toContain("tự giao");
 });
 it("explicit snap scopes and ortho",()=>{
  expect(snapLayerAllowed("active","a","b",[])).toBe(false);
  expect(snapLayerAllowed("selected","a","b",["a"])).toBe(true);
  expect(snapLayerAllowed("selected","a","a",[])).toBe(false);
  expect(orthoPoint([2,1.2],[1,1])).toEqual([2,1]);
 });
});

import {lockedSegment,sphericalInverse,hasSegmentLock} from "./digitizing";
describe("spherical digitizing constraints",()=>{
 it("locks distance and north-based bearing",()=>{
  const a:[number,number]=[106.5,10.5],b=lockedSegment(a,a,{distance:"100",bearing:"90"}),r=sphericalInverse(a,b);
  expect(r.distance).toBeCloseTo(100,5);expect(r.bearing).toBeCloseTo(90,5);
  expect(b[0]).toBeGreaterThan(a[0]);
  const c=lockedSegment(a,[106.5,10.6],{distance:"10",bearing:""});
  expect(sphericalInverse(a,c).distance).toBeCloseTo(10,5);
 });
 it("rejects invalid constraints, never silently clips",()=>{
  const a:[number,number]=[106,10];
  for(const lock of [{distance:"0",bearing:"90"},{distance:"10",bearing:"360"},{distance:"x",bearing:"0"},{distance:"10",bearing:""}])expect(()=>lockedSegment(a,a,lock)).toThrow();
  expect(hasSegmentLock({distance:" ",bearing:""})).toBe(false);
 });
});

import {findScreenSnap} from "./SnapMath";
describe("layer-specific screen snap tolerances",()=>{
 it("keeps the global default and rejects corrupt or inherited overrides",()=>{
  const clean=cleanSnapTolerances(JSON.parse('{"a":4,"b":25,"bad":26,"string":"8","__proto__":12,"constructor":10}'));
  expect(clean).toEqual({a:4,b:25});
  expect(snapToleranceFor(10,clean,"a")).toBe(4);
  expect(snapToleranceFor(10,clean,"missing")).toBe(10);
  expect(snapToleranceFor(10,Object.create({a:25}),"a")).toBe(10);
 });
 it("checks vertex and edge distances against their own layer's limit",()=>{
  expect(findScreenSnap([0,0],25,"vertex",[[5,0],[12,0]],[],undefined,[4,15])?.point).toEqual([12,0]);
  expect(findScreenSnap([0,0],25,"edge",[],[{a:[-10,5],b:[10,5],radius:4},{a:[-10,12],b:[10,12],radius:15}])?.point).toEqual([0,12]);
  expect(findScreenSnap([0,0],25,"vertex",[[0,0]],[],undefined,[NaN])).toBeNull();
 });
 it("requires a crossing to be inside both segment tolerances",()=>{
  const a={a:[-20,6] as [number,number],b:[20,6] as [number,number],radius:4};
  const b={a:[0,-20] as [number,number],b:[0,20] as [number,number],radius:15};
  expect(findScreenSnap([0,0],25,"intersection",[],[a,b])).toBeNull();
  expect(findScreenSnap([0,0],25,"intersection",[],[{...a,radius:8},b])?.point).toEqual([0,6]);
 });
});
