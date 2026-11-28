import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import {buildQueryDefinition,type FilterRow} from "./workbenches/features/platform/query-filters";
export const TABLE_QUERY_LIMIT=100;
export const TABLE_QUERY_MAX_OFFSET=25000;
export const LOCAL_TABLE_PAGE_SIZE=100;
export function localTablePage<T>(rows:readonly T[],page:number):T[]{
 if(!Number.isInteger(page)||page<0)throw Error("Trang không hợp lệ.");
 const start=page*LOCAL_TABLE_PAGE_SIZE;
 return rows.slice(start,start+LOCAL_TABLE_PAGE_SIZE);
}
export function tableQueryBody(layer:SpatialLayer,row:FilterRow|FilterRow[],sortField:string,direction:"asc"|"desc",offset:number,fingerprint?:string,group:"and"|"or"="and"){
 const fields=layer.source.schema.fields;
 if(!Number.isInteger(offset)||offset<0||offset>TABLE_QUERY_MAX_OFFSET||(offset>0&&!fingerprint))throw Error("Phân trang vượt giới hạn hoặc thiếu phiên bản truy vấn.");
 if(sortField&&!fields.some(f=>f.field_id===sortField))throw Error("Trường sắp xếp không hợp lệ.");
 return {...buildQueryDefinition(Array.isArray(row)?row:[row],fields,group,"intersects"),...(sortField?{sort_field:sortField}:{}),sort_direction:direction,expected_content_revision:layer.source.content_revision,expected_schema_revision:layer.source.schema_revision,limit:TABLE_QUERY_LIMIT,offset,...(fingerprint?{expected_query_fingerprint:fingerprint}:{})};
}
export function verifyTableQuery(layer:SpatialLayer,result:LayerQueryResponse,offset:number,fingerprint?:string){
 if(result.layer_id!==layer.id||result.source_id!==layer.source_id||result.content_revision!==layer.source.content_revision||result.schema_revision!==layer.source.schema_revision||result.offset!==offset||result.limit!==TABLE_QUERY_LIMIT||!result.query_fingerprint||(fingerprint&&result.query_fingerprint!==fingerprint))throw Error("Kết quả không khớp phạm vi hoặc phiên bản. Hãy chạy lại.");
 return result;
}
