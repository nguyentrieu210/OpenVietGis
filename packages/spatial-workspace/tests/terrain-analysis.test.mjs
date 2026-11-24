import {test} from "node:test";
import assert from "node:assert/strict";
import {geographic3857,validateTerrain,terrainTriangles} from "../src/terrain.ts";
import {interpolateTIN,gridGeoTiff,cutFill,terrainDerivatives,flowD8,contours,analysisFeatures,elevationPoints} from "../src/terrain-analysis.ts";
const grid=(values,width=3,height=3)=>{
 const a=values.filter(v=>v!==null);
 return validateTerrain({source_id:"test",name:"test",crs_code:3857,source_crs_code:3857,width,height,extent:[0,0,(width-1)*10,(height-1)*10],values,min_m:Math.min(...a),max_m:Math.max(...a),render_origin_m:Math.min(...a),valid_count:a.length,nodata_count:values.length-a.length,vertical_reference:"local",reference_label:"BM",elevation_unit:"metre",sampled:true,survey_grade:false,declared_by_user:true},"test");
};
const near=(a,b,tol=.00001)=>assert.ok(Math.abs(a-b)<tol,a+" != "+b);
test("flat reference plane cut/fill, metric area near equator",()=>{
 const g=grid(Array(9).fill(12)),r=cutFill(g,10);
 near(r.coveredArea,400);near(r.cut,800);near(r.fill,0);
 near(cutFill(g,14).fill,800);
});
test("cut-fill crossing triangle integrated, net zero",()=>{
 const g=grid([0,10,0,10],2,2),v=cutFill(g,5);
 near(v.cut,125);near(v.fill,125);near(v.net,0);near(v.coveredArea,100);
});
test("slope and aspect on planar DEM, missing cells remain null",()=>{
 const g=grid([0,10,20,0,10,20,0,10,20]),d=terrainDerivatives(g);
 near(d.slope[4],45);near(d.aspect[4],270);assert.equal(d.slope[0],null);
 assert.equal(terrainDerivatives(grid([0,10,20,null,10,20,0,10,20])).slope[4],null);
 assert.equal(terrainDerivatives(grid(Array(9).fill(5))).aspect[4],null);
});
test("D8 strictly downhill, no cycles, accumulation reaches terminal",()=>{
 const g=grid([8,7,6,5,4,3,2,1,0]),f=flowD8(g);
 assert.equal(f.sinks,1);near(f.acc[8],400);
 for(let i=0;i<8;i++)assert.ok(g.values[f.down[i]]<g.values[i]);
 assert.equal(f.down[8],-1);
});
test("TIN plane recovery, no outside convex hull, long-edge and conflict guards",()=>{
 const points=[[0,0,0],[100,0,10],[0,100,20]].map(([x,y,z])=>[...geographic3857(x,y),z]);
 const g=interpolateTIN(points,200,"BM","local",16);
 assert.equal(g.nodata_count>0,true);assert.ok(terrainTriangles(g).length>0);
 assert.equal(g.values[15],null);near(g.values[0],20);near(g.values[240],0);near(g.values[255],10);
 assert.throws(()=>interpolateTIN(points,20,"BM"),/tam giác/);
 assert.throws(()=>interpolateTIN([...points,[...points[0].slice(0,2),10]],200,"BM"),/khác cao độ/);
});
test("contours, mask, no extrapolation and TIFF nodata tags",()=>{
 const g=grid([0,10,20,0,10,20,0,10,20]),f=contours(g,5);
 assert.ok(f.features.length>0);assert.ok(f.features.every(x=>x.properties.elevation_m%5===0));
 assert.throws(()=>contours(g,.00001),/120/);
 assert.equal(analysisFeatures(g,"flood",-1).features.length,0);
 assert.equal(analysisFeatures(g,"flood",100).features.length,4);
 assert.equal(analysisFeatures(grid([0,10,null,0,10,null,0,10,null]),"elevation",0).features.length,2);
 const bytes=new DataView(gridGeoTiff(g));assert.equal(bytes.getUint16(2,true),42);assert.equal(bytes.getUint16(8,true),16);
 assert.throws(()=>elevationPoints({type:"Polygon",coordinates:[]},10),/điểm/);
});
