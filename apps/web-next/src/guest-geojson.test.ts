import {describe,expect,it} from "vitest";
import {parseGuestGeoJson,validPosition} from "./guest-geojson";
const feature=(geometry:unknown)=>({type:"Feature",geometry,properties:{}});
describe("guest local GIS import",()=>{
 it("accepts points, lines and closed polygons",()=>{
 const features=[feature({type:"Point",coordinates:[106,10]}),feature({type:"LineString",coordinates:[[106,10],[107,11]]}),feature({type:"Polygon",coordinates:[[[106,10],[107,10],[107,11],[106,10]]]})];
 expect(parseGuestGeoJson({type:"FeatureCollection",features}).features).toHaveLength(3);
 });
 it("rejects invalid coordinates and unclosed rings",()=>{
 expect(validPosition([181,0])).toBe(false);
 expect(()=>parseGuestGeoJson(feature({type:"Point",coordinates:[0,91]}))).toThrow();
 expect(()=>parseGuestGeoJson(feature({type:"Polygon",coordinates:[[[0,0],[1,0],[1,1],[0,1]]]}))).toThrow();
 });
 it("rejects invalid or oversized feature collections",()=>{
 expect(()=>parseGuestGeoJson({type:"FeatureCollection",features:[{}]})).toThrow();
 expect(()=>parseGuestGeoJson({type:"FeatureCollection",features:Array(10001).fill(feature({type:"Point",coordinates:[0,0]}))})).toThrow();
 });
});
