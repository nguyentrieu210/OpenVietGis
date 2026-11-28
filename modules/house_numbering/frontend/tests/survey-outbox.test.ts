import {describe,it,expect} from "vitest";
import {acknowledgedIds,scopeKey,draftKey,type SurveyOperation} from "../survey-outbox";
const op=(id:string):SurveyOperation=>({client_uuid:id,building_id:"b",condition:"house",queue_actor_id:"actor",expected_building_revision:1,photo_artifact_ids:[]});
describe("survey outbox acknowledgements",()=>{
 it("removes only explicitly confirmed rows from this request",()=>{
  expect([...acknowledgedIds([op("a"),op("b"),op("c")],[{client_uuid:"a",survey_id:"s",status:"applied"},{client_uuid:"outside",survey_id:"z",status:"applied"},{client_uuid:"b",survey_id:"q",status:"replayed"}],[{client_uuid:"b",code:"STALE_BUILDING"}])]).toEqual(["a"]);
 });
 it("missing or malformed receipts never acknowledge unknown rows",()=>{
  expect(acknowledgedIds([op("a")],undefined,[]).size).toBe(0);
  expect(acknowledgedIds([op("a")],[null,{client_uuid:"a",survey_id:"",status:"applied"},{client_uuid:"a",survey_id:"s",status:"unknown"}],[]).size).toBe(0);
 });
 it("isolates actor, organization and project keys without delimiter collisions",()=>{
  expect(scopeKey("a","o","p")).not.toBe(scopeKey("b","o","p"));
  expect(scopeKey("a","o","p")).not.toBe(scopeKey("a","o","q"));
  expect(draftKey(scopeKey("a","o:p","q"),"id")).not.toBe(draftKey(scopeKey("a","o","p:q"),"id"));
 });
});
