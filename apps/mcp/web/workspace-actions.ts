import {ApiClient,getSpatialLayer,listSpatialLayers,readSpatialFeatures,getSpatialFeature,commitChangeset,revertChangeset,loadCsrfToken,getEffectivePermissions,type SpatialLayer} from "@openvietgis/client-sdk";
import type {Geometry} from "geojson";
type Params=Record<string,unknown>;
type Pin={layer_id:string;content_revision:string;schema_revision:string;feature_ids?:string[]};
type Overlay={title:string;geojson:{type:"FeatureCollection";features:{type:"Feature";geometry:Geometry;properties:Record<string,never>}[]}};
type Host={client:ApiClient;org:string;pid:string;guard:()=>void;refresh:()=>void;visibility:(id:string,visible:boolean)=>void;select:(layer:string,ids:string[])=>void;overlay:(data:Overlay)=>void};
const mutations=new Set(["gis_changeset","gis_revert","gis_create_layer","gis_schema"]);
const allowed=new Set([...mutations,"gis_layers","gis_features","gis_feature","gis_query","gis_geometry_preview","gis_topology","gis_overlay","gis_layer_visibility","gis_select","gis_export"]);
const uuid=(v:unknown)=>{if(typeof v!=="string"||!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v))throw Error("ID không hợp lệ");return v};
const fid=(v:unknown)=>{if(typeof v!=="string"||!/^[A-Za-z0-9_-]{1,128}$/.test(v))throw Error("FID không hợp lệ");return v};
function output(v:unknown){if(!v||typeof v!=="object"||Array.isArray(v)||new TextEncoder().encode(JSON.stringify(v)).length>80000)throw Error("Kết quả quá lớn; giảm giới hạn truy vấn");return v}
export async function executeWorkspace(h:Host,method:string,p:Params){
 if(!allowed.has(method))throw Error("Lệnh AI không hỗ trợ");
 h.guard();
 const base="/organizations/"+encodeURIComponent(h.org)+"/projects/"+encodeURIComponent(h.pid);
 const permissions=await getEffectivePermissions(h.client,h.org,{project_id:h.pid});
 if(!permissions.permissions.includes("project.read")||mutations.has(method)&&!permissions.permissions.includes("project.edit"))throw Error("Không có quyền thao tác");
 const access=await h.client.request<{restricted:boolean}>(base+"/access/mine");
 if(access.restricted!==false)throw Error("Không hỗ trợ tài khoản GIS giới hạn");
 async function layer(id:unknown){const l=await getSpatialLayer(h.client,h.org,h.pid,uuid(id));if(l.lifecycle!=="active")throw Error("Lớp không hoạt động");return l}
 async function pinned(raw:unknown,edit=false):Promise<SpatialLayer>{
   if(!raw||typeof raw!=="object")throw Error("Thiếu revision pin");
   const pin=raw as Pin,l=await layer(pin.layer_id);
   if(l.source.content_revision!==pin.content_revision||l.source.schema_revision!==pin.schema_revision)throw Error("Phiên bản đã thay đổi, đọc lại lớp");
   if(!l.capabilities.some(c=>c.name===(edit?"edit":"feature_read")&&c.available&&c.actor_authorized))throw Error("Không được cấp quyền trên lớp");
   return l;
 }
 const post=async(path:string,body:unknown)=>{await loadCsrfToken(h.client);h.guard();return h.client.request<Record<string,unknown>>(base+path,{method:"POST",body:JSON.stringify(body)})};
 let result:unknown;
 switch(method){
 case "gis_layers":{
   const v=await listSpatialLayers(h.client,h.org,h.pid,{limit:Number(p.limit),...(p.after_id?{after_id:uuid(p.after_id)}:{})});
   result={items:v.items.filter(l=>l.lifecycle==="active").map(l=>({id:l.id,name:l.name,source_id:l.source_id,layer_revision:l.layer_revision,source:{content_revision:l.source.content_revision,schema_revision:l.source.schema_revision,geometry_family:l.source.schema?.geometry_family,stored_crs:l.source.stored_crs,feature_count:l.source.feature_count,schema:l.source.schema},capabilities:l.capabilities})),next_after_id:v.next_after_id};break;
 }
 case "gis_features":case "gis_feature":{
   const l=await pinned(p),opts={content_revision:l.source.content_revision,schema_revision:l.source.schema_revision,include_geometry:p.include_geometry!==false,response_crs:4326};
   result=method==="gis_feature"?await getSpatialFeature(h.client,h.org,h.pid,l.id,fid(p.feature_id),opts):await readSpatialFeatures(h.client,h.org,h.pid,l.id,{...opts,limit:Number(p.limit),...(p.after_feature_id?{after_feature_id:fid(p.after_feature_id)}:{})});break;
 }
 case "gis_query":{const l=await pinned(p);result=await post("/layers/"+l.id+"/query",{attribute_filter:p.attribute_filter,spatial_filter:p.spatial_filter,sort_field:p.sort_field,sort_direction:p.sort_direction,limit:p.limit,offset:p.offset,expected_content_revision:l.source.content_revision,expected_schema_revision:l.source.schema_revision});break}
 case "gis_geometry_preview":case "gis_topology": {
   const l=await pinned(p.primary);if(p.secondary)await pinned(p.secondary);
   const r=await post(method==="gis_topology"?"/pro/topology":"/pro/geometry",p);
   if(r.source_id!==l.source_id||r.content_revision!==l.source.content_revision)throw Error("Kết quả phân tích không khớp pin");
   result=r;break;
 }
 case "gis_overlay":{
   if(!Array.isArray(p.geometries)||!p.geometries.length)throw Error("Thiếu hình học");
   h.guard();h.overlay({title:String(p.title),geojson:{type:"FeatureCollection",features:p.geometries.map(g=>({type:"Feature",geometry:g as Geometry,properties:{}}))}});
   result={displayed:p.geometries.length,persistent:false};break;
 }
 case "gis_layer_visibility":{const l=await layer(p.layer_id);h.guard();h.visibility(l.id,p.visible===true);result={layer_id:l.id,visible:p.visible===true};break}
 case "gis_select":{const l=await pinned(p);if(!Array.isArray(p.feature_ids))throw Error("Thiếu FID");h.guard();h.select(l.id,p.feature_ids.map(fid));result={layer_id:l.id,selected:p.feature_ids.length};break}
 case "gis_changeset":{
   const l=await pinned(p,true);if(l.source.stored_crs?.code!==4326)throw Error("Chỉ sửa nguồn native EPSG:4326");
   const csrf=await loadCsrfToken(h.client);h.guard();
   const op=uuid(p.operation_id);
   const receipt=await commitChangeset(h.client,h.org,h.pid,l.source_id,{base_content_revision:l.source.content_revision,base_schema_revision:l.source.schema_revision,operation_id:op,summary:String(p.summary),operations:p.operations as Parameters<typeof commitChangeset>[4]["operations"]},csrf);
   if(receipt.operation_id!==op||receipt.source_id!==l.source_id||receipt.base_content_revision!==l.source.content_revision)throw Error("Biên nhận không khớp, kiểm tra lịch sử trước khi gửi lại");
   h.refresh();result={id:receipt.id,operation_id:op,source_id:l.source_id,base_content_revision:receipt.base_content_revision};break;
 }
 case "gis_revert":{
   const csrf=await loadCsrfToken(h.client);h.guard();
   const op=uuid(p.operation_id),receipt=await revertChangeset(h.client,h.org,h.pid,uuid(p.changeset_id),op,csrf,String(p.summary));
   if(receipt.operation_id!==op)throw Error("Biên nhận không khớp");h.refresh();result={id:receipt.id,operation_id:op,source_id:receipt.source_id};break;
 }
 case "gis_create_layer":{
   const r=await post("/layers/manual",p);if(r.operation_id!==p.operation_id)throw Error("Biên nhận không khớp");
   h.refresh();result={receipt:r,layer:await layer(r.layer_id)};break;
 }
 case "gis_schema":{await pinned(p.primary,true);result=await post("/pro/schema",p);h.refresh();break}
 case "gis_export":{
   const l=await pinned(p);
   const response=await h.client.requestRaw(base+"/access/export/"+l.id+"?format=geojson");
   const blob=await response.blob();await pinned(p);h.guard();
   const url=URL.createObjectURL(blob),a=document.createElement("a");a.href=url;a.download="features.geojson";a.click();setTimeout(()=>URL.revokeObjectURL(url),2000);
   result={download:"features.geojson",bytes:blob.size,layer_id:l.id};break;
 }
 default:throw Error("Lệnh AI không hỗ trợ");
 }
 return output(result);
}
