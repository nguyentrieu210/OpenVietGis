import {describe,it,expect} from "vitest";
import type {AdapterLayerDescriptor,IntegrationLayerExportResult,IntegrationSyncPreviewResponse,IntegrationSyncPushResponse} from "@openvietgis/client-sdk";
import {exportPin,verifySyncPreview,verifySyncPush,safeTransferName} from "./transfer-verification";
const layer={layer_id:"l",source_id:"s",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),export_permitted:true} as AdapterLayerDescriptor;
const file={contentRevision:layer.content_revision,schemaRevision:layer.schema_revision,sha256:"c".repeat(64),crs:"EPSG:4326",featureCount:2} as IntegrationLayerExportResult;
const p={adapter_key:"qgis",layer_id:"l",source_id:"s",base_content_revision:layer.content_revision,current_content_revision:layer.content_revision,base_schema_revision:layer.schema_revision,current_schema_revision:layer.schema_revision,diff_sha256:"d".repeat(64),deletions_blocked:true,create_count:1,update_count:1,omitted_server_feature_count:1,can_commit:true,stale_revision:false} as IntegrationSyncPreviewResponse;
const r={changeset_id:"r",audit_event_id:"a",layer_id:"l",source_id:"s",operation_id:"op",diff_sha256:p.diff_sha256,resulting_content_revision:"e".repeat(64),deletions_blocked:true,create_count:1,update_count:1,operation_count:2,omitted_server_feature_count:1} as IntegrationSyncPushResponse;
describe("transfer verification",()=>{
 it("rejects missing digest and mismatched source revisions",()=>{
  expect(()=>exportPin(layer,file)).not.toThrow();
  expect(()=>exportPin(layer,{...file,sha256:null})).toThrow();
  expect(()=>exportPin(layer,{...file,contentRevision:"foreign"})).toThrow();
 });
 it("rejects stale committable previews and mismatched push receipt",()=>{
  expect(verifySyncPreview(layer,p)).toBe(p);expect(verifySyncPush(p,"op",r)).toBe(r);
  expect(()=>verifySyncPreview(layer,{...p,stale_revision:true})).toThrow();
  expect(()=>verifySyncPreview(layer,{...p,deletions_blocked:false})).toThrow();
  expect(()=>verifySyncPush(p,"op",{...r,operation_id:"other"})).toThrow();
  expect(()=>verifySyncPush(p,"op",{...r,operation_count:1})).toThrow();
 });
 it("sanitizes duplicate archive names",()=>{const used=new Set<string>();expect(safeTransferName("../a.gpkg",used)).toBe("_a.gpkg");expect(safeTransferName("_A.gpkg",used)).toBe("_A-2.gpkg")});
});
