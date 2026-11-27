import {describe,it,expect} from "vitest";
import {readWorkspaceRoute} from "./route-compat";
describe("workspace compatibility",()=>{
 it.each(["/app","", "/devui/app"])("maps project workbench from %s",base=>expect(readWorkspaceRoute(base+"/organizations/org2/projects/project2/queries")).toMatchObject({org:"org2",project:"project2",tool:"queries"}));
 it("ignores unknown and prototype names",()=>{expect(readWorkspaceRoute("/app/organizations/o/projects/p/toString").tool).toBeNull();expect(readWorkspaceRoute("/app/organizations/o/projects/p/map").tool).toBeNull()});
 it("rejects malformed encoding",()=>expect(readWorkspaceRoute("/app/organizations/%E0/projects/p/queries").org).toBe(""));
});
