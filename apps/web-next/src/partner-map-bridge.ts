import type {Geometry,FeatureCollection} from "geojson";

export type PartnerMapMethod = "map_view" | "map_navigate" | "map_selection" | "map_overlay" | "tool_register";
export type PartnerOverlay = {title:string;geojson:FeatureCollection};
export type PartnerToolInput = {id:string;label:string};
export type PartnerTool = PartnerToolInput & {run:()=>void};
export type NavigateTarget = {longitude:number; latitude:number; zoom:number};
export type PartnerMapBridge = {
  view:()=>unknown;
  selection:()=>unknown;
  navigate:(target:NavigateTarget)=>void;
  overlay:(data:PartnerOverlay)=>void;
  clearOverlay:()=>void;
};
export function navigateTarget(value:unknown):NavigateTarget|null {
  if (!value || typeof value!=="object") return null;
  const {longitude,latitude,zoom}=value as Record<string,unknown>;
  if (typeof longitude!=="number"||!Number.isFinite(longitude)||Math.abs(longitude)>180
      ||typeof latitude!=="number"||!Number.isFinite(latitude)||Math.abs(latitude)>85
      ||typeof zoom!=="number"||!Number.isFinite(zoom)||zoom<0||zoom>22) return null;
  return {longitude,latitude,zoom};
}
const depthByType:Record<string,number>={Point:0,MultiPoint:1,LineString:1,MultiLineString:2,Polygon:2,MultiPolygon:3};
/** Only a bounded 2D WGS84 geometry. No dataset IDs, attributes or original object references. */
export function shareGeometry(geometry:Geometry|null):unknown {
  if (!geometry || !("coordinates" in geometry)) throw Error("Không có hình học được hỗ trợ.");
  const depth=depthByType[geometry.type];
  if (!Object.hasOwn(depthByType,geometry.type)||typeof depth!=="number") throw Error("Không hỗ trợ loại hình học.");
  let count=0,nodes=0;
  const walk=(value:unknown,d:number):unknown=>{
    if (++nodes>4096||!Array.isArray(value)||!value.length||value.length>2048) throw Error("Hình học quá lớn hoặc không hợp lệ.");
    if(d===0){
      if (++count>2048||value.length<2||typeof value[0]!=="number"||typeof value[1]!=="number"||
          !Number.isFinite(value[0])||!Number.isFinite(value[1])||Math.abs(value[0])>180||Math.abs(value[1])>90) throw Error("Tọa độ không hợp lệ.");
      return [value[0],value[1]];
    }
    return value.map(x=>walk(x,d-1));
  };
  return {type:geometry.type,coordinates:walk(geometry.coordinates,depth)};
}
const cleanLabel=(value:unknown,max:number)=>typeof value==="string"&&value.trim()&&value.length<=max&&!/[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069]/.test(value)?value.trim():null;
export function partnerTool(value:unknown):PartnerToolInput|null{
  if(!value||typeof value!=="object")return null;
  const {id,label}=value as Record<string,unknown>,name=cleanLabel(label,40);
  return typeof id==="string"&&/^[a-z][a-z0-9_]{0,31}$/.test(id)&&name?{id,label:name}:null;
}
export function partnerOverlay(value:unknown):PartnerOverlay|null{
  if(!value||typeof value!=="object")return null;
  const {title,geojson}=value as Record<string,unknown>,name=cleanLabel(title,60);
  if(!name||!geojson||typeof geojson!=="object")return null;
  const fc=geojson as Record<string,unknown>;
  if(fc.type!=="FeatureCollection"||!Array.isArray(fc.features)||!fc.features.length||fc.features.length>100)return null;
  let positions=0;
  const visit=(v:unknown)=>{if(!Array.isArray(v))throw Error("Invalid geometry");if(typeof v[0]==="number"){if(++positions>4096)throw Error("Too many positions")}else v.forEach(visit)};
  const line=(v:unknown)=>Array.isArray(v)&&v.length>=2;
  const ring=(v:unknown)=>Array.isArray(v)&&v.length>=4&&JSON.stringify(v[0])===JSON.stringify(v[v.length-1]);
  const polygon=(v:unknown)=>Array.isArray(v)&&v.length>0&&v.every(ring);
  try{
    const features=fc.features.map(f=>{
      if(!f||f.type!=="Feature"||!f.geometry)throw Error("Invalid feature");
      const geometry=shareGeometry(f.geometry as Geometry) as Geometry;
      if(!("coordinates" in geometry))throw Error("Invalid geometry");
      const {type,coordinates}=geometry;
      if(type==="LineString"&&!line(coordinates)||type==="MultiLineString"&&!coordinates.every(line)||type==="Polygon"&&!polygon(coordinates)||type==="MultiPolygon"&&!coordinates.every(polygon))throw Error("Invalid geometry");
      visit(coordinates);
      return {type:"Feature" as const,geometry,properties:{}};
    });
    return {title:name,geojson:{type:"FeatureCollection",features}};
  }catch{return null}
}
