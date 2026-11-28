import {describe,it,expect} from "vitest";
import {fitVectorAffine,georeferenceVector,type VectorGcp} from "./vector-georeference";
const points:VectorGcp[]=[[1000,2000,106,11],[1100,2000,107,11],[1000,2100,106,12],[1100,2100,107,12]];
const raw={type:"FeatureCollection",bbox:[0,0,0,0],features:[{type:"Feature",id:"a",properties:{name:"x"},geometry:{type:"Point",coordinates:[1050,2050]}}]};
describe("vector affine georeferencing",()=>{
 it("fits independent points, keeps attributes and strips stale bbox",()=>{
  const out=georeferenceVector(raw,points);
  expect(out.collection.features[0]).toEqual({type:"Feature",id:"a",properties:{name:"x"},geometry:{type:"Point",coordinates:[106.5,11.5]}});
  expect(out.rms).toBeLessThan(1e-10);expect(out.collection).not.toHaveProperty("bbox");expect(raw.features[0]!.geometry.coordinates).toEqual([1050,2050]);
 });
 it("rejects collinear source, collapsed target, invalid coordinates and open rings",()=>{
  expect(()=>fitVectorAffine([[0,0,1,1],[1,1,2,2],[2,2,3,3]])).toThrow();
  expect(()=>fitVectorAffine([[0,0,1,1],[1,0,2,2],[0,1,3,3]])).toThrow();
  expect(()=>georeferenceVector({...raw,features:[{...raw.features[0],geometry:{type:"Point",coordinates:[1e12,1e12]}}]},points)).toThrow();
  expect(()=>georeferenceVector({...raw,features:[{...raw.features[0],geometry:{type:"Polygon",coordinates:[[[1000,2000],[1100,2000],[1100,2100],[1000,2100]]]}}]},points)).toThrow();
 });
});
