import {describe,expect,it,vi} from "vitest";
import {ApiClient,getPluginV2Runtime,invokePluginV2,listMarketplaceV2,setPluginV2Grant,type PluginV2Runtime} from "../src";
describe("SPEC075 read-only declarative runtime",()=>{
 it("preserves scope, immutable release, revision, request ID and CSRF",async()=>{
  const runtime:PluginV2Runtime={document:{slot:"workspace",widgets:[],actions:[]},manifest_id:"manifest",activation_revision:5,release_digest:"a".repeat(64),profile:"web_read_only",granted:true,grant_revision:"12",permissions:[],lock_digest:"b".repeat(64),catalog_epoch:2,dependency_lock:[]};
  const fetchImpl=vi.fn().mockImplementation(async()=>new Response(JSON.stringify(runtime),{status:200,headers:{"content-type":"application/json"}}));
  const client=new ApiClient({baseUrl:"https://gis.example/api/v1",fetchImpl});client.setCsrfToken("fixture");
  const controller=new AbortController();
  await getPluginV2Runtime(client,"org","project","manifest",controller.signal);
  await invokePluginV2(client,"org","project",runtime,"action",{},"request",controller.signal);
  await listMarketplaceV2(client,"web","1.0.0");
  await setPluginV2Grant(client,"org","project",runtime,false,controller.signal);
  expect(fetchImpl.mock.calls[0]?.[0]).toBe("https://gis.example/api/v1/organizations/org/projects/project/extensions/manifest/v2-runtime");
  const req=fetchImpl.mock.calls[1]?.[1] as RequestInit;
  expect(req.signal).toBe(controller.signal);
  expect(new Headers(req.headers).get("x-openvietgis-csrf")).toBe("fixture");
  expect(JSON.parse(String(req.body))).toEqual({request_id:"request",release_digest:"a".repeat(64),activation_revision:5,action_id:"action",input:{}});
  expect(fetchImpl.mock.calls[2]?.[0]).toContain("/marketplace/v2/catalog?host=web&api=1.0.0");
  expect(JSON.parse(String((fetchImpl.mock.calls[3]?.[1] as RequestInit).body))).toEqual({release_digest:"a".repeat(64),activation_revision:5,grant_revision:"12",lock_digest:"b".repeat(64),enabled:false});
 });
});
