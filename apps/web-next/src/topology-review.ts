import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {Geometry} from "geojson";
export type TopologyPopulation={requested_count:number;scanned_count:number;skipped_count:number;truncated:boolean;complete:boolean;source_id:string;content_revision:string;schema_revision:string;secondary_pin:null|{source_id:string;content_revision:string;schema_revision:string};items:unknown[]};
export function topologyReadable(l:SpatialLayer|undefined|null){
 return !!l&&l.lifecycle==="active"&&l.source.stored_crs?.code===4326&&l.source.validation_state==="ready"&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized);
}
export function sameReadPin(a:SpatialLayer|undefined|null,b:SpatialLayer){
 return topologyReadable(a)&&a!.id===b.id&&a!.source_id===b.source_id&&a!.source.content_revision===b.source.content_revision&&a!.source.schema_revision===b.source.schema_revision;
}
const count=(v:unknown):v is number=>typeof v==="number"&&Number.isSafeInteger(v)&&v>=0;
export function verifyTopology(r:TopologyPopulation,l:SpatialLayer,secondary:SpatialLayer|undefined){
 if(r.source_id!==l.source_id||r.content_revision!==l.source.content_revision||r.schema_revision!==l.source.schema_revision||!count(r.requested_count)||!count(r.scanned_count)||!count(r.skipped_count)||r.requested_count!==l.source.feature_count||r.requested_count!==r.scanned_count+r.skipped_count||typeof r.truncated!=="boolean"||r.complete!==(!r.truncated&&r.skipped_count===0)||!Array.isArray(r.items)||r.items.length>200||r.truncated&&r.items.length!==200)throw Error("Phạm vi kiểm tra topology không khớp.");
 if(secondary?(!r.secondary_pin||r.secondary_pin.source_id!==secondary.source_id||r.secondary_pin.content_revision!==secondary.source.content_revision||r.secondary_pin.schema_revision!==secondary.source.schema_revision):r.secondary_pin!==null)throw Error("Ranh tham chiếu đã thay đổi.");
 return r;
}
export function reviewGeometry(g:Geometry):boolean{
 let n=0;
 const walk=(v:unknown):boolean=>Array.isArray(v)&&v.length>0&&(typeof v[0]==="number"?v.length===2&&v.every(Number.isFinite)&&Math.abs(v[0])<=180&&Math.abs(v[1])<=90&&++n<=100000:v.every(walk));
 return !!g&&["Point","MultiPoint","LineString","MultiLineString","Polygon","MultiPolygon"].includes(g.type)&&"coordinates" in g&&walk(g.coordinates);
}
