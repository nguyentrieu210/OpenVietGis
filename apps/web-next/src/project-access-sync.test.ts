import {describe,it,expect,vi,beforeEach,afterEach} from "vitest";
import {publishProjectAccessInvalidation,subscribeProjectAccessInvalidation,validProjectAccessSignal} from "./project-access-sync";

class FakeChannel{
 static peers=new Set<FakeChannel>();
 onmessage:((event:{data:unknown})=>void)|null=null;
 constructor(public name:string){FakeChannel.peers.add(this)}
 postMessage(data:unknown){for(const peer of FakeChannel.peers)if(peer!==this&&peer.name===this.name)peer.onmessage?.({data})}
 close(){FakeChannel.peers.delete(this)}
}
const scope={account:"actor",org:"org",project:"project"};

beforeEach(()=>{
 FakeChannel.peers.clear();
 vi.stubGlobal("BroadcastChannel",FakeChannel);
 vi.stubGlobal("crypto",{randomUUID:()=>"12345678-1234-1234-1234-123456789abc"});
 vi.stubGlobal("localStorage",{setItem:vi.fn(),removeItem:vi.fn()});
 vi.stubGlobal("window",{addEventListener:vi.fn(),removeEventListener:vi.fn()});
});
afterEach(()=>vi.unstubAllGlobals());

describe("project access cross-tab coordination",()=>{
 it("accepts only strict scope signals",()=>{
  expect(validProjectAccessSignal({version:1,scope,nonce:"12345678"})).toBe(true);
  expect(validProjectAccessSignal({version:1,scope:{...scope,project:""},nonce:"12345678"})).toBe(false);
  expect(validProjectAccessSignal({version:1,scope,nonce:"x"})).toBe(false);
  expect(validProjectAccessSignal({version:2,scope,nonce:"12345678"})).toBe(false);
  expect(validProjectAccessSignal({version:1,scope:{...scope,org:"x".repeat(129)},nonce:"12345678"})).toBe(false);
 });
 it("publishes to another channel but not back into the sender",()=>{
  const received:unknown[]=[];const peer=new FakeChannel("ovg-project-access-v1");peer.onmessage=e=>received.push(e.data);
  publishProjectAccessInvalidation(scope);
  expect(received).toHaveLength(1);
  expect(validProjectAccessSignal(received[0])).toBe(true);
  expect((received[0] as {scope:typeof scope}).scope).toEqual(scope);
  expect(localStorage.setItem).not.toHaveBeenCalled();
 });
 it("subscription closes its channel",()=>{
  const handler=vi.fn();const close=subscribeProjectAccessInvalidation(handler);
  expect(FakeChannel.peers.size).toBe(1);close();expect(FakeChannel.peers.size).toBe(0);
 });
});
