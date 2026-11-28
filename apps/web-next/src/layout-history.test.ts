import {describe,it,expect} from "vitest";
import {alignLayout,rememberLayout,undoLayout,redoLayout,moveLayoutGroup} from "./layout-history";
const items=[{id:"a",x:10,y:20,w:20,h:20},{id:"b",x:70,y:30,w:30,h:20},{id:"c",x:160,y:40,w:10,h:20}];
describe("layout editing",()=>{
 it("aligns to page or selected bounds without mutating",()=>{
  expect(alignLayout(items,new Set(["a"]),"center-x",200,100)[0]?.x).toBe(90);
  expect(alignLayout(items,new Set(["a","b"]),"right",200,100).map(i=>i.x)).toEqual([80,70,160]);
  expect(items[0]?.x).toBe(10);
 });
 it("distributes equal gaps and rejects overlap",()=>{
  expect(alignLayout(items,new Set(["a","b","c"]),"distribute-x",200,100)[1]?.x).toBe(80);
  expect(()=>alignLayout(items.map(i=>({...i,x:0})),new Set(["a","b","c"]),"distribute-x",200,100)).toThrow();
 });
 it("undo and redo; new edits clear redo",()=>{
  const h=rememberLayout({past:[],future:[]},items,[] as typeof items);
  const u=undoLayout(h,[] as typeof items)!;expect(u.document).toEqual(items);
  expect(redoLayout(u.history,u.document)?.document).toEqual([]);
  expect(rememberLayout(u.history,items,items.slice(1)).future).toEqual([]);
 });
});

it("moves the selected group with a common clamped delta",()=>{
 const result=moveLayoutGroup(items,new Set(["a","b"]),999,-999,200,100,true);
 expect(result.map(i=>[i.x,i.y])).toEqual([[110,0],[170,10],[160,40]]);
 expect(items[0]?.x).toBe(10);
 expect(moveLayoutGroup(items,new Set(["a","b"]),2.6,3.4,200,100,true)[1]?.x).toBe(73);
});
