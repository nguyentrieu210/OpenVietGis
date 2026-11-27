import type {Feature,FeatureCollection,Geometry,Position} from "geojson";
export function validPosition(p:unknown):p is Position {
 return Array.isArray(p)&&p.length>=2&&p.every(x=>typeof x==="number"&&Number.isFinite(x))&&Math.abs(p[0])<=180&&Math.abs(p[1])<=90;
}
const positions=(p:unknown,min:number)=>Array.isArray(p)&&p.length>=min&&p.every(validPosition);
const ring=(p:unknown)=>positions(p,4)&&Array.isArray(p)&&p[0][0]===p[p.length-1][0]&&p[0][1]===p[p.length-1][1];
function validGeometry(g:unknown):g is Geometry {
 if(!g||typeof g!=="object")return false;
 const {type,coordinates:c}=g as {type:string;coordinates:unknown};
 switch(type){
 case "Point":return validPosition(c);
 case "MultiPoint":return positions(c,1);
 case "LineString":return positions(c,2);
 case "MultiLineString":return Array.isArray(c)&&c.length>0&&c.every(v=>positions(v,2));
 case "Polygon":return Array.isArray(c)&&c.length>0&&c.every(ring);
 case "MultiPolygon":return Array.isArray(c)&&c.length>0&&c.every(v=>Array.isArray(v)&&v.length>0&&v.every(ring));
 default:return false;
 }
}
export function parseGuestGeoJson(input:unknown):FeatureCollection {
 if(!input||typeof input!=="object")throw Error("GeoJSON không hợp lệ.");
 const x=input as {type:string;features?:unknown[]};
 const fs=x.type==="FeatureCollection"?x.features:x.type==="Feature"?[x]:null;
 if(!Array.isArray(fs)||fs.length>10000||!fs.every(f=>f&&typeof f==="object"&&"type" in f&&f.type==="Feature"&&"geometry" in f&&validGeometry(f.geometry)&&"properties" in f&&(f.properties===null||(typeof f.properties==="object"&&!Array.isArray(f.properties)))))throw Error("Chỉ hỗ trợ GeoJSON WGS84 hợp lệ, tối đa 10.000 đối tượng.");
 return {type:"FeatureCollection",features:fs as Feature[]};
}
