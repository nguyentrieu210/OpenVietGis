import {normalizeCommand} from "./command-search";
export type LocatorQuery={kind:"layer"|"feature"|"all";term:string}|{kind:"coordinate";point:[number,number]}|{kind:"invalid";reason:string};
const decimal="[+-]?(?:\\d+(?:\\.\\d+)?|\\.\\d+)";
const pair=new RegExp("^("+decimal+")\\s*[,;]\\s*("+decimal+")$");
export function parseLocator(raw:string):LocatorQuery{
 const input=raw.trim(),prefix=/^(layer|lớp|lop|feature|đối tượng|doi tuong|xy|tọa độ|toa do)\s*:\s*(.*)$/iu.exec(input);
 if(prefix){const kind=normalizeCommand(prefix[1]!),term=prefix[2]!.trim();
  if(kind==="LAYER"||kind==="LOP")return {kind:"layer",term};
  if(kind==="FEATURE"||kind==="DOI TUONG")return {kind:"feature",term};
  const found=pair.exec(term);
  if(!found)return {kind:"invalid",reason:"Nhập xy: kinh độ, vĩ độ (EPSG:4326)."};
  const x=Number(found[1]),y=Number(found[2]);
  if(!Number.isFinite(x)||!Number.isFinite(y)||Math.abs(x)>180||Math.abs(y)>85.05112878)return {kind:"invalid",reason:"Tọa độ ngoài phạm vi Web Mercator; thứ tự kinh độ, vĩ độ."};
  return {kind:"coordinate",point:[x,y]};
 }
 return {kind:"all",term:input};
}
export function locatorMatches(value:string,term:string){const haystack=normalizeCommand(value);return normalizeCommand(term).split(/\s+/).filter(Boolean).every(word=>haystack.includes(word))}
export function formatDms(value:number,pos:string,neg:string){
 if(!Number.isFinite(value))return "—";
 const total=Math.round(Math.abs(value)*360000),d=Math.floor(total/360000),m=Math.floor(total%360000/6000),s=(total%6000)/100;
 return d+"°"+String(m).padStart(2,"0")+"′"+s.toFixed(2).padStart(5,"0")+"″"+(value<0?neg:pos);
}
