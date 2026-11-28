import {describe,it,expect} from "vitest";
import {loadRemoteEpt,remoteEptUrl} from "./local-ept";
const url="https://data.example.org/cloud/ept.json";
function fixture(){
 const meta={version:"1.1.0",dataType:"binary",hierarchyType:"json",points:2,bounds:[106,10,0,107,11,100],srs:{authority:"EPSG",horizontal:"4326"},schema:["X","Y","Z"].map((name,i)=>({name,type:"signed",size:4,scale:.01,offset:[106,10,0][i]}))};
 const record=(x:number,y:number,z:number)=>{const b=new ArrayBuffer(12),d=new DataView(b);d.setInt32(0,x,true);d.setInt32(4,y,true);d.setInt32(8,z,true);return b};
 const files=new Map<string,BodyInit>([["ept.json",JSON.stringify(meta)],["ept-hierarchy/0-0-0-0.json",JSON.stringify({"0-0-0-0":1,"1-1-1-1":-1})],["ept-hierarchy/1-1-1-1.json",JSON.stringify({"1-1-1-1":1})],["ept-data/0-0-0-0.bin",record(10,10,100)],["ept-data/1-1-1-1.bin",record(90,90,6000)]]);
 const requests:string[]=[];
 const fetcher:typeof fetch=async(input,init)=>{const u=new URL(String(input)),path=u.pathname.replace("/cloud/","");requests.push(path);expect(u.origin).toBe("https://data.example.org");expect(init?.credentials).toBe("omit");expect(init?.redirect).toBe("error");return new Response(files.get(path)??null,{status:files.has(path)?200:404})};
 return {requests,files,fetcher};
}
describe("remote EPT bounded LOD",()=>{
 it("only requests root at depth zero",async()=>{const f=fixture();const c=await loadRemoteEpt(url,4326,0,new AbortController().signal,f.fetcher);expect(c.count).toBe(1);expect(c.datasetCount).toBe(2);expect(c.complete).toBe(false);expect(f.requests).toEqual(["ept.json","ept-hierarchy/0-0-0-0.json","ept-data/0-0-0-0.bin"])});
 it("requests child page and tile at depth one",async()=>{const f=fixture();const c=await loadRemoteEpt(url,4326,1,new AbortController().signal,f.fetcher);expect(c.count).toBe(2);expect(c.complete).toBe(true);expect(c.points[1]?.height).toBe(60)});
 it("rejects credentials, private literals, redirects and oversized nodes",async()=>{for(const u of ["http://data.example.org/ept.json","https://a:b@data.example.org/ept.json","https://127.0.0.1/ept.json","https://data.example.org/ept.json?token=x"])expect(()=>remoteEptUrl(u)).toThrow();const f=fixture();f.files.set("ept-data/0-0-0-0.bin",new Uint8Array(13));await expect(loadRemoteEpt(url,4326,0,new AbortController().signal,f.fetcher)).rejects.toThrow()});
});
