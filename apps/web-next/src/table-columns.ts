// Personal column preferences are constrained to the current schema.
export function readHiddenColumns(raw:string|null,fields:readonly string[]):string[]{
 try{const v=JSON.parse(raw??"null");return Array.isArray(v)?[...new Set(v.filter((x):x is string=>typeof x==="string"&&fields.includes(x)))]:[]}catch{return []}
}
export function togglePageSelection(previous:ReadonlySet<string>,keys:readonly string[],checked:boolean):Set<string>{
 const next=new Set(previous);for(const key of keys)checked?next.add(key):next.delete(key);return next;
}
export function toggleFeatureSelection(previous:ReadonlySet<string>,key:string):Set<string>{
 const next=new Set(previous);next.has(key)?next.delete(key):next.add(key);return next;
}
export function tsvCell(value:unknown):string{
 // Spreadsheet formula injection protection for strings only; preserve numeric values.
 const text=String(value??"").replace(/[\t\r\n]/g," ");
 return typeof value==="string"&&/^[\s]*[=+\-@]/.test(text)?"'"+text:text;
}
export function tableTsv(headers:readonly string[],rows:readonly (readonly unknown[])[]):string{
 return [headers,...rows].map(row=>row.map(tsvCell).join("\t")).join("\n");
}
