import {describe,it,expect} from "vitest";
import {cameraRegion,nativeRegion,intersectsCloudNode} from "./cloud-region";
describe("point cloud camera cell selection",()=>{
 it("retains root, prunes nonintersecting descendants",()=>{const r=[1,1,2,2] as const,b=[0,0,0,8,8,8];expect(intersectsCloudNode([0,0,0,0],b,r)).toBe(true);expect(intersectsCloudNode([1,0,0,0],b,r)).toBe(true);expect(intersectsCloudNode([1,1,1,0],b,r)).toBe(false)});
 it("quantizes camera movement and converts WebMercator",()=>{expect(cameraRegion(106,10,3)).toBeUndefined();expect(cameraRegion(106,10,12)).not.toEqual(cameraRegion(107,10,12));expect(nativeRegion([0,0,1,1],3857)?.[2]).toBeCloseTo(111319.49,1)});
});
