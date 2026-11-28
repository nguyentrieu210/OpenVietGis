import {describe,it,expect} from "vitest";
import {readFileSync} from "node:fs";
import {createLazPerf} from "laz-perf";
import {decodeLazWith,decodeLazChunkWith,lazHeader,type LazModule} from "./laz-decoder";
import {readRemoteCopc} from "./remote-copc";
const fixture=(name:string)=>{const b=readFileSync(new URL("./fixtures/point-cloud/"+name,import.meta.url));return new Uint8Array(b).buffer};
const module=async()=>await (createLazPerf as unknown as ()=>Promise<LazModule>)();
describe("real LAZ and COPC fixtures",()=>{
 it("decodes lossless LAZ 1.4 and COPC using the locked native WASM codec",async()=>{
  const mod=await module();
  for(const name of ["ellipsoid-1.4.laz","ellipsoid.copc.laz"]){
   const bytes=fixture(name),header=lazHeader(bytes),cloud=decodeLazWith(bytes,3857,mod);
   expect(cloud.count).toBe(header.count);expect(cloud.count).toBeGreaterThan(0);
   expect(cloud.points.length).toBeGreaterThan(0);expect(cloud.points.every(p=>Number.isFinite(p.longitude)&&Number.isFinite(p.latitude)&&Number.isFinite(p.height))).toBe(true);
  }
 },30000);
 it("decodes only root chunk and then entire real COPC through exact ranges",async()=>{
  const bytes=fixture("ellipsoid.copc.laz"),mod=await module(),ranges:string[]=[];
  const fetcher:typeof fetch=async(_,init)=>{const range=new Headers(init?.headers).get("Range")!;ranges.push(range);const [a,b]=range.replace("bytes=","").split("-").map(Number);return new Response(bytes.slice(a,b!+1),{status:206,headers:{"Content-Range":"bytes "+a+"-"+b+"/"+bytes.byteLength,ETag:'"synthetic-fixture"'}})};
  const decode=async(b:ArrayBuffer,count:number,format:number,length:number)=>decodeLazChunkWith(b,count,format,length,mod);
  const root=await readRemoteCopc("https://data.example.org/ellipsoid.copc.laz",3857,0,new AbortController().signal,fetcher,decode);
  expect(root.datasetCount).toBe(100000);expect(root.count).toBe(66272);expect(root.complete).toBe(false);expect(root.nodeCount).toBe(1);
  expect(ranges).toContain("bytes=1432-359919");expect(ranges).not.toContain("bytes=359920-459452");
  const full=await readRemoteCopc("https://data.example.org/ellipsoid.copc.laz",3857,1,new AbortController().signal,fetcher,decode);
  expect(full.count).toBe(100000);expect(full.complete).toBe(true);expect(full.nodeCount).toBe(5);
  const local=decodeLazWith(bytes,3857,mod);
  expect(full.bounds).toEqual(local.bounds);
 },30000);
});
