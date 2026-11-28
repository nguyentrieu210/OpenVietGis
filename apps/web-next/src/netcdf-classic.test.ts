import {describe,it,expect} from "vitest";
import {parseClassicNetcdf} from "./netcdf-classic";
import {parse2dm} from "./local-mesh";
const mesh=parse2dm("MESH2D\nND 3 106 10 0\nND 1 106.1 10 0\nND 2 106 10.1 0\nE3T 1 3 1 2 1",4326);
function fixture(version=1,record=false){
 const a:number[]=[];const u=(n:number)=>a.push((n>>>24)&255,(n>>>16)&255,(n>>>8)&255,n&255);
 const str=(s:string)=>{u(s.length);a.push(...Array.from(s,x=>x.charCodeAt(0)));while(a.length%4)a.push(0)};
 const attr=(k:string,v:string)=>{str(k);u(2);str(v)};
 u(0x43444600|version);u(record?2:0);u(10);u(2);str("time");u(record?0:2);str("node");u(3);u(0);u(0);
 u(11);u(3);const offsets:number[]=[];
 function variable(name:string,dims:number[],size:number,attributes:Record<string,string>){
  str(name);u(dims.length);dims.forEach(u);const entries=Object.entries(attributes);u(entries.length?12:0);u(entries.length);entries.forEach(([k,v])=>attr(k,v));u(4);u(size);if(version===2)u(0);offsets.push(a.length);u(0);
 }
 variable("node_id",[1],12,{});variable("time",[0],record?4:8,{units:"hours since 2026-01-01 00:00:00 UTC"});variable("depth",[0,1],record?12:24,{units:"m"});
 const arrays=record?[[1,2,3],[0],[1,2,3,1,4,-2147483647,6]]:[[1,2,3],[0,1],[1,2,3,4,-2147483647,6]];
 arrays.forEach((v,i)=>{const n=a.length;const at=offsets[i]!;a[at]=(n>>>24)&255;a[at+1]=(n>>>16)&255;a[at+2]=(n>>>8)&255;a[at+3]=n&255;v.forEach(u)});
 return Uint8Array.from(a).buffer;
}
describe("classic NetCDF mesh time series",()=>{
 it("reads CDF-1 and CDF-2 with node identity, UTC and default fill",()=>{for(const v of [1,2]){const x=parseClassicNetcdf(fixture(v),mesh,"depth");expect(x.frames).toEqual([[3,1,2],[6,4,null]]);expect(x.times[1]!-x.times[0]!).toBe(3600000);expect(x.unit).toBe("m")}});
 it("reads unlimited time record arrays",()=>{for(const v of [1,2]){const x=parseClassicNetcdf(fixture(v,true),mesh,"depth");expect(x.frames).toEqual([[3,1,2],[6,4,null]]);expect(()=>parseClassicNetcdf(fixture(v,true).slice(0,-1),mesh,"depth")).toThrow()}});
 it("rejects truncated data, HDF5 and unknown variables",()=>{expect(()=>parseClassicNetcdf(fixture().slice(0,-1),mesh,"depth")).toThrow();expect(()=>parseClassicNetcdf(fixture(),mesh,"missing")).toThrow();const b=fixture();new Uint8Array(b)[3]=5;expect(()=>parseClassicNetcdf(b,mesh,"depth")).toThrow()});
});
