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

// Decimal values can be canonical strings with up to 38 digits; never round through Number.
function decimalParts(value:unknown):{negative:boolean;whole:string;fraction:string}|null{
 const s=typeof value==="number"&&Number.isFinite(value)?String(value):typeof value==="string"?value:"";
 if(!/^-?\d+(?:\.\d+)?$/.test(s))return null;
 const [w,f=""]=s.replace(/^-/,"").split("."),whole=w!.replace(/^0+/,"")||"0",fraction=f.replace(/0+$/,"");
 return {negative:s.startsWith("-")&&(whole!=="0"||!!fraction),whole,fraction};
}
function decimalCompare(a:NonNullable<ReturnType<typeof decimalParts>>,b:NonNullable<ReturnType<typeof decimalParts>>){
 if(a.negative!==b.negative)return a.negative?-1:1;
 const sign=a.negative?-1:1,cmp=a.whole.length-b.whole.length|| (a.whole<b.whole?-1:a.whole>b.whole?1:0);
 if(cmp)return sign*cmp;
 const len=Math.max(a.fraction.length,b.fraction.length),x=a.fraction.padEnd(len,"0"),y=b.fraction.padEnd(len,"0");
 return sign*(x<y?-1:x>y?1:0);
}
export function compareTableValue(a:unknown,b:unknown,type:string|undefined,direction:"asc"|"desc"):number{
 const value=(v:unknown):unknown=>v==null?null:type==="decimal"||type==="integer"?decimalParts(v):type==="timestamp"?typeof v==="string"&&Number.isFinite(Date.parse(v))?Date.parse(v):null:type==="boolean"?typeof v==="boolean"?Number(v):null:String(v);
 const x=value(a),y=value(b);if(x===null||y===null)return x===y?0:x===null?1:-1;
 const cmp=type==="decimal"||type==="integer"?decimalCompare(x as NonNullable<ReturnType<typeof decimalParts>>,y as NonNullable<ReturnType<typeof decimalParts>>):typeof x==="number"&&typeof y==="number"?x-y:String(x).localeCompare(String(y),"vi",{numeric:false});
 return direction==="asc"?cmp:-cmp;
}
export type SelectionLayerPin={id:string;source_id:string;lifecycle:string;layer_revision?:number;source:{content_revision:string;schema_revision:string};capabilities:readonly {name:string;available:boolean;actor_authorized:boolean}[]};
export function selectionScopePin(layers:readonly SelectionLayerPin[],permissions:readonly string[],restricted:boolean|undefined):string{
 return JSON.stringify([restricted,[...permissions].sort(),layers.map(l=>[l.id,l.source_id,l.lifecycle,l.layer_revision,l.source.content_revision,l.source.schema_revision,l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized)]).sort((a,b)=>String(a[0]).localeCompare(String(b[0])))]);
}

export type ColumnConfig={hidden:string[];order:string[];widths:Record<string,number>};
export function columnConfig(raw:unknown,fields:readonly string[]):ColumnConfig{
 const v=raw&&typeof raw==="object"&&!Array.isArray(raw)?raw as Partial<ColumnConfig>:{};
 const list=(x:unknown)=>Array.isArray(x)?[...new Set(x.filter((id):id is string=>typeof id==="string"&&fields.includes(id)))]:[];
 const order=list(v.order),widths:Record<string,number>=Object.create(null);
 for(const id of fields){const w=v.widths&&Object.hasOwn(v.widths,id)?v.widths[id]:undefined;if(typeof w==="number"&&Number.isFinite(w))widths[id]=Math.round(Math.min(600,Math.max(70,w)))}
 return {hidden:list(Array.isArray(raw)?raw:v.hidden),order:[...order,...fields.filter(id=>!order.includes(id))],widths};
}
export function readColumnConfig(raw:string|null,fields:readonly string[]):ColumnConfig{
 try{return columnConfig(JSON.parse(raw??"null"),fields)}catch{return columnConfig(null,fields)}
}
export function moveColumn(order:readonly string[],id:string,direction:-1|1){
 const next=[...order],i=next.indexOf(id),j=i+direction;if(i<0||j<0||j>=next.length)return next;
 [next[i],next[j]]=[next[j]!,next[i]!];return next;
}
export function completeSelection<T extends {key:string;layer:{id:string}}>(rows:readonly T[],selected:ReadonlySet<string>,scope:string){
 const unique=new Map<string,T>();for(const r of rows)if(selected.has(r.key)&&(!scope||r.layer.id===scope)&&!unique.has(r.key))unique.set(r.key,r);
 return unique.size===selected.size?[...unique.values()]:null;
}
