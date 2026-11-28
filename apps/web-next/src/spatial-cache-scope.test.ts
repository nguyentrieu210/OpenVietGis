import {describe,it,expect} from "vitest";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import {cacheAuthorized,cacheMatches,cacheScopeMatches,permissionFingerprint,type CacheEntry} from "./SpatialCache";
const scope={account:"actor",org:"org",project:"project"};
const permissions=["project.read","project.edit"];
const layer={id:"layer",organization_id:"org",project_id:"project",source_id:"source",layer_revision:1,source:{feature_count:0,content_revision:"c1",schema_revision:"s1"},capabilities:[{name:"feature_read",available:true,actor_authorized:true}]} as unknown as SpatialLayer;
const entry:CacheEntry={key:JSON.stringify(["actor","org","project","layer"]),scope,layer,rows:[],bytes:0,saved:Date.now(),permissionKey:permissionFingerprint(permissions)};
describe("spatial cache authorization binding",()=>{
 it("allows only complete same-scope same-revision snapshots",()=>{
  expect(cacheAuthorized(false,permissions,layer)).toBe(true);
  expect(cacheMatches(entry,layer,scope,[...permissions].reverse())).toBe(true);
  expect(cacheMatches(entry,{...layer,source:{...layer.source,content_revision:"c2"}},scope,permissions)).toBe(false);
 });
 it("rejects other accounts, organizations, projects and malformed cache keys",()=>{
  for(const s of [{...scope,account:"other"},{...scope,org:"other"},{...scope,project:"other"}])expect(cacheScopeMatches(entry,layer,s,permissions)).toBe(false);
  expect(cacheScopeMatches({...entry,key:"wrong"},layer,scope,permissions)).toBe(false);
  expect(cacheScopeMatches(entry,{...layer,project_id:"other"},scope,permissions)).toBe(false);
 });
 it("rejects old entries and permission/capability changes even if data revision is unchanged",()=>{
  expect(cacheScopeMatches({...entry,permissionKey:undefined},layer,scope,permissions)).toBe(false);
  expect(cacheScopeMatches(entry,layer,scope,["project.read"])).toBe(false);
  const denied={...layer,capabilities:layer.capabilities.map(c=>({...c,actor_authorized:false}))};
  expect(cacheScopeMatches(entry,denied,scope,permissions)).toBe(false);
  expect(cacheAuthorized(false,permissions,denied)).toBe(false);
  expect(cacheAuthorized(true,permissions,layer)).toBe(false);
 });
});
