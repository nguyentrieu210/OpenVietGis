import {ApiError,getChangeset,getSpatialFeature,listChangesets,type ApiClient,type SpatialLayer} from "@openvietgis/client-sdk";
import {featureKey,type StudioRow} from "./studio-contract";

import {revisionChain,parallelBounded} from "./SpatialSyncCore";
export {parallelBounded} from "./SpatialSyncCore";

export async function incrementalLayerRows(client:ApiClient,org:string,pid:string,previous:SpatialLayer,layer:SpatialLayer,rows:StudioRow[],color:string,active:()=>boolean):Promise<StudioRow[]|null>{
  if(!layer.source_id||previous.source_id!==layer.source_id||previous.source.schema_revision!==layer.source.schema_revision||previous.layer_revision!==layer.layer_revision)return null;
  // A partial snapshot cannot safely be patched as if it were complete.
  if(typeof previous.source.feature_count!=="number"||rows.length!==previous.source.feature_count)return null;
  const history=await listChangesets(client,org,pid,layer.source_id);
  const chain=revisionChain(history.items,previous.source.content_revision,layer.source.content_revision);
  if(!chain||!active())return null;
  const changes=new Map<string,string>();
  for(const c of chain){
    const detail=await getChangeset(client,org,pid,c.id);
    if(!active()||detail.source_id!==layer.source_id||detail.operations.length!==c.operation_count)return null;
    for(const op of detail.operations)changes.set(op.feature_id,op.op_type);
  }
  const replacements=new Map<string,StudioRow>();
  await parallelBounded([...changes],4,async([id,op])=>{
    if(op==="delete"||!active())return;
    const detail=await getSpatialFeature(client,org,pid,layer.id,id,{content_revision:layer.source.content_revision,schema_revision:layer.source.schema_revision,include_geometry:true,response_crs:4326});
    const c=detail.context,f=detail.feature;
    if(c.source_id!==layer.source_id||c.project_id!==pid||c.layer_id!==layer.id||c.content_revision!==layer.source.content_revision||f.feature_id!==id)throw Error("Phiên bản thay đổi khi nhận delta.");
    const g=f.geometry_state==="present"&&f.geometry?.coordinate_reference.code===4326?{type:f.geometry.type,coordinates:f.geometry.coordinates} as StudioRow["geometry"]:null;
    replacements.set(id,{key:featureKey(layer.source_id,id),layer,feature:f,geometry:g,color});
  });
  if(!active())return null;
  const updated=rows.filter(r=>!changes.has(r.feature.feature_id)).map(r=>({...r,layer}));
  updated.push(...replacements.values());
  if(typeof layer.source.feature_count==="number"&&updated.length!==layer.source.feature_count)return null;
  return updated;
}

export function isPermissionError(e:unknown){return e instanceof ApiError&&(e.status===401||e.status===403||e.status===404);}
