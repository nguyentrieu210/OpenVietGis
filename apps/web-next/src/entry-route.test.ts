import {describe,it,expect} from "vitest";
import {entryRoute} from "./entry-route";
describe("route-specific entry", () => {
  it.each(["/offline-field/o/p","/devui/offline-field/o/p/"])("prioritizes offline route %s",path=>expect(entryRoute(path,"?qgis_connect=1")).toBe("offline"));
  it.each([["?qgis_connect=1&auth_complete=1","qgis"],["?auth_complete=1&marketplace=1","auth"],["?marketplace=1&developer=1","marketplace"],["?developer=1","developer"],["?marketplace=0","workspace"],["","workspace"]])("maps %s to %s",(query,route)=>expect(entryRoute("/app",query)).toBe(route));
  it("preserves deep workspace paths",()=>expect(entryRoute("/app/organizations/o/projects/p/cad","")).toBe("workspace"));
  it("does not treat incomplete paths as offline",()=>expect(entryRoute("/offline-field/o","")).toBe("workspace"));
});
