import type {Changeset,SpatialLayer,commitChangeset} from "@openvietgis/client-sdk";
export type EditBody=Parameters<typeof commitChangeset>[4];
export function sameEditPin(current:SpatialLayer|undefined,base:SpatialLayer){
 return !!current&&current.id===base.id&&current.lifecycle==="active"&&current.source_id===base.source_id&&current.source.content_revision===base.source.content_revision&&current.source.schema_revision===base.source.schema_revision&&current.capabilities.some(c=>c.name==="edit"&&c.available&&c.actor_authorized);
}
export function verifyEditReceipt(result:Changeset,source:string,body:EditBody){
 if(!result.id||result.source_id!==source||result.operation_id!==body.operation_id||result.base_content_revision!==body.base_content_revision||result.status!=="committed"||result.operation_count!==body.operations.length||!result.resulting_content_revision)throw Error("Biên nhận không khớp; giữ nguyên yêu cầu để xác minh.");
 return result;
}
export function definitiveFirstRejection(status:number,alreadyUncertain:boolean){return !alreadyUncertain&&[400,403,404,409,422].includes(status)}
