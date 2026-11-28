import type {AdapterLayerDescriptor,IntegrationLayerExportResult,IntegrationSyncPreviewResponse,IntegrationSyncPushResponse} from "@openvietgis/client-sdk";
const hex64=(s:unknown):s is string=>typeof s==="string"&&/^[0-9a-f]{64}$/.test(s);
export function exportPin(layer:Pick<AdapterLayerDescriptor,"export_permitted"|"content_revision"|"schema_revision">,result:IntegrationLayerExportResult){
 if(!layer.export_permitted||result.contentRevision!==layer.content_revision||result.schemaRevision!==layer.schema_revision||!hex64(result.sha256)||!result.crs||!Number.isInteger(result.featureCount)||result.featureCount!<0)throw Error("Biên nhận xuất không khớp revision hoặc thiếu thông tin xác minh. Tải lại hồ sơ lớp.");
}
export async function verifyExport(layer:Pick<AdapterLayerDescriptor,"export_permitted"|"content_revision"|"schema_revision">,result:IntegrationLayerExportResult){
 exportPin(layer,result);
 if(result.blob.size>128*1024*1024)throw Error("Tệp vượt giới hạn xác minh 128 MiB.");
 if(!globalThis.crypto?.subtle)throw Error("Cần HTTPS để xác minh SHA-256.");
 const bytes=await crypto.subtle.digest("SHA-256",await result.blob.arrayBuffer());
 const hash=Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,"0")).join("");
 if(hash!==result.sha256)throw Error("SHA-256 tệp xuất không khớp.");
 return result;
}
export function verifySyncPreview(layer:AdapterLayerDescriptor,p:IntegrationSyncPreviewResponse){
 if(p.adapter_key!=="qgis"||p.layer_id!==layer.layer_id||p.source_id!==layer.source_id||p.base_content_revision!==layer.content_revision||p.base_schema_revision!==layer.schema_revision||!hex64(p.diff_sha256)||!p.deletions_blocked||!Number.isInteger(p.create_count)||p.create_count<0||!Number.isInteger(p.update_count)||p.update_count<0)throw Error("Phản hồi diff không khớp lớp hoặc phạm vi thao tác.");
 if(p.can_commit&&(p.stale_revision||p.current_content_revision!==layer.content_revision||p.current_schema_revision!==layer.schema_revision||p.create_count+p.update_count===0))throw Error("Diff không đủ điều kiện xác nhận.");
 return p;
}
export function verifySyncPush(p:IntegrationSyncPreviewResponse,op:string,r:IntegrationSyncPushResponse){
 if(!r.changeset_id||!r.audit_event_id||r.layer_id!==p.layer_id||r.source_id!==p.source_id||r.operation_id!==op||r.diff_sha256!==p.diff_sha256||!hex64(r.resulting_content_revision)||r.resulting_content_revision===p.base_content_revision||!r.deletions_blocked||r.create_count!==p.create_count||r.update_count!==p.update_count||r.operation_count!==p.create_count+p.update_count||r.omitted_server_feature_count!==p.omitted_server_feature_count)throw Error("Không xác minh được biên nhận đồng bộ.");
 return r;
}
export function safeTransferName(name:string,used:Set<string>){
 const clean=name.replace(/[\\/:*?"<>|\u0000-\u001f]/g,"_").replace(/^\.+/,"").slice(0,180)||"layer";
 const dot=clean.lastIndexOf("."),stem=dot>0?clean.slice(0,dot):clean,ext=dot>0?clean.slice(dot):"";
 let value=clean,n=2;while(used.has(value.toLowerCase()))value=stem+"-"+n+++ext;
 used.add(value.toLowerCase());return value;
}
