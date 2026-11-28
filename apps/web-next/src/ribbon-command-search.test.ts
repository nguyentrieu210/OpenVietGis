import {describe,it,expect} from "vitest";
import {searchRibbonCommands} from "./command-search";
const commands=[
 {alias:"BUFFER",label:"Vùng đệm",group:"Vector",disabled:true},
 {alias:"RASTERCLIP",label:"Cắt raster",group:"Raster",hint:"Buffer bounds"},
 {alias:"TOOLBOX",label:"Hộp công cụ",group:"Xử lý",hint:"Buffer/Clip"},
 {alias:"MODELER",label:"Mô hình",group:"Xử lý",hint:"Buffer"},
];
describe("ribbon command search",()=>{
 it("returns only exact alias even when unavailable",()=>{
  expect(searchRibbonCommands(commands,"BUFFER",{})).toEqual([commands[0]]);
  expect(searchRibbonCommands(commands,"B",{B:"BUFFER"})).toEqual([commands[0]]);
 });
 it("keeps all fuzzy matches and original category order for an empty query",()=>{
  expect(searchRibbonCommands(commands,"buff",{})).toHaveLength(4);
  expect(searchRibbonCommands(commands,"vung dem",{})).toEqual([commands[0]]);
  expect(searchRibbonCommands(commands,"",{})).toEqual(commands);
 });
});

import {executeHostCommand} from "./command-search";
describe("shared host command execution",()=>{
 it("preserves CAD arguments and rechecks live permissions",async()=>{
  const args:string[]=[];let current=[{alias:"LINE",label:"Vẽ đường",group:"CAD",disabled:false,run:(v:string)=>{args.push(v)}}];
  await executeHostCommand(()=>current,"line","106.6,11.1 106.7,11.2",()=>"Không có quyền");
  expect(args).toEqual(["106.6,11.1 106.7,11.2"]);
  current=[{...current[0]!,disabled:true}];
  await expect(executeHostCommand(()=>current,"LINE","1,2",()=>"Không có quyền")).rejects.toThrow("Không có quyền");
  await expect(executeHostCommand(()=>current,"LIN","",()=>"Không có quyền")).rejects.toThrow("không còn khả dụng");
  current=[current[0]!,current[0]!];
  await expect(executeHostCommand(()=>current,"LINE","",()=>"Không có quyền")).rejects.toThrow("trùng");
  expect(args).toHaveLength(1);
 });
});