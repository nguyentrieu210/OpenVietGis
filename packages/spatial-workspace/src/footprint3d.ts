import type {Feature,FeatureCollection,Polygon,MultiPolygon} from "geojson";
export type Footprint=Readonly<{id:string;geometry:Polygon|MultiPolygon;baseMetres:number;heightMetres:number}>;
export type FootprintView=FeatureCollection<Polygon|MultiPolygon,{featureId:string;base:number;top:number}>;
export function footprint3d(features:readonly Footprint[],crs:"EPSG:4326",heightReference:"relative-flat-reference"):FootprintView{
 if(crs!=="EPSG:4326"||heightReference!=="relative-flat-reference")throw Error("Explicit horizontal and height reference required");
 if(features.length>5000)throw Error("3D footprint limit exceeded");
 const ids=new Set<string>();let positions=0;
 const out:FootprintView={type:"FeatureCollection",features:[]};
 for(const f of features){
  if(!f.id||ids.has(f.id)||!Number.isFinite(f.baseMetres)||!Number.isFinite(f.heightMetres)||f.baseMetres<0||f.heightMetres<=0||f.baseMetres+f.heightMetres>10000)throw Error("Invalid footprint identity or explicit height");
  ids.add(f.id);
  if(f.geometry.type!=="Polygon"&&f.geometry.type!=="MultiPolygon")throw Error("Expected polygon geometry");
  const polys=f.geometry.type==="Polygon"?[f.geometry.coordinates]:f.geometry.coordinates;
  if(!polys.length)throw Error("Empty polygon");
  for(const polygon of polys){if(!polygon.length)throw Error("Empty polygon");for(const ring of polygon){
   if(ring.length<4||ring[0][0]!==ring.at(-1)?.[0]||ring[0][1]!==ring.at(-1)?.[1])throw Error("Unclosed polygon ring");
   for(const point of ring){if(++positions>200000||point.length!==2||!point.every(Number.isFinite)||Math.abs(point[0])>180||Math.abs(point[1])>85)throw Error("Invalid WGS84 position or geometry limit")}
  }}
  const geometry=structuredClone(f.geometry);
  const feature:Feature<Polygon|MultiPolygon,{featureId:string;base:number;top:number}>={type:"Feature",geometry,properties:{featureId:f.id,base:f.baseMetres,top:f.baseMetres+f.heightMetres}};
  out.features.push(feature);
 }
 return out;
}
