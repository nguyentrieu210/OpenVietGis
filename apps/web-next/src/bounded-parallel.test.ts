import {describe,it,expect} from "vitest";
import {boundedParallel} from "./bounded-parallel";
describe("bounded parallel downloads",()=>{
 it("limits concurrent jobs and preserves order",async()=>{
  let active=0,peak=0;const ctl=new AbortController();
  const result=await boundedParallel([1,2,3,4,5,6,7],3,async n=>{active++;peak=Math.max(peak,active);await new Promise(r=>setTimeout(r,8-n));active--;return n*2},ctl.signal);
  expect(peak).toBe(3);expect(result).toEqual([2,4,6,8,10,12,14]);expect(active).toBe(0);
 });
 it("joins already started work and stops starting more on error",async()=>{
  let active=0,started=0;
  await expect(boundedParallel([1,2,3,4,5,6],2,async n=>{started++;active++;try{if(n===1)throw Error("bad tile");await new Promise(r=>setTimeout(r,5));return n}finally{active--}},new AbortController().signal)).rejects.toThrow("bad tile");
  expect(started).toBeLessThanOrEqual(2);expect(active).toBe(0);
 });
});
