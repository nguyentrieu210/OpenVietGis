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
