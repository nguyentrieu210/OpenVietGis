import {describe,it,expect,vi,afterEach} from "vitest";

class FakeWorker{
 static instances:FakeWorker[]=[];
 onmessage:((event:{data:{id:number;value:unknown}})=>void)|null=null;
 onerror:((event:Event)=>void)|null=null;
 onmessageerror:((event:MessageEvent)=>void)|null=null;
 messages:unknown[]=[];terminated=0;
 constructor(){FakeWorker.instances.push(this)}
 postMessage(value:unknown){this.messages.push(value)}
 terminate(){this.terminated++}
}

afterEach(()=>{vi.useRealTimers();vi.unstubAllGlobals();vi.resetModules();FakeWorker.instances=[]});

describe("spatial cache worker RPC bounds",()=>{
 it("bounds outstanding work and kills timed-out queued work before restarting",async()=>{
  vi.useFakeTimers();
  vi.stubGlobal("Worker",FakeWorker);
  vi.stubGlobal("window",{setTimeout,clearTimeout});
  const {readCache}=await import("./SpatialCache");
  const scope={account:"a",org:"o",project:"p"};
  const pending=Array.from({length:24},(_,i)=>readCache(scope,"layer-"+i));
  const overflow=readCache(scope,"overflow");
  await expect(overflow).resolves.toBeNull();
  expect(FakeWorker.instances).toHaveLength(1);
  expect(FakeWorker.instances[0]!.messages).toHaveLength(24);
  await vi.advanceTimersByTimeAsync(15000);
  await expect(Promise.all(pending)).resolves.toEqual(Array(24).fill(null));
  expect(FakeWorker.instances[0]!.terminated).toBe(1);
  const restarted=readCache(scope,"next");
  expect(FakeWorker.instances).toHaveLength(2);
  await vi.advanceTimersByTimeAsync(15000);
  await expect(restarted).resolves.toBeNull();
  expect(FakeWorker.instances[1]!.terminated).toBe(1);
 });
});
