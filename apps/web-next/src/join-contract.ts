/** Exact-key, bounded many-to-one spreadsheet join. Blank keys never match. */
export function joinKeyOf(value:unknown):string|null{
 if(typeof value==="string")return value.trim()?value:null;
 if(typeof value==="number")return Number.isSafeInteger(value)?String(value):null;
 if(typeof value==="boolean")return String(value);
 return null;
}
export function joinLookup(rows:readonly string[][],key:number,value:number){
 if(key<0||value<0||key===value||rows.length>10000)throw Error("Chọn hai cột nguồn khác nhau; tối đa 10.000 dòng.");
 const map=new Map<string,string>();let blank=0;
 for(const row of rows){if(key>=row.length||value>=row.length)throw Error("Số cột không đồng nhất.");const k=joinKeyOf(row[key]);if(k===null){blank++;continue}if(map.has(k))throw Error("Khóa Excel trùng: "+k);map.set(k,row[value]!);}
 return {map,blank};
}
export function stripTrailingNewlines(raw:string){return raw.replace(/(?:\r?\n)+$/,"")}
