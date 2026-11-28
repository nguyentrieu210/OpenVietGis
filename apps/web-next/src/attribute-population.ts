import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import {tableQueryBody,verifyTableQuery,TABLE_QUERY_LIMIT} from "./table-query";
import type {AttributeRecord} from "./precise-calculation";
export async function loadAttributePopulation(layer:SpatialLayer,limit:number,execute:(body:Record<string,unknown>)=>Promise<LayerQueryResponse>,current:()=>boolean,onProgress:(n:number)=>void){
 const count=layer.source.feature_count;
 if(count===null||!Number.isSafeInteger(count)||count<0||count>limit)throw Error("Nguồn vượt giới hạn "+limit+" bản ghi hoặc chưa có số lượng xác minh.");
 const records:AttributeRecord[]=[],ids=new Set<string>();let fp:string|undefined;
 const ensure=()=>{if(!current())throw Error("Nguồn hoặc quyền truy cập đã thay đổi.")};
 for(let offset=0;;offset+=TABLE_QUERY_LIMIT){
  ensure();const body=tableQueryBody(layer,{field:"",op:"eq",value:"",negate:false},"","asc",offset,fp);
  const r=verifyTableQuery(layer,await execute(body),offset,fp);ensure();
  if(r.total_count!==count)throw Error("Số bản ghi không khớp revision.");
  fp=r.query_fingerprint;
  for(const row of r.items){if(ids.has(row.feature_id))throw Error("Trùng FID giữa các trang.");ids.add(row.feature_id);records.push({feature_id:row.feature_id,attributes:row.attributes})}
  onProgress(records.length);if(!r.has_more)break;
 }
 if(records.length!==count)throw Error("Chưa nạp đầy đủ nguồn.");
 return records;
}
