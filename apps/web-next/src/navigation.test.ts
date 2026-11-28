import {describe,it,expect} from "vitest";
import {decodeRoute,parsePoint} from "./navigation";
describe("navigation coordinates",()=>{
 it("requires explicit longitude and latitude order",()=>{expect(parsePoint("106.8, 10.9")).toEqual([106.8,10.9]);expect(parsePoint("181, 0")).toBeNull();expect(parsePoint("x,y")).toBeNull()});
 it("decodes Valhalla polyline6 and rejects truncated geometry",()=>{expect(decodeRoute("??@@")).toEqual([[0,0],[-0.000001,-0.000001]]);expect(()=>decodeRoute("_")).toThrow()});
});
