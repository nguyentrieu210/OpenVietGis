import {describe,expect,it} from "vitest";
import {roleResolver} from "../layer-roles";
import type {SpatialLayer,ModuleLayerBindings} from "@openvietgis/client-sdk";
const layer=(id:string,name:string,geometry="Point",revision="a".repeat(64))=>({id,source_id:id,name,lifecycle:"active",source:{schema_revision:revision,schema:{geometry_family:geometry,fields:[{field_id:"ap"},{field_id:"ten_duong"}]}}}) as unknown as SpatialLayer;
const configured=(source_id:string,role="survey_points",status:"ready"|"unavailable"|"schema_changed"="ready"):ModuleLayerBindings=>({configured:true,revision:1,items:[{source_id,role,schema_revision:"a".repeat(64),current_schema_revision:"a".repeat(64),status}]});
describe("explicit module layer bindings",()=>{
 it("keeps identity after rename and does not adopt lookalike names",()=>{const renamed=layer("one","New name"),lookalike=layer("two","Khảo sát số nhà");const resolve=roleResolver(configured("one"),[renamed,lookalike]);expect(resolve(renamed)).toBe("survey_points");expect(resolve(lookalike)).toBeUndefined();});
 it("refuses changed schema, retired binding and wrong geometry",()=>{const current=layer("one","Khảo sát số nhà","Point","b".repeat(64));expect(roleResolver(configured("one"),[current])(current)).toBeUndefined();const normal=layer("one","A");expect(roleResolver(configured("one","survey_points","unavailable"),[normal])(normal)).toBeUndefined();const polygon=layer("one","A","Polygon");expect(roleResolver(configured("one"),[polygon])(polygon)).toBeUndefined();});
 it("legacy names only apply when server confirms unconfigured state",()=>{const old=layer("one","Khảo sát số nhà");expect(roleResolver(null,[old])(old)).toBeUndefined();expect(roleResolver({revision:0,configured:false,items:[]},[old])(old)).toBe("survey_points");expect(roleResolver({revision:1,configured:true,items:[]},[old])(old)).toBeUndefined();});
});
