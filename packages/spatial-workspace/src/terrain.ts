/** Bounded native DEM samples, explicitly declared metre elevations. */
export type TerrainGrid=Readonly<{
 source_id:string;name:string;crs_code:3857;source_crs_code:number;width:number;height:number;
 extent:[number,number,number,number];values:(number|null)[];min_m:number;max_m:number;
 valid_count:number;nodata_count:number;render_origin_m:number;vertical_reference:"local"|"orthometric"|"ellipsoidal";
 reference_label:string;elevation_unit:"metre";sampled:true;survey_grade:false;declared_by_user:true;
}>;
export const WEB_WORLD=20037508.342789244;
export const project3857=(lon:number,lat:number):[number,number]=>[lon*WEB_WORLD/180,WEB_WORLD/Math.PI*Math.log(Math.tan(Math.PI/4+lat*Math.PI/360))];
export const geographic3857=(x:number,y:number):[number,number]=>[x*180/WEB_WORLD,Math.atan(Math.sinh(y*Math.PI/WEB_WORLD))*180/Math.PI];
export function validateTerrain(value:unknown,expectedId:string):TerrainGrid{
 const g=value as TerrainGrid;
 if(!g||g.source_id!==expectedId||typeof g.name!=="string"||g.crs_code!==3857||g.elevation_unit!=="metre"||!g.declared_by_user||g.survey_grade!==false||g.sampled!==true||!["local","orthometric","ellipsoidal"].includes(g.vertical_reference)||typeof g.reference_label!=="string"||!g.reference_label.trim()||!Number.isInteger(g.width)||!Number.isInteger(g.height)||g.width<2||g.height<2||g.width>128||g.height>128||!Array.isArray(g.extent)||g.extent.length!==4||!g.extent.every(v=>Number.isFinite(v)&&Math.abs(v)<=WEB_WORLD)||g.extent[2]<=g.extent[0]||g.extent[3]<=g.extent[1]||g.extent[2]-g.extent[0]>100000||g.extent[3]-g.extent[1]>100000||!Array.isArray(g.values)||g.values.length!==g.width*g.height)throw Error("Dữ liệu DEM không hợp lệ.");
 const valid=g.values.filter((v):v is number=>v!==null);
 if(!valid.length||valid.some(v=>typeof v!=="number"||!Number.isFinite(v)||v< -12000||v>10000)||g.min_m!==Math.min(...valid)||g.max_m!==Math.max(...valid)||g.render_origin_m!==g.min_m||g.valid_count!==valid.length||g.nodata_count!==g.values.length-valid.length)throw Error("Giá trị cao độ DEM không hợp lệ.");
 return g;
}
/** Nearest DEM vertex is not enough: require all four corners and interpolate using mesh triangles. */
export function sampleTerrain(g:TerrainGrid,longitude:number,latitude:number):number|null{
 if(!Number.isFinite(longitude)||!Number.isFinite(latitude)||Math.abs(longitude)>180||Math.abs(latitude)>85.05112878)return null;
 const [x,y]=project3857(longitude,latitude),[l,b,r,t]=g.extent;
 const u=(x-l)/(r-l)*(g.width-1),v=(t-y)/(t-b)*(g.height-1);
 if(u< -1e-7||u>g.width-1+1e-7||v< -1e-7||v>g.height-1+1e-7)return null;
 const col=Math.max(0,Math.min(g.width-2,Math.floor(u))),row=Math.max(0,Math.min(g.height-2,Math.floor(v))),a=Math.max(0,Math.min(1,u-col)),d=Math.max(0,Math.min(1,v-row));
 const i=row*g.width+col,q=g.values[i],p=g.values[i+1],s=g.values[i+g.width],z=g.values[i+g.width+1];
 if(q==null||p==null||s==null||z==null)return null;
 // Same NW-SE diagonal as terrain mesh.
 return a>=d?q+(p-q)*a+(z-p)*d:q+(s-q)*d+(z-s)*a;
}
export function terrainTriangles(g:TerrainGrid):number[]{
 const indices:number[]=[];for(let y=0;y<g.height-1;y++)for(let x=0;x<g.width-1;x++){const a=y*g.width+x,b=a+1,c=a+g.width,d=c+1;if([a,b,c,d].every(i=>g.values[i]!=null))indices.push(a,b,d,a,d,c)}
 if(!indices.length)throw Error("Không có ô DEM đầy đủ.");return indices;
}
