import {describe,it,expect} from "vitest";
import {searchRibbonCommands,uniqueCommands} from "./command-search";
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

it("deduplicates normalized aliases without changing first-handler permission precedence",()=>{
 const first={alias:"UNION",label:"First",group:"Vector",disabled:true};
 const second={alias:"union",label:"Duplicate",group:"Vector",disabled:false};
 expect(uniqueCommands([first,second,...commands])).toEqual([first,...commands]);
 expect(searchRibbonCommands(uniqueCommands([first,second,...commands]),"UNION",{})).toEqual([first]);
});
