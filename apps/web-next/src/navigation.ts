export type Point = [number, number];
export type Place = {label:string;point:Point;extent?:number[]|null;osm_id?:number;osm_type?:string;source:string};
export type Places = {items:Place[];crs:string;attribution:string};
export type Maneuver = {instruction:string;length:number;time:number};
export type RouteResult = {source?:string;route:{trip:{summary:{length:number;time:number};legs:{shape:string;maneuvers:Maneuver[]}[]}};attribution:string};
export function validPoint(p:Point){return p.every(Number.isFinite)&&Math.abs(p[0])<=180&&Math.abs(p[1])<=85.05112878}
export function parsePoint(text:string):Point|null{
 const m=/^\s*([+-]?(?:\d+(?:\.\d+)?|\.\d+))\s*[,;]\s*([+-]?(?:\d+(?:\.\d+)?|\.\d+))\s*$/.exec(text);
 if(!m)return null;const p:Point=[Number(m[1]),Number(m[2])];return validPoint(p)?p:null;
}
export function decodeRoute(shape:string):Point[]{
 let index=0,lat=0,lon=0;const points:Point[]=[];
 const read=()=>{let result=0,shift=0,b:number;do{if(index>=shape.length||shift>30)throw Error("Hình học tuyến không hợp lệ");b=shape.charCodeAt(index++)-63;if(b<0||b>63)throw Error("Hình học tuyến không hợp lệ");result|=(b&31)<<shift;shift+=5}while(b>=32);return result&1?~(result>>>1):result>>>1};
 while(index<shape.length){lat+=read();lon+=read();const p:Point=[lon/1e6,lat/1e6];if(!validPoint(p))throw Error("Tọa độ tuyến không hợp lệ");points.push(p)}
 return points;
}
