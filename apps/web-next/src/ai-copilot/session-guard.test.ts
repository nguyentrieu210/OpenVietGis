import {describe,it,expect} from "vitest";
import {pinCommand,assertCommandCurrent,validSession,recordHistory,type ContextPin} from "./session-guard.js";
const ctx:ContextPin={mode:"cad",epoch:1,cad:"doc:1"};
const input={id:"one",method:"cad_select_handle",params:{document_id:"doc",version:1,handle:"A"},expires_in:20};
describe("AI context and bounded history",()=>{
 it("rejects mode switch, return to same mode, different document and expiry",()=>{
  const c=pinCommand(input,ctx,1000);expect(()=>assertCommandCurrent(c,ctx,2000)).not.toThrow();
  expect(()=>assertCommandCurrent(c,{...ctx,mode:"gis"},2000)).toThrow();
  expect(()=>assertCommandCurrent(c,{...ctx,epoch:3},2000)).toThrow();
  expect(()=>assertCommandCurrent(c,{...ctx,cad:"doc:2"},2000)).toThrow();
  expect(()=>assertCommandCurrent(c,ctx,21000)).toThrow();
 });
 it("rejects unsupported mode or session response",()=>{
  expect(()=>pinCommand(input,{...ctx,mode:"gis"},1000)).toThrow();
  expect(validSession({session_id:"../bad",browser_token:"x",expires_in:900})).toBe(false);
  expect(validSession({session_id:"one",browser_token:"x",expires_in:900})).toBe(true);
 });
 it("history is bounded and excludes payload",()=>{
  const h=Array.from({length:30},(_,i)=>({id:String(i),method:"map_view",mode:"gis",status:"done",at:i}));
  const entry={id:"new",method:"map_view",mode:"gis",status:"unknown",at:31,params:{secret:"x"}};
  const result=recordHistory(h,entry);expect(result.length).toBe(20);expect(result[0]).not.toHaveProperty("params");
 });
});
