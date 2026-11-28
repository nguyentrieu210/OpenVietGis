import {describe,it,expect,vi} from "vitest";
import {depthForZoom,PointCloudLod,type RemoteCloudSource} from "./point-cloud-lod";
const source:RemoteCloudSource={url:"https://data.example.org/ept.json",crs:4326,format:"EPT",maxDepth:8,initialDepth:0};
const cloud=(n:number)=>({points:[{longitude:106,latitude:10,height:n,intensity:0}],count:n,center:[106,10] as [number,number],bounds:[106,10,106,10] as [number,number,number,number]});
describe("camera LOD session",()=>{
 it("bounds zoom depth",()=>{expect(depthForZoom(1,8)).toBe(0);expect(depthForZoom(12,8)).toBe(6);expect(depthForZoom(23,8)).toBe(8)});
 it("debounces and caches, cancels stale loads and releases on dispose",async()=>{
  vi.useFakeTimers();try{
   const render=vi.fn(),status=vi.fn();let resolveOld:(c:ReturnType<typeof cloud>)=>void=()=>{};
   const load=vi.fn((_s:RemoteCloudSource,d:number,_signal:AbortSignal)=>d===4?new Promise<ReturnType<typeof cloud>>(r=>{resolveOld=r}):Promise.resolve(cloud(d)));
   const lod=new PointCloudLod(source,cloud(0),render,status,load);
   lod.update({longitude:106,latitude:10,zoom:9});await vi.advanceTimersByTimeAsync(350);
   const old=load.mock.calls[0]![2];lod.update({longitude:106,latitude:10,zoom:12});expect(old.aborted).toBe(true);
   await vi.advanceTimersByTimeAsync(350);expect(render).toHaveBeenLastCalledWith(cloud(6),6);
   resolveOld(cloud(4));await Promise.resolve();expect(render).toHaveBeenCalledTimes(1);
   lod.update({longitude:106,latitude:10,zoom:3});expect(render).toHaveBeenLastCalledWith(cloud(0),0);
   lod.update({longitude:106,latitude:10,zoom:12});expect(render).toHaveBeenLastCalledWith(cloud(6),6);expect(load).toHaveBeenCalledTimes(2);
   lod.dispose();lod.update({longitude:106,latitude:10,zoom:15});await vi.advanceTimersByTimeAsync(1000);expect(load).toHaveBeenCalledTimes(2);
  }finally{vi.useRealTimers()}
 });
});
