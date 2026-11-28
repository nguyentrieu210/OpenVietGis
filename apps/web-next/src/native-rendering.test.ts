import {describe,it,expect} from "vitest";
import {createSnapIndex,indexedSnapCandidates} from "./snap-spatial-index";
import {vertexData} from "./gpu-vertex-handles";
import {taskPinData} from "./gpu-task-pins";
import type {ProjectTask} from "./project-tasks";
import type {Geometry,Position} from "geojson";
const project=(p:Position):[number,number]=>[p[0],p[1]];
describe("native rendering inputs",()=>{
 it("indexes edges crossing query even when neither endpoint is nearby",()=>{
  const g:Geometry={type:"LineString",coordinates:[[-100,0],[100,0],[100,100]]};
  const near=createSnapIndex(g,project,false)([0,0],2);
  expect(near.vertices).toEqual([]);expect(near.segments).toEqual([{a:[-100,0],b:[100,0]}]);
 });
 it("invalidates screen index on camera change",()=>{
  const map={},g:Geometry={type:"Point",coordinates:[2,3]};
  expect(indexedSnapCandidates(map,"a",g,project,false,[2,3],1).vertices).toEqual([[2,3]]);
  expect(indexedSnapCandidates(map,"b",g,p=>[p[0]+100,p[1]],false,[102,3],1).vertices).toEqual([[102,3]]);
 });
 it("keeps stable vertex IDs beyond previous DOM limit",()=>{
  const data=vertexData(Array.from({length:1000},(_,i)=>[i,0]));
  expect(data.features.length).toBe(1000);expect(data.features[999].id).toBe(999);
 });
 it("excludes completed and unlocated tasks and does not truncate at 300",()=>{
  const task=(i:number):ProjectTask=>({id:String(i),title:"task",description:"",status:"todo",priority:"normal",assignee_id:"a",assignee_name:"a",creator_name:"a",due_date:null,longitude:106,latitude:11});
  const tasks=Array.from({length:400},(_,i)=>task(i));tasks.push({...task(401),status:"done"},{...task(402),latitude:null});
  const data=taskPinData(tasks);expect(data.features.length).toBe(400);expect(data.features[399].properties?.taskId).toBe("399");
 });
});
