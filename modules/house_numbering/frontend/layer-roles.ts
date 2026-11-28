import type {ModuleLayerBindings,SpatialLayer} from "@openvietgis/client-sdk";
export const ROLES={survey_points:"Điểm khảo sát",commune_boundary:"Ranh xã",hamlet_boundary:"Ranh ấp",boundary:"Ranh giới khác",parcel:"Thửa đất",building:"Mặt bằng công trình",road:"Tuyến đường / hẻm"} as const;
export type LayerRole=keyof typeof ROLES;
export function legacyRole(name:string):LayerRole|undefined{
 if(name.startsWith("Khảo sát số nhà"))return "survey_points";
 if(/^(Ranh giới xã|Ranh xã)/.test(name))return "commune_boundary";
 if(/^(Ranh giới ấp|Ấp hiện hành)/.test(name))return "hamlet_boundary";
 if(name.includes("Ranh giới"))return "boundary";
 if(name.includes("Thửa"))return "parcel";
 if(name.includes("Mặt bằng"))return "building";
 return undefined;
}
export const isRole=(value:string):value is LayerRole=>Object.prototype.hasOwnProperty.call(ROLES,value);
export function validRoleForLayer(role:LayerRole,layer:SpatialLayer):boolean{
 const geometry=layer.source.schema?.geometry_family;
 if(role==="survey_points")return geometry==="Point"&&["ap","ten_duong"].every(id=>layer.source.schema?.fields?.some(f=>f.field_id===id));
 if(role==="road")return geometry==="LineString";
 if(role==="building")return geometry==="Point"||geometry==="Polygon";
 return geometry==="Polygon";
}
export function roleResolver(bindings:ModuleLayerBindings|null,layers:SpatialLayer[]){
 const map=new Map<string,LayerRole>();
 if(bindings===null)return (_:SpatialLayer)=>undefined;
 if(!bindings.configured){for(const layer of layers){const role=legacyRole(layer.name);if(role)map.set(layer.source_id,role);}}
 else for(const item of bindings.items){const layer=layers.find(l=>l.source_id===item.source_id&&l.lifecycle==="active");if(layer&&item.status==="ready"&&item.schema_revision===layer.source.schema_revision&&isRole(item.role)&&validRoleForLayer(item.role,layer))map.set(item.source_id,item.role);}
 return (layer:SpatialLayer)=>map.get(layer.source_id);
}
