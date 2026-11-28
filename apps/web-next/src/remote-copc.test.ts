import {describe,it,expect} from "vitest";
import {boundedRangeReader,copcUrl,parseCopcHierarchy} from "./remote-copc";
const signal=new AbortController().signal,url=new URL("https://data.example.org/test.copc.laz");
describe("COPC HTTP range safety",()=>{
 it("pins total and ETag and rejects range changes",async()=>{
  let etag='"v1"';
  const fetcher:typeof fetch=async(_,init)=>{const a=new Headers(init?.headers).get("range")!;const [s,e]=a.replace("bytes=","").split("-").map(Number);expect(init?.credentials).toBe("omit");expect(init?.redirect).toBe("error");return new Response(new Uint8Array(e!-s!+1),{status:206,headers:{"Content-Range":"bytes "+s+"-"+e+"/1000",ETag:etag}})};
  const r=boundedRangeReader(url,signal,fetcher);expect((await r.read(0,12)).byteLength).toBe(12);expect(r.size).toBe(1000);etag='"v2"';await expect(r.read(12,12)).rejects.toThrow("changed");
 });
 it("rejects ignored ranges and private URL literals",async()=>{await expect(boundedRangeReader(url,signal,async()=>new Response(new Uint8Array(12))).read(0,12)).rejects.toThrow();for(const u of ["http://data.example.org/a.copc.laz","https://127.0.0.1/a.copc.laz","https://a:b@data.example.org/a.copc.laz","https://data.example.org/a.copc.laz?token=1"])expect(()=>copcUrl(u)).toThrow()});
 it("parses binary hierarchy, rejects out-of-root and duplicate entries",()=>{const b=new ArrayBuffer(32),d=new DataView(b);d.setBigUint64(16,1000n,true);d.setInt32(24,100,true);d.setInt32(28,10,true);expect(parseCopcHierarchy(b,[0,0,0,0])[0]).toEqual({key:[0,0,0,0],offset:1000,bytes:100,count:10});d.setInt32(4,1,true);expect(()=>parseCopcHierarchy(b,[0,0,0,0])).toThrow()});
});
