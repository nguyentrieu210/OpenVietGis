import type {SpatialFeature,SpatialFeaturePage,SpatialLayer} from "@openvietgis/client-sdk";
import type {Camera} from "./LayoutRender";
export const ATLAS_LIMIT=8;
export const atlasReadable=(l:SpatialLayer)=>l.lifecycle==="active"&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized);
export const atlasLayer=(l:SpatialLayer)=>atlasReadable(l)&&Number.isInteger(l.source.feature_count)&&l.source.feature_count!>0&&l.source.feature_count!<=ATLAS_LIMIT;
export const layerPin=(l:SpatialLayer)=>JSON.stringify([l.id,l.source_id,l.layer_revision,l.source.content_revision,l.source.schema_revision,l.source.feature_count,atlasReadable(l)]);
export function atlasFeatures(page:SpatialFeaturePage,l:SpatialLayer,org:string,pid:string):SpatialFeature[]{
 const c=page.context;
 if(!atlasLayer(l)||c.organization_id!==org||c.project_id!==pid||c.layer_id!==l.id||c.source_id!==l.source_id||c.content_revision!==l.source.content_revision||c.schema_revision!==l.source.schema_revision||c.response_crs?.code!==4326||!c.geometry_included||page.next_after_feature_id!==null||page.items.length!==l.source.feature_count||new Set(page.items.map(f=>f.feature_id)).size!==page.items.length)throw Error("Phạm vi hoặc phiên bản lớp Atlas không khớp.");
 return [...page.items].sort((a,b)=>a.feature_id.localeCompare(b.feature_id,"en"));
}
const merc=(lat:number)=>Math.log(Math.tan(Math.PI/4+lat*Math.PI/360))/(2*Math.PI);
export function atlasCamera(feature:SpatialFeature,w:number,h:number,margin=0.15):Camera{
 if(feature.geometry_state!=="present"||!feature.geometry||feature.geometry.coordinate_reference?.code!==4326||!Number.isFinite(w)||!Number.isFinite(h)||w<=0||h<=0)throw Error("Đối tượng không có hình học hợp lệ.");
 let west=Infinity,east=-Infinity,south=Infinity,north=-Infinity,n=0;
 const walk=(v:unknown)=>{if(!Array.isArray(v))throw Error("Tọa độ không hợp lệ");if(typeof v[0]==="number"){const x=v[0],y=v[1];if(!Number.isFinite(x)||typeof y!=="number"||!Number.isFinite(y)||Math.abs(x)>180||Math.abs(y)>85||++n>200000)throw Error("Hình học Atlas ngoài giới hạn.");west=Math.min(west,x);east=Math.max(east,x);south=Math.min(south,y);north=Math.max(north,y)}else for(const p of v)walk(p)};
 walk(feature.geometry.coordinates);
 if(!n||east-west>180)throw Error("Atlas chưa hỗ trợ đối tượng qua đường đổi ngày.");
 const reference_width=900,reference_height=reference_width*h/w,dx=(east-west)/360,dy=merc(north)-merc(south);
 const span=Math.max(dx,dy*reference_width/reference_height,reference_width/(512*2**18));
 const zoom=Math.min(18,Math.max(0,Math.log2(reference_width/(512*span*(1+2*margin)))));
 const lat=Math.atan(Math.sinh(2*Math.PI*(merc(north)+merc(south))/2))*180/Math.PI;
 return {lng:(west+east)/2,lat,zoom,bearing:0,reference_width};
}
export function atlasText(template:string,f:SpatialFeature,index:number,total:number){
 return template.replace(/\{(fid|page|pages|field:([A-Za-z0-9_-]+))\}/g,(_,key:string,field:string|undefined)=>key==="fid"?f.feature_id:key==="page"?String(index+1):key==="pages"?String(total):field&&Object.hasOwn(f.attributes,field)?String(f.attributes[field]??""):"").slice(0,500);
}
