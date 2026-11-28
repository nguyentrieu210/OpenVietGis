import type {DerivedArtifact,ExportPreviewInput,ExportPreviewResponse,TransferJob} from "@openvietgis/client-sdk";
export type BinaryPlan={input:ExportPreviewInput;preview:ExportPreviewResponse;op:string};
const hex=(v:unknown)=>typeof v==="string"&&/^[a-f0-9]{64}$/.test(v);
export function validateBinaryPreview(input:ExportPreviewInput,source:string,count:number|null,p:ExportPreviewResponse){
 if(!p.confirmable||p.layer_id!==input.layer_id||p.source_id!==source||p.content_revision!==input.content_revision||p.schema_revision!==input.schema_revision||p.output_format!==input.output_format||p.output_crs!==input.output_crs||p.feature_count!==count||!hex(p.preview_fingerprint)||!Array.isArray(p.disclosures)||!Array.isArray(p.field_mappings))throw Error("Bản xem trước không khớp lớp, CRS hoặc revision.");
 return p;
}
export function validateBinaryArtifact(plan:BinaryPlan,org:string,pid:string,job:TransferJob,a:DerivedArtifact){
 const p=plan.preview;
 if(job.operation_id!==plan.op||job.kind!=="export"||job.status!=="succeeded"||job.organization_id!==org||job.project_id!==pid||job.target_source_id!==p.source_id||job.target_layer_id!==p.layer_id||job.artifact_id!==a.id||a.job_id!==job.id||a.organization_id!==org||a.project_id!==pid||a.source_id!==p.source_id||a.layer_id!==p.layer_id||a.source_content_revision!==p.content_revision||a.source_schema_revision!==p.schema_revision||a.output_format!==p.output_format||a.output_crs!==p.output_crs||a.state!=="ready"||!hex(a.sha256)||!Number.isSafeInteger(a.byte_size)||a.byte_size<=0||a.byte_size>64*1024*1024||a.comparison_report?.exported_feature_count!==p.feature_count)throw Error("Biên nhận tệp không khớp bản xem trước.");
 return a;
}
export async function validateBinaryBytes(a:DerivedArtifact,d:{bytes:ArrayBuffer;sha256:string|null;contentType:string|null}){
 const media=a.output_format==="geopackage"?"application/geopackage+sqlite3":"application/zip";
 if(d.bytes.byteLength!==a.byte_size||d.sha256!==a.sha256||d.contentType?.split(";")[0]!==media||a.content_type!==media||!crypto.subtle)throw Error("Tệp tải về thiếu thông tin xác minh.");
 const hash=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",d.bytes)),b=>b.toString(16).padStart(2,"0")).join("");
 if(hash!==a.sha256)throw Error("SHA-256 tệp tải về không khớp.");
 return new Blob([d.bytes],{type:media});
}
