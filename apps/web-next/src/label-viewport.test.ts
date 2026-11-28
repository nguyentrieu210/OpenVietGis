import {describe, expect, it} from "vitest";
import {visiblePolygonAnchor} from "./label-viewport";
const rect=(x:number,y:number,w:number,h:number)=>[{x,y},{x:x+w,y},{x:x+w,y:y+h},{x,y:y+h},{x,y}];
describe("visible polygon label anchor",()=>{
  it("keeps the label visible when zooming into a corner of a large polygon",()=>{
    expect(visiblePolygonAnchor([rect(-900,-700,1000,800)],200,200,12,12)).toEqual({x:56,y:56});
  });
  it("handles a polygon enclosing the viewport",()=>{
    expect(visiblePolygonAnchor([rect(-500,-500,1000,1000)],200,100,12,12)).toEqual({x:100,y:50});
  });
  it("avoids placing a label inside a hole",()=>{
    const point=visiblePolygonAnchor([rect(-100,-100,400,400),rect(40,-50,120,300)],200,100,10,10);
    expect(point).not.toBeNull();
    expect(point!.x<=40||point!.x>=160).toBe(true);
  });
  it("omits offscreen polygons and viewports entirely in a hole",()=>{
    expect(visiblePolygonAnchor([rect(300,300,50,50)],200,100,10,10)).toBeNull();
    expect(visiblePolygonAnchor([rect(-100,-100,400,400),rect(-20,-20,240,140)],200,100,10,10)).toBeNull();
  });
});
