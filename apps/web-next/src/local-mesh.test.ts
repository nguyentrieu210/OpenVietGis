import {describe,it,expect} from "vitest";
import {parse2dm} from "./local-mesh";
const mesh="MESH2D\nE4Q 1 1 2 3 4 1\nND 1 106 10 1\nND 2 106.1 10 2\nND 3 106.1 10.1 3\nND 4 106 10.1 4";
describe("2DM preview",()=>{
 it("resolves out-of-order node IDs and triangulates convex quadrilaterals",()=>{const m=parse2dm(mesh,4326);expect(m.triangles).toEqual([0,1,2,0,2,3]);expect(m.minHeight).toBe(1);expect(m.maxHeight).toBe(4)});
 it("rejects duplicate nodes, unsupported records and missing nodes",()=>{expect(()=>parse2dm(mesh+"\nND 1 106 10 1",4326)).toThrow();expect(()=>parse2dm(mesh+"\nE6T 2 1 2 3 4 5 6 1",4326)).toThrow();expect(()=>parse2dm(mesh.replace("ND 4","ND 5"),4326)).toThrow()});
 it("rejects crossing quads and duplicate faces",()=>{expect(()=>parse2dm(mesh.replace("1 2 3 4 1","1 3 2 4 1"),4326)).toThrow();expect(()=>parse2dm(mesh+"\nE3T 2 3 2 1 1",4326)).toThrow()});
});
