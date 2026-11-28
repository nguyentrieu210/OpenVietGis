import type {SpatialFeaturePage,SpatialFeatureReadContext,SpatialLayer,SpatialFeature} from "@openvietgis/client-sdk";
export function verifyReadContext(context:SpatialFeatureReadContext,layer:SpatialLayer,org:string,project:string){
 if(!context||context.organization_id!==org||context.project_id!==project||context.layer_id!==layer.id||context.source_id!==layer.source_id||context.content_revision!==layer.source.content_revision||context.schema_revision!==layer.source.schema_revision||context.response_crs?.code!==4326||context.geometry_included!==true)throw Error("Phản hồi sai dự án, nguồn, revision hoặc CRS.");
}
export function validFeatureIdentity(f:SpatialFeature){
 return !!f&&typeof f.feature_id==="string"&&f.feature_id.length>0&&f.feature_id.length<=512&&!!f.attributes&&typeof f.attributes==="object"&&!Array.isArray(f.attributes)&&["present","absent","empty","invalid","unsupported"].includes(f.geometry_state)&&(f.geometry_state!=="present"||f.geometry?.coordinate_reference.code===4326);
}
export function verifyFeaturePage(page:SpatialFeaturePage,layer:SpatialLayer,org:string,project:string,limit:number,seen:ReadonlySet<string>,cursors:ReadonlySet<string>){
 verifyReadContext(page.context,layer,org,project);
 if(!Array.isArray(page.items)||page.items.length>limit)throw Error("Trang dữ liệu vượt giới hạn.");
 const local=new Set<string>();
 for(const f of page.items){if(!validFeatureIdentity(f)||seen.has(f.feature_id)||local.has(f.feature_id))throw Error("FID trùng hoặc dữ liệu không hợp lệ.");local.add(f.feature_id)}
 if(page.next_after_feature_id!==null&&(typeof page.next_after_feature_id!=="string"||!page.next_after_feature_id||!page.items.length||page.next_after_feature_id!==page.items.at(-1)?.feature_id||cursors.has(page.next_after_feature_id)))throw Error("Cursor phân trang không tiến.");
 if(layer.source.feature_count!==null&&seen.size+local.size>layer.source.feature_count)throw Error("Số bản ghi vượt quá nguồn đã pin.");
 return local;
}
export function completeSnapshot(rows:readonly {feature:SpatialFeature;geometry:unknown}[],expected:number|null){
 return typeof expected==="number"&&Number.isSafeInteger(expected)&&expected>=0&&rows.length===expected&&rows.every(r=>validFeatureIdentity(r.feature)&&(r.feature.geometry_state==="present"?r.geometry!==null&&r.geometry!==undefined:r.geometry===null))&&new Set(rows.map(r=>r.feature.feature_id)).size===rows.length;
}
