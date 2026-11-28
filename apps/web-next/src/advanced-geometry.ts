import type {Geometry} from "geojson";

export type XY=[number,number];

export function vertexEditable(g?:Geometry|null):g is Extract<Geometry,{type:"LineString"|"MultiLineString"|"Polygon"|"MultiPolygon"}>{
 return !!g&&(g.type==="LineString"||g.type==="MultiLineString"||g.type==="Polygon"||g.type==="MultiPolygon");
}

export function segmentEditable(g?:Geometry|null):g is Extract<Geometry,{type:"LineString"|"Polygon"}>{
 return !!g&&(g.type==="LineString"||g.type==="Polygon");
}

export function geometryPartCount(g:Geometry):number{
 return g.type==="MultiLineString"?g.coordinates.length:g.type==="MultiPolygon"?g.coordinates.length:1;
}

export function geometryRingCount(g:Geometry,part=0):number{
 return g.type==="Polygon"?g.coordinates.length:g.type==="MultiPolygon"?g.coordinates[part]?.length??0:1;
}

export function geometryVertices(g:Geometry,part=0,ring=0):XY[]{
 if(g.type==="LineString")return g.coordinates as XY[];
 if(g.type==="MultiLineString")return (g.coordinates[part]??[]) as XY[];
 if(g.type==="Polygon")return (g.coordinates[ring]??[]).slice(0,-1) as XY[];
 if(g.type==="MultiPolygon")return (g.coordinates[part]?.[ring]??[]).slice(0,-1) as XY[];
 return [];
}

export function withGeometryVertices(g:Geometry,part:number,ring:number,points:XY[]):Geometry{
 if(!points.length)return g;
 if(g.type==="LineString")return {...g,coordinates:points};
 if(g.type==="MultiLineString")return {...g,coordinates:g.coordinates.map((line,index)=>index===part?points:line)};
 if(g.type==="Polygon")return {...g,coordinates:g.coordinates.map((r,index)=>index===ring?[...points,points[0]!]:r)};
 if(g.type==="MultiPolygon")return {...g,coordinates:g.coordinates.map((polygon,index)=>index===part?polygon.map((r,j)=>j===ring?[...points,points[0]!]:r):polygon)};
 return g;
}
