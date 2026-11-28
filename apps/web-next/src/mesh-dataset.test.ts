import {describe,it,expect} from "vitest";
import {parse2dm} from "./local-mesh";
import {parseMeshDataset} from "./mesh-dataset";
const mesh=parse2dm("MESH2D\nND 3 106 10 0\nND 1 106.1 10 0\nND 2 106 10.1 0\nE3T 1 3 1 2 1",4326);
const raw={version:1,name:"depth",unit:"m",kind:"scalar",node_ids:[1,2,3],frames:[{time:"2026-11-28T00:00:00Z",values:[1,2,3]},{time:"2026-11-28T01:00:00Z",values:[null,5,6]}]};
describe("mesh dataset",()=>{
 it("maps explicit IDs, preserves missing values and shares a scale",()=>{const d=parseMeshDataset(raw,mesh);expect(d.frames).toEqual([[3,1,2],[6,null,5]]);expect([d.min,d.max]).toEqual([1,6])});
 it("supports vector magnitude and rejects invalid dates/ids",()=>{expect(parseMeshDataset({...raw,kind:"vector",frames:[{time:raw.frames[0]!.time,values:[[3,4],[0,0],null]}]},mesh).frames[0]).toEqual([null,5,0]);expect(()=>parseMeshDataset({...raw,node_ids:[1,1,3]},mesh)).toThrow();expect(()=>parseMeshDataset({...raw,frames:[{...raw.frames[0],time:"2026-02-30T00:00:00Z"}]},mesh)).toThrow();expect(()=>parseMeshDataset({...raw,frames:[raw.frames[1],raw.frames[0]]},mesh)).toThrow()});
});
