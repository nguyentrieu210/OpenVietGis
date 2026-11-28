import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import {buildQueryDefinition,type FilterRow} from "./workbenches/features/platform/query-filters";
export const TABLE_QUERY_LIMIT=100;
export const TABLE_QUERY_MAX_OFFSET=50000;
export function tableQueryBody(layer:SpatialLayer,row:FilterRow|FilterRow[],sortField:string,direction:"asc"|"desc",offset:number,fingerprint?:string,group:"and"|"or"="and"){
 const fields=layer.source.schema.fields;
 if(!Number.isInteger(offset)||offset<0||offset>TABLE_QUERY_MAX_OFFSET||(offset>0&&!fingerprint)||!(direction==="asc"||direction==="desc")||(fingerprint!==undefined&&(typeof fingerprint!=="string"||!fingerprint||fingerprint.length>512)))throw Error("Phân trang vượt giới hạn hoặc thiếu phiên bản truy vấn.");
 if(sortField&&!fields.some(f=>f.field_id===sortField))throw Error("Trường sắp xếp không hợp lệ.");
 return {attribute_only:true,...buildQueryDefinition(Array.isArray(row)?row:[row],fields,group,"intersects"),...(sortField?{sort_field:sortField}:{}),sort_direction:direction,expected_content_revision:layer.source.content_revision,expected_schema_revision:layer.source.schema_revision,limit:TABLE_QUERY_LIMIT,offset,...(fingerprint?{expected_query_fingerprint:fingerprint}:{})};
}
export function verifyTableQuery(layer:SpatialLayer,result:LayerQueryResponse,offset:number,fingerprint?:string){
 if(!result||result.attribute_only!==true||result.layer_id!==layer.id||result.source_id!==layer.source_id||result.content_revision!==layer.source.content_revision||result.schema_revision!==layer.source.schema_revision||result.offset!==offset||result.limit!==TABLE_QUERY_LIMIT||!Number.isInteger(result.total_count)||result.total_count<0||result.total_count>50000||!Array.isArray(result.items)||result.items.length>TABLE_QUERY_LIMIT||result.items.length!==Math.min(TABLE_QUERY_LIMIT,Math.max(0,result.total_count-offset))||result.has_more!==(offset+result.items.length<result.total_count)||result.items.some(x=>!x||typeof x.feature_id!=="string"||!x.feature_id||x.feature_id.length>512||!x.attributes||typeof x.attributes!=="object"||Array.isArray(x.attributes)||x.geometry!==null||!["present","absent","empty","invalid","unsupported"].includes(x.geometry_state??""))||new Set(result.items.map(x=>x.feature_id)).size!==result.items.length||typeof result.query_fingerprint!=="string"||!result.query_fingerprint||result.query_fingerprint.length>512||(fingerprint&&result.query_fingerprint!==fingerprint))throw Error("Kết quả không khớp phạm vi hoặc phiên bản. Hãy chạy lại.");
 return result;
}
