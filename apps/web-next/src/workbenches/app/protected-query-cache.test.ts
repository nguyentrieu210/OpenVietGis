import { QueryClient, QueryObserver, onlineManager } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { currentProtectedData, protectedQueryOptions } from "./protected-query";

function pending<T>() {
  let resolve!: (value:T) => void;
  let reject!: (error:unknown) => void;
  const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});
  return {promise,resolve,reject};
}

describe("actual query-cache authorization transitions",()=>{
  const key=["map-workspace","actor-a","org-a","project-a","definition"];
  const privateValue={id:"authorized-before"};

  it("does not expose a warm successful payload while a real mount refetch is unsettled or denied",async()=>{
    const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
    const response=pending<typeof privateValue>();
    client.setQueryData(key,privateValue);
    const observer=new QueryObserver(client,{queryKey:key,queryFn:()=>response.promise,...protectedQueryOptions});
    const unsubscribe=observer.subscribe(()=>{});
    try {
      await vi.waitFor(()=>expect(observer.getCurrentResult().fetchStatus).toBe("fetching"));
      expect(observer.getCurrentResult().data).toEqual(privateValue);
      expect(currentProtectedData(observer.getCurrentResult())).toBeUndefined();

      response.reject(Object.assign(new Error("revoked"),{status:403}));
      await vi.waitFor(()=>expect(observer.getCurrentResult().isError).toBe(true));
      // TanStack can keep the previous successful cache payload even after
      // revocation; the product gate must not identify it as current data.
      expect(observer.getCurrentResult().data).toEqual(privateValue);
      expect(currentProtectedData(observer.getCurrentResult())).toBeUndefined();
    } finally {unsubscribe();client.clear();}
  });

  it("does not expose warm data during a paused refetch and only restores a new settled payload",async()=>{
    const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
    const previousOnline=onlineManager.isOnline();
    const response=pending<typeof privateValue>();
    client.setQueryData(key,privateValue);
    onlineManager.setOnline(false);
    client.mount();
    const observer=new QueryObserver(client,{queryKey:key,queryFn:()=>response.promise,...protectedQueryOptions});
    const unsubscribe=observer.subscribe(()=>{});
    try {
      await vi.waitFor(()=>expect(observer.getCurrentResult().fetchStatus).toBe("paused"));
      expect(observer.getCurrentResult().data).toEqual(privateValue);
      expect(currentProtectedData(observer.getCurrentResult())).toBeUndefined();

      onlineManager.setOnline(true);
      await vi.waitFor(()=>expect(observer.getCurrentResult().fetchStatus).toBe("fetching"));
      expect(currentProtectedData(observer.getCurrentResult())).toBeUndefined();
      response.resolve({id:"authorized-after"});
      await vi.waitFor(()=>expect(currentProtectedData(observer.getCurrentResult())).toEqual({id:"authorized-after"}));
    } finally {
      unsubscribe();client.unmount();client.clear();onlineManager.setOnline(previousOnline);
    }
  });
});
