// Spec 058: client-side writers for the new export dialog (selected / in-view features). Geometry is WGS 84.
type G={type:string;coordinates?:unknown};
export type Feat={geometry:G|null;properties:Record<string,unknown>};
const ring=(c:number[][])=>c.map(p=>p[0]+" "+p[1]).join(", ");
export function toWkt(g:G|null):string{if(!g)return "";const c=g.coordinates as never;switch(g.type){
 case "Point":return "POINT ("+(c as number[]).slice(0,2).join(" ")+")";
 case "LineString":return "LINESTRING ("+ring(c)+")";
 case "Polygon":return "POLYGON ("+(c as number[][][]).map(r=>"("+ring(r)+")").join(", ")+")";
 case "MultiPoint":return "MULTIPOINT ("+(c as number[][]).map(p=>"("+p[0]+" "+p[1]+")").join(", ")+")";
 case "MultiLineString":return "MULTILINESTRING ("+(c as number[][][]).map(r=>"("+ring(r)+")").join(", ")+")";
 case "MultiPolygon":return "MULTIPOLYGON ("+(c as number[][][][]).map(p=>"("+p.map(r=>"("+ring(r)+")").join(", ")+")").join(", ")+")";
 default:return ""}}
const cell=(v:unknown)=>{const t=v===null||v===undefined?"":typeof v==="object"?JSON.stringify(v):String(v);return /[",\n\r]/.test(t)?'"'+t.replace(/"/g,'""')+'"':t};
export function toCsv(feats:Feat[]):string{const keys=[...new Set(feats.flatMap(f=>Object.keys(f.properties)))];return "﻿"+[[...keys,"WKT"].map(cell).join(","),...feats.map(f=>[...keys.map(k=>cell(f.properties[k])),cell(toWkt(f.geometry))].join(","))].join("\r\n")}
const esc=(t:string)=>t.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;");
const kc=(c:number[][])=>c.map(p=>p[0]+","+p[1]).join(" ");
function kmlGeom(g:G|null):string{if(!g)return "";const c=g.coordinates as never;switch(g.type){
 case "Point":return "<Point><coordinates>"+(c as number[]).slice(0,2).join(",")+"</coordinates></Point>";
 case "LineString":return "<LineString><coordinates>"+kc(c)+"</coordinates></LineString>";
 case "Polygon":return "<Polygon>"+(c as number[][][]).map((r,i)=>(i?"<innerBoundaryIs>":"<outerBoundaryIs>")+"<LinearRing><coordinates>"+kc(r)+"</coordinates></LinearRing>"+(i?"</innerBoundaryIs>":"</outerBoundaryIs>")).join("")+"</Polygon>";
 case "MultiPoint":case "MultiLineString":case "MultiPolygon":{const t=g.type.slice(5);return "<MultiGeometry>"+(c as unknown[]).map(x=>kmlGeom({type:t,coordinates:x})).join("")+"</MultiGeometry>"}
 default:return ""}}
export function toKml(name:string,feats:Feat[],label?:(f:Feat)=>string):string{return '<?xml version="1.0" encoding="UTF-8"?>\n<kml xmlns="http://www.opengis.net/kml/2.2"><Document><name>'+esc(name)+"</name>"+feats.map(f=>"<Placemark><name>"+esc(label?.(f)??"")+"</name><ExtendedData>"+Object.entries(f.properties).map(([k,v])=>'<Data name="'+esc(k)+'"><value>'+esc(v===null||v===undefined?"":String(v))+"</value></Data>").join("")+"</ExtendedData>"+kmlGeom(f.geometry)+"</Placemark>").join("")+"</Document></kml>"}
export const toGeoJson=(feats:Feat[])=>JSON.stringify({type:"FeatureCollection",features:feats.map(f=>({type:"Feature",geometry:f.geometry,properties:f.properties}))});
export const safeName=(s:string)=>s.normalize("NFD").replace(/\p{M}/gu,"").replace(/[đĐ]/g,"d").replace(/[^\w.-]+/g,"-").replace(/^-+|-+$/g,"").slice(0,60)||"du-lieu";
