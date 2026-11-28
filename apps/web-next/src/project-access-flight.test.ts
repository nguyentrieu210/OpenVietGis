import {describe,it,expect,vi} from "vitest";
import {ApiClient} from "@openvietgis/client-sdk";
import {projectAccessFlight,invalidateProjectAccess} from "./project-access-flight";
const scope={account:"actor",org:"org",project:"project"};
function fixture(){
 let release!:()=>void;
 const gate=new Promise<void>(r=>release=r);
 const fetchImpl=vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{
  await gate;
  return new Response(JSON.stringify(String(input).includes("effective-permissions")?{permissions:["project.read"]}:{restricted:false}),{status:200});
 });
 return {client:new ApiClient({baseUrl:"/api/v1",fetchImpl}),fetchImpl,release};
}
describe("scoped in-flight access checks",()=>{
 it("joins only simultaneous same-scope checks and never reuses settled permissions",async()=>{
  const {client,fetchImpl,release}=fixture();
  const a=projectAccessFlight(client,scope),b=projectAccessFlight(client,scope);
  expect(a).toBe(b);expect(fetchImpl).toHaveBeenCalledTimes(2);
  release();await a;await projectAccessFlight(client,scope);expect(fetchImpl).toHaveBeenCalledTimes(4);
  expect(fetchImpl.mock.calls.every(([,init])=>init?.cache==="no-store")).toBe(true);
 });
 it("isolates actor, project, organization and client",async()=>{
  const {client,fetchImpl,release}=fixture();const other=fixture();
  const work=[projectAccessFlight(client,scope),projectAccessFlight(client,{...scope,account:"other"}),projectAccessFlight(client,{...scope,project:"other"}),projectAccessFlight(client,{...scope,org:"other"}),projectAccessFlight(other.client,scope)];
  expect(fetchImpl).toHaveBeenCalledTimes(8);expect(other.fetchImpl).toHaveBeenCalledTimes(2);
  release();other.release();await Promise.all(work);
 });
 it("cancels old results even if transport ignores abort, and never poisons next flight",async()=>{
  const {client,release}=fixture();
  const old=projectAccessFlight(client,scope);const rejected=expect(old).rejects.toMatchObject({name:"AbortError"});
  invalidateProjectAccess(client);
  const next=projectAccessFlight(client,scope);expect(next).not.toBe(old);
  release();await rejected;await expect(next).resolves.toMatchObject({access:{restricted:false}});
 });
 it("denial is never cached and malformed success fails closed",async()=>{
  let n=0;const client=new ApiClient({baseUrl:"/api/v1",fetchImpl:async(input)=>{
   n++;return new Response(JSON.stringify(String(input).includes("effective-permissions")?{permissions:["project.read"]}:{}),{status:n===1?403:200});
  }});
  await expect(projectAccessFlight(client,scope)).rejects.toMatchObject({status:403});
  await expect(projectAccessFlight(client,scope)).rejects.toThrow("Invalid project authorization response");
  expect(n).toBe(4);
 });
});
