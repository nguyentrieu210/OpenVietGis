import type {GeographicCamera} from "@openvietgis/spatial-workspace/maplibre3d";
type DocumentIdentity={id:string;version:number;sha256:string};
export type CadResume={kind:"vault"}|({kind:"document"}&DocumentIdentity);
const valid=(v:unknown):v is DocumentIdentity=>{const x=v as DocumentIdentity;return !!x&&typeof x.id==="string"&&/^[A-Za-z0-9_-]{1,128}$/.test(x.id)&&Number.isSafeInteger(x.version)&&x.version>=1&&x.version<=20&&typeof x.sha256==="string"&&/^[a-f0-9]{64}$/.test(x.sha256)};
export function parseCadResume(raw:string|null):CadResume|null{
 try{const v=JSON.parse(raw??"null");if(v?.kind==="vault")return {kind:"vault"};if(v?.kind==="document"&&valid(v))return {kind:"document",id:v.id,version:v.version,sha256:v.sha256};return null}catch{return null}
}
export function cadResumeDocument<T extends DocumentIdentity>(pin:CadResume|null,docs:readonly T[]):T|null{
 if(pin?.kind!=="document")return null;
 const matches=docs.filter(d=>d.id===pin.id&&d.version===pin.version&&d.sha256===pin.sha256);
 return matches.length===1?matches[0]!:null;
}
export function makeCadResume(doc:DocumentIdentity):CadResume|null{return valid(doc)?{kind:"document",id:doc.id,version:doc.version,sha256:doc.sha256}:null}
export function validGlobeCamera(raw:unknown):GeographicCamera|null{
 const c=raw as GeographicCamera;
 const range=(v:unknown,min:number,max:number):v is number=>typeof v==="number"&&Number.isFinite(v)&&v>=min&&v<=max;
 if(!c||!range(c.longitude,-180,180)||!range(c.latitude,-85.05112878,85.05112878)||!range(c.zoom,0,22)||!range(c.bearing,-360,360)||!range(c.pitch??0,0,89))return null;
 return {longitude:c.longitude,latitude:c.latitude,zoom:c.zoom,bearing:((c.bearing+180)%360+360)%360-180,pitch:c.pitch??0};
}
export function readGlobeCamera(raw:string|null){try{return validGlobeCamera(JSON.parse(raw??"null"))}catch{return null}}
export function readPreference(key:string){try{return localStorage.getItem(key)}catch{return null}}
export function writePreference(key:string,value:unknown){try{localStorage.setItem(key,JSON.stringify(value))}catch{/* preference only */}}
