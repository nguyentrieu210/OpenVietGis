import type {TilesetRecord} from "./TilesetPanel";
import type {TileHit} from "@openvietgis/spatial-workspace/tiles3d";
export type CadLink={document_id:string;version:number;filename:string;source_id:string;layer_id:string;feature_id:string;handle:string;layer_name:string;original_revision:boolean};
export const validLink=(v:CadLink)=>!!v&&typeof v.document_id==="string"&&typeof v.source_id==="string"&&typeof v.layer_id==="string"&&typeof v.feature_id==="string"&&typeof v.filename==="string"&&typeof v.layer_name==="string"&&typeof v.original_revision==="boolean"&&Number.isInteger(v.version)&&v.version>=1&&v.version<=20&&typeof v.handle==="string"&&/^[0-9a-fA-F]{1,32}$/.test(v.handle);
export type SourceFeatureRef={source_id:string;layer_id:string;feature_id:string;layer_revision?:number;content_revision?:string;schema_revision?:string};
export type TileFeatureIdentity={tileset_id:string;uri:string;feature_id:string;first_triangle:number;triangle_count:number;source_id:string;layer_id:string;layer_revision:number;content_revision:string;schema_revision:string};
type Current=TileFeatureIdentity&{state:"current";layer_name:string;attributes:Record<string,unknown>;field_names:Record<string,string>;cad_links:CadLink[]};
export type TileFeature=Current|(TileFeatureIdentity&{state:"stale"});
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==="object"&&!Array.isArray(v);
export function parseTileFeature(raw:unknown,record:TilesetRecord,hit:TileHit):TileFeature{
 const p=record.provenance;
 if(!object(raw)||p?.feature_picking!==true||raw.tileset_id!==record.id||raw.uri!==hit.uri||typeof raw.feature_id!=="string"||!/^[A-Za-z0-9_-]{1,128}$/.test(raw.feature_id)||!Number.isInteger(raw.first_triangle)||!Number.isInteger(raw.triangle_count))throw Error("Phản hồi chỉ mục không khớp.");
 const first=Number(raw.first_triangle),count=Number(raw.triangle_count);
 if(first<0||count<1||first+count>100000||hit.triangle<first||hit.triangle>=first+count)throw Error("Phạm vi tam giác không hợp lệ.");
 if(!Number.isInteger(raw.layer_revision)||raw.layer_revision!==p.layer_revision)throw Error("Revision lớp không khớp.");
 for(const k of ["source_id","layer_id","content_revision","schema_revision"] as const)if(typeof raw[k]!=="string"||raw[k]!==p[k])throw Error("Phiên bản nguồn không khớp.");
 if(raw.state==="stale")return raw as unknown as TileFeature;
 if(raw.state!=="current"||typeof raw.layer_name!=="string"||!object(raw.attributes)||!object(raw.field_names)||Object.values(raw.field_names).some(v=>typeof v!=="string")||!Array.isArray(raw.cad_links)||raw.cad_links.some(v=>!validLink(v)||v.source_id!==raw.source_id||v.feature_id!==raw.feature_id))throw Error("Thuộc tính nguồn không hợp lệ.");
 return raw as unknown as Current;
}
export function parseLocatedFeature(raw:unknown,record:TilesetRecord,source:SourceFeatureRef):TileFeature{
 if(!object(raw)||typeof raw.uri!=="string"||!Number.isInteger(raw.first_triangle)||raw.feature_id!==source.feature_id||raw.source_id!==source.source_id||raw.layer_id!==source.layer_id)throw Error("Đối tượng nguồn không khớp.");
 const v=parseTileFeature(raw,record,{uri:raw.uri,triangle:Number(raw.first_triangle)});
 for(const k of ["layer_revision","content_revision","schema_revision"] as const)if(source[k]!==undefined&&source[k]!==v[k])throw Error("Bộ tile khác phiên bản nguồn.");
 return v;
}
export function TileFeaturePanel({record,feature,message,busy,onNavigate}:{record:TilesetRecord;feature:TileFeature|null;message:string;busy:boolean;onNavigate:(target:"gis"|CadLink)=>void}){
 if(record.provenance?.feature_picking!==true)return <small>Chỉ xem. Tạo lại bộ tile từ GIS để chọn từng công trình.</small>;
 return <section aria-label="Công trình đang chọn"><strong>Đối tượng 3D</strong>{message&&<small role="status">{message}</small>}{!feature&&!message&&<small>Bấm một công trình để xem thuộc tính.</small>}{feature&&<><small>{feature.feature_id}</small>{feature.state==="stale"?<small>Tile cũ: dữ liệu GIS nguồn đã đổi. Tạo lại bộ tile để tra thuộc tính.</small>:<><small>{feature.layer_name} · {feature.content_revision}</small><dl className="ovg-tile-attributes">{Object.entries(feature.attributes).map(([k,v])=><div key={k}><dt>{feature.field_names[k]??k}</dt><dd>{v===null?"—":typeof v==="object"?JSON.stringify(v):String(v)}</dd></div>)}</dl><button disabled={busy} onClick={()=>onNavigate("gis")}>Xem đối tượng GIS</button>{feature.cad_links.map(link=><button disabled={busy} key={link.document_id+":"+link.version+":"+link.handle} onClick={()=>onNavigate(link)}>CAD nguồn · v{link.version} · {link.handle}</button>)}{!feature.cad_links.length&&<small>Không có bản vẽ CAD nguồn.</small>}</>}</>}</section>;
}
