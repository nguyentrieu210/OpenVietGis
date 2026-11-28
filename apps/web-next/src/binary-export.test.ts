import {describe,it,expect} from "vitest";
import {validateBinaryPreview,validateBinaryArtifact} from "./binary-export";
import type {ExportPreviewResponse,ExportPreviewInput,TransferJob,DerivedArtifact} from "@openvietgis/client-sdk";
const input:ExportPreviewInput={layer_id:"l",content_revision:"c",schema_revision:"s",output_format:"geopackage",output_crs:4326};
const p={...input,source_id:"source",source_crs:4326,geometry_family:"Point",confirmable:true,feature_count:3,preview_fingerprint:"a".repeat(64),disclosures:[],field_mappings:[]} as ExportPreviewResponse;
const job={id:"j",operation_id:"op",kind:"export",status:"succeeded",organization_id:"o",project_id:"p",target_source_id:"source",target_layer_id:"l",artifact_id:"a"} as TransferJob;
const a={id:"a",job_id:"j",organization_id:"o",project_id:"p",source_id:"source",layer_id:"l",source_content_revision:"c",source_schema_revision:"s",output_format:"geopackage",output_crs:4326,state:"ready",sha256:"b".repeat(64),byte_size:10,filename:"a.gpkg",content_type:"application/geopackage+sqlite3",created_by_display:"QA",created_at:"2026-11-28",expires_at:"2026-11-29",comparison_report:{exported_feature_count:3}} as DerivedArtifact;
describe("binary export pins",()=>{
 it("rejects changed preview scope/count",()=>{expect(validateBinaryPreview(input,"source",3,p)).toBe(p);expect(()=>validateBinaryPreview(input,"source",4,p)).toThrow();expect(()=>validateBinaryPreview(input,"other",3,p)).toThrow()});
 it("rejects unrelated or oversized receipt",()=>{const plan={input,preview:p,op:"op"};expect(validateBinaryArtifact(plan,"o","p",job,a)).toBe(a);expect(()=>validateBinaryArtifact(plan,"o","p",job,{...a,byte_size:100*1024*1024})).toThrow();expect(()=>validateBinaryArtifact(plan,"o","p",{...job,operation_id:"other"},a)).toThrow()});
});
