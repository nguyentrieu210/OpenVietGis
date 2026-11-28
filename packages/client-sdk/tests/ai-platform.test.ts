import {describe,it,expect,vi} from "vitest";
import {AiClient,requestAi} from "../src/ai-platform";
const session={session_id:"12345678-1234-4234-8234-123456789abc",browser_token:"fixture"};
describe("AI platform v1",()=>{
 it("pins same-origin session and never retries",async()=>{
  const send=vi.fn<typeof fetch>().mockResolvedValue(new Response("{}",{status:403}));
  const c=new AiClient(session,send);
  await expect(c.run({provider:"openai",expected_model:"fixture",user:"inspect",skill:null,consent:true,allow_writes:false,multi_agent:false})).rejects.toThrow("403");
  expect(send).toHaveBeenCalledTimes(1);
  expect(send.mock.calls[0]?.[0]).toBe("/ai-copilot/agent/"+session.session_id+"/run");
  expect(send.mock.calls[0]?.[1]).toMatchObject({redirect:"error",credentials:"same-origin",cache:"no-store"});
 });
 it("rejects incompatible versions and traversal",async()=>{
  const send=vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({platform:{version:2}})));
  await expect(new AiClient(session,send).discover()).rejects.toThrow("version");
  await expect(requestAi({...session,session_id:"../evil"},"providers",{},undefined,send)).rejects.toThrow("Invalid");
  expect(send).toHaveBeenCalledTimes(1);
 });
});
