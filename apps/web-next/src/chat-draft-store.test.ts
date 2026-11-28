import {describe,it,expect} from "vitest";
import {RoomDraftStore,mergeChatWindow} from "./chat-draft-store";
describe("chat scope",()=>{
 it("retains separate drafts and prunes rooms whose permission was removed",()=>{
  const s=new RoomDraftStore(()=>({body:"",mentions:[] as string[]}),2);
  s.update("a",()=>({body:"A",mentions:["u"]}));s.update("b",()=>({body:"B",mentions:[]}));
  expect(s.get("a").body).toBe("A");expect(s.get("b").body).toBe("B");
  s.retain(new Set(["b"]));expect(s.get("a").body).toBe("");expect(s.get("b").body).toBe("B");
  s.clear();expect(s.get("b").body).toBe("");
  expect(s.claim("b")).toBe(true);expect(s.claim("b")).toBe(false);s.release("b");expect(s.claim("b")).toBe(true);
 });
 it("bounds retained drafts and preserves unrelated room",()=>{
  const s=new RoomDraftStore(()=>"",2);s.update("a",()=>"A");s.update("b",()=>"B");s.update("a",v=>v+"1");s.update("c",()=>"C");
  expect(s.get("a")).toBe("A1");expect(s.get("b")).toBe("");expect(s.get("c")).toBe("C");
 });
 it("deduplicates refreshed messages, preserves older page and replaces current window",()=>{
  const m=(id:string,created_at="2026-11-01")=>({id,created_at});
  expect(mergeChatWindow([m("a"),m("b"),m("c")],[m("c"),m("b"),m("d")]).map(x=>x.id)).toEqual(["a","b","c","d"]);
  expect(mergeChatWindow([m("a")],[])).toEqual([]);
 });
});
