import {describe,expect,it,vi} from "vitest";
import {ApiClient,getPluginV2Runtime,invokePluginV2,listOwnPluginAudit,listMarketplaceV2,setPluginV2Grant,type PluginV2Runtime} from "../src";
describe("SPEC075 read-only declarative runtime",()=>{
 it("preserves scope, immutable release, revision, request ID and CSRF",async()=>{
  const runtime:PluginV2Runtime={document:{slot:"workspace",widgets:[],actions:[{id:"action",command_id:"host_context",permission:"ext.partner_fixture_context.host_context",consent:"read",timeout_ms:1000,input:{}}]},manifest_id:"10000000-0000-0000-0000-000000000001",activation_revision:5,release_digest:"a".repeat(64),profile:"web_read_only",granted:true,grant_revision:"12",permissions:["ext.partner_fixture_context.host_context"],lock_digest:"b".repeat(64),catalog_epoch:2,dependency_lock:[{key:"partner_fixture_context",version:"1.0.0",release_digest:"a".repeat(64),manifest_id:"10000000-0000-0000-0000-000000000001",activation_id:"20000000-0000-0000-0000-000000000001",activation_revision:5}]};
  const fetchImpl=vi.fn().mockImplementation(async()=>new Response(JSON.stringify(runtime),{status:200,headers:{"content-type":"application/json"}}));
  const client=new ApiClient({baseUrl:"https://gis.example/api/v1",fetchImpl});client.setCsrfToken("fixture");
  const controller=new AbortController();
  await getPluginV2Runtime(client,"org","project",runtime.manifest_id,controller.signal);
  await invokePluginV2(client,"org","project",runtime,"action",{},"30000000-0000-0000-0000-000000000001",controller.signal);
  await listMarketplaceV2(client,"web","1.0.0");
  await setPluginV2Grant(client,"org","project",runtime,false,controller.signal);
  await listOwnPluginAudit(client,"org","project","cursor",50,controller.signal);
  expect(fetchImpl.mock.calls[4]?.[0]).toBe("https://gis.example/api/v1/organizations/org/projects/project/extensions/v2-audit?limit=50&after_id=cursor");
  expect((fetchImpl.mock.calls[4]?.[1] as RequestInit).signal).toBe(controller.signal);
  expect(fetchImpl.mock.calls[0]?.[0]).toBe("https://gis.example/api/v1/organizations/org/projects/project/extensions/10000000-0000-0000-0000-000000000001/v2-runtime");
  const req=fetchImpl.mock.calls[1]?.[1] as RequestInit;
  expect(req.signal).toBe(controller.signal);
  expect(new Headers(req.headers).get("x-openvietgis-csrf")).toBe("fixture");
  expect(JSON.parse(String(req.body))).toEqual({request_id:"30000000-0000-0000-0000-000000000001",release_digest:"a".repeat(64),activation_revision:5,grant_revision:runtime.grant_revision,lock_digest:runtime.lock_digest,catalog_epoch:runtime.catalog_epoch,action_id:"action",input:{}});
  expect(fetchImpl.mock.calls[2]?.[0]).toContain("/marketplace/v2/catalog?host=web&api=1.0.0");
  expect(JSON.parse(String((fetchImpl.mock.calls[3]?.[1] as RequestInit).body))).toEqual({release_digest:"a".repeat(64),activation_revision:5,grant_revision:"12",lock_digest:"b".repeat(64),enabled:false});
 });
});
