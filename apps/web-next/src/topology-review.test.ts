import {describe,it,expect} from "vitest";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import {verifyTopology,sameReadPin,reviewGeometry,type TopologyPopulation} from "./topology-review";
const l={id:"l",source_id:"s",lifecycle:"active",source:{stored_crs:{code:4326},validation_state:"ready",content_revision:"c",schema_revision:"r",feature_count:2},capabilities:[{name:"feature_read",available:true,actor_authorized:true}]} as unknown as SpatialLayer;
const r:TopologyPopulation={source_id:"s",content_revision:"c",schema_revision:"r",requested_count:2,scanned_count:1,skipped_count:1,complete:false,truncated:false,secondary_pin:null,items:[]};
describe("topology evidence",()=>{
 it("does not accept skipped geometry as clean",()=>{
  expect(verifyTopology(r,l,undefined)).toBe(r);expect(()=>verifyTopology({...r,complete:true},l,undefined)).toThrow();expect(()=>verifyTopology({...r,requested_count:1},l,undefined)).toThrow();
 });
 it("requires current pin and reference revision",()=>{
  expect(sameReadPin(l,l)).toBe(true);expect(sameReadPin({...l,capabilities:[]},l)).toBe(false);
  expect(()=>verifyTopology(r,l,l)).toThrow();
 });
 it("validates preview coordinates without repairing invalid polygon",()=>{
  expect(reviewGeometry({type:"Polygon",coordinates:[[[0,0],[1,1],[1,0],[0,1],[0,0]]]})).toBe(true);
  expect(reviewGeometry({type:"Point",coordinates:[181,0]})).toBe(false);
 });
});
