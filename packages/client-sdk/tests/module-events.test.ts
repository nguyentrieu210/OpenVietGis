import {describe,it,expect,vi} from "vitest";
import {ApiClient} from "../src/http";
import {subscribeModuleChanges,notifyModuleChange} from "../src/module-events";
import {replaceModulePolicy,replaceModuleBindings} from "../src/module-runtime";
const mock=(status=200,body:unknown={permissions:[],revision:1})=>new ApiClient({baseUrl:"https://example.test/api/v1",fetchImpl:(async()=>new Response(JSON.stringify(body),{status,headers:{"content-type":"application/json"}})) as typeof fetch});
const notice={organizationId:"o",projectId:"p",extensionKey:"mod",kind:"permissions" as const};
describe("module invalidation notices",()=>{
 it("isolates the client, organization, project and module and unsubscribes",()=>{
  const c=mock(),other=mock(),a=vi.fn(),b=vi.fn();const stop=subscribeModuleChanges(c,"o","p","mod",a);
  subscribeModuleChanges(other,"o","p","mod",b);subscribeModuleChanges(c,"wrong","p","mod",b);subscribeModuleChanges(c,"o","wrong","mod",b);subscribeModuleChanges(c,"o","p","wrong",b);
  notifyModuleChange(c,notice);expect(a).toHaveBeenCalledOnce();expect(b).not.toHaveBeenCalled();stop();notifyModuleChange(c,notice);expect(a).toHaveBeenCalledOnce();
 });
 it("emits after successful write only, with no private data",async()=>{
  const c=mock(),d=mock(409),fn=vi.fn();subscribeModuleChanges(c,"o","p","mod",fn);subscribeModuleChanges(d,"o","p","mod",fn);c.setCsrfToken("csrf");d.setCsrfToken("csrf");
  await expect(replaceModulePolicy(d,"o","p","mod","human","id",{permissions:[],revision:0})).rejects.toThrow();expect(fn).not.toHaveBeenCalled();
  await replaceModulePolicy(c,"o","p","mod","human","id",{permissions:[],revision:0});expect(fn).toHaveBeenCalledExactlyOnceWith(notice);
  await replaceModuleBindings(c,"o","p","mod",0,[]);expect(fn).toHaveBeenLastCalledWith({...notice,kind:"bindings"});
 });
 it("isolates listener exceptions",()=>{const c=mock(),fn=vi.fn();const error=vi.spyOn(console,"error").mockImplementation(()=>{});try{subscribeModuleChanges(c,"o","p","mod",()=>{throw Error("fixture")});subscribeModuleChanges(c,"o","p","mod",fn);expect(()=>notifyModuleChange(c,notice)).not.toThrow();expect(fn).toHaveBeenCalledOnce();}finally{error.mockRestore()}});
});

import {consumeModuleChanges,replayModuleChanges} from "../src/module-events";
describe("durable event pages",()=>{
 it("coalesces hints and refreshes activation before notifying",async()=>{
  const c=mock(),order:string[]=[];subscribeModuleChanges(c,"o","p","mod",e=>order.push(e.kind));
  const page={mode:"delta" as const,cursor:"cursor-3",has_more:false,items:[
   {sequence:"1",extension_key:"mod",kind:"permissions" as const},{sequence:"2",extension_key:"mod",kind:"permissions" as const},{sequence:"3",extension_key:"mod",kind:"activation" as const}]};
  expect(await consumeModuleChanges(c,"o","p",page,async()=>{},async()=>{order.push("refresh")})).toBe("cursor-3");
  expect(order).toEqual(["refresh","permissions","activation"]);
 });
 it("requires snapshot before advancing and retries on failure",async()=>{
  const c=mock();const page={mode:"snapshot_required" as const,cursor:"new-epoch",items:[],has_more:false};
  await expect(consumeModuleChanges(c,"o","p",page,async()=>{throw Error("offline")},async()=>{})).rejects.toThrow("offline");
  expect(await consumeModuleChanges(c,"o","p",page,async()=>{},async()=>{})).toBe("new-epoch");
 });
 it("uses scoped replay URL and opaque cursor",async()=>{
  let url="";const c=new ApiClient({baseUrl:"https://example.test/api/v1",fetchImpl:(async(input)=>{url=String(input);return new Response(JSON.stringify({mode:"delta",cursor:"x",items:[],has_more:false}),{status:200})}) as typeof fetch});
  await replayModuleChanges(c,"o","p","epoch:99");expect(url).toContain("/organizations/o/projects/p/extensions/change-events?limit=100&cursor=epoch%3A99");
 });
});
