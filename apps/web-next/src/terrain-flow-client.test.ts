import {describe,it,expect,vi} from "vitest";
import {bindTerrainFlowWorker} from "./terrain-flow-client";

class FakeWorker{
  onmessage:((event:MessageEvent<unknown>)=>void)|null=null;
  onerror:(()=>void)|null=null;
  terminated=0;
  terminate(){this.terminated++}
  emit(data:unknown){this.onmessage?.({data} as MessageEvent<unknown>)}
}
const asWorker=(worker:FakeWorker)=>worker as unknown as Worker;

describe("terrain flow worker generations",()=>{
  it("ignores a stale worker result after a replacement",()=>{
    let current:Worker|null=null;
    const success=vi.fn(),failure=vi.fn(),busy=vi.fn();
    const clear=(worker:Worker)=>{if(current===worker)current=null};

    const old=new FakeWorker();current=asWorker(old);
    bindTerrainFlowWorker(asWorker(old),{current:()=>current,clear,busy,success,failure});
    const next=new FakeWorker();current=asWorker(next);
    bindTerrainFlowWorker(asWorker(next),{current:()=>current,clear,busy,success,failure});

    old.emit({frames:[1]});
    expect(success).not.toHaveBeenCalled();
    expect(failure).not.toHaveBeenCalled();
    expect(old.terminated).toBe(0);

    next.emit({frames:[2]});
    expect(success).toHaveBeenCalledOnce();
    expect(success).toHaveBeenCalledWith([2]);
    expect(next.terminated).toBe(1);
    expect(current).toBeNull();
    expect(busy).toHaveBeenLastCalledWith(false);
  });

  it("cancels only the current generation and rejects late callbacks",()=>{
    let current:Worker|null=null;
    const success=vi.fn(),failure=vi.fn(),busy=vi.fn();
    const worker=new FakeWorker();current=asWorker(worker);
    const job=bindTerrainFlowWorker(asWorker(worker),{
      current:()=>current,
      clear:w=>{if(current===w)current=null},
      busy,success,failure,
    });
    expect(job.cancel()).toBe(true);
    expect(worker.terminated).toBe(1);
    worker.emit({frames:[9]});
    worker.onerror?.();
    expect(success).not.toHaveBeenCalled();
    expect(failure).not.toHaveBeenCalled();
    expect(job.cancel()).toBe(false);
  });
});
