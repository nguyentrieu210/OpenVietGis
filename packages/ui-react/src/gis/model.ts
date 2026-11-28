export type GeometryFamily='point'|'line'|'polygon'|'raster';
export type SymbolStyle={color:string;stroke:string;opacity:number;size:number};
export const hexColor=(x:unknown):x is string=>typeof x==='string'&&/^#[0-9a-f]{6}$/i.test(x);
export const finiteBetween=(x:unknown,min:number,max:number):x is number=>typeof x==='number'&&Number.isFinite(x)&&x>=min&&x<=max;
export const validSymbol=(v:SymbolStyle)=>hexColor(v.color)&&hexColor(v.stroke)&&finiteBetween(v.opacity,0,1)&&finiteBetween(v.size,1,32);
export type LayerNode={kind:'layer';id:string;name:string;visible:boolean;locked?:boolean;canVisibility:boolean;canReorder:boolean;family:GeometryFamily;symbol:SymbolStyle}|{kind:'group';id:string;name:string;children:readonly LayerNode[]};
export type LayerLeaf=Extract<LayerNode,{kind:'layer'}>;
export function layerLeaves(nodes:readonly LayerNode[]):LayerLeaf[]{
 const out:LayerLeaf[]=[],seen=new Set<string>();let count=0;
 const visit=(items:readonly LayerNode[],depth:number)=>{if(depth>32)throw Error('Layer tree too deep');for(const n of items){if(++count>10000||!n.id||seen.has(n.id))throw Error('Invalid layer tree');seen.add(n.id);if(n.kind==='layer')out.push(n);else visit(n.children,depth+1)}};visit(nodes,0);return out;
}
export function visibilityIntent(nodes:readonly LayerNode[],visible:boolean){return layerLeaves(nodes).filter(n=>n.canVisibility&&n.visible!==visible).map(n=>({id:n.id,visible}))}
export type CrsOption={id:string;name:string;group:string;unit:'degree'|'metre'|'foot'|'other';axisOrder:'xy'|'yx';description?:string};
export function filterCrs(options:readonly CrsOption[],query:string){const q=query.normalize('NFC').toLocaleLowerCase('vi').trim();return options.filter(x=>[x.id,x.name,x.group,x.description??''].some(v=>v.normalize('NFC').toLocaleLowerCase('vi').includes(q)))}
export type CoordinateContext={crsId:string;unit:CrsOption['unit'];axisOrder:CrsOption['axisOrder'];geographic?:boolean};
export type CoordinateIntent={context:CoordinateContext;mode:'absolute'|'relative';x:number;y:number};
const decimal=/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
export function parseCoordinate(x:string,y:string,context:CoordinateContext,mode:CoordinateIntent['mode']):CoordinateIntent|null{
 if(!context.crsId.trim()||!decimal.test(x.trim())||!decimal.test(y.trim()))return null;
 const a=Number(x),b=Number(y);if(!Number.isFinite(a)||!Number.isFinite(b))return null;
 if(context.geographic&&mode==='absolute'){const [lon,lat]=context.axisOrder==='xy'?[a,b]:[b,a];if(Math.abs(lon)>180||Math.abs(lat)>90)return null}
 return {context:{...context},mode,x:a,y:b};
}
export type SnapKind='vertex'|'edge'|'intersection'|'midpoint'|'endpoint'|'perpendicular';
export type SnapConfig={enabled:boolean;kinds:readonly SnapKind[];tolerance:number;unit:'px'|'m';scope:'active'|'all'|'selected'};
export function validSnap(v:SnapConfig){return typeof v.enabled==='boolean'&&['px','m'].includes(v.unit)&&['active','all','selected'].includes(v.scope)&&finiteBetween(v.tolerance,.1,v.unit==='px'?100:1000)&&new Set(v.kinds).size===v.kinds.length&&v.kinds.every(k=>['vertex','edge','intersection','midpoint','endpoint','perpendicular'].includes(k))&&(!v.enabled||v.kinds.length>0)}
export type PropertyValue=string|number|boolean|null;
export type PropertyField={id:string;label:string;kind:'text'|'number'|'boolean'|'enum';value:PropertyValue;editable:boolean;nullable?:boolean;options?:readonly string[];min?:number;max?:number;maxLength?:number};
export function parseProperty(f:PropertyField,raw:string):{ok:true;value:PropertyValue}|{ok:false}{
 if(raw===''&&f.nullable)return {ok:true,value:null};
 if(f.kind==='number'){if(!decimal.test(raw.trim()))return {ok:false};const n=Number(raw);return finiteBetween(n,f.min??-Number.MAX_VALUE,f.max??Number.MAX_VALUE)?{ok:true,value:n}:{ok:false}}
 if(f.kind==='boolean')return raw==='true'||raw==='false'?{ok:true,value:raw==='true'}:{ok:false};
 if(f.kind==='enum')return f.options?.includes(raw)?{ok:true,value:raw}:{ok:false};
 return raw.length<=(f.maxLength??4096)?{ok:true,value:raw}:{ok:false};
}
export const propertyText=(v:PropertyValue)=>v===null?'':String(v);
export function rampColors(from:string,to:string,steps:number):string[]{
 if(!hexColor(from)||!hexColor(to)||!Number.isInteger(steps)||steps<2||steps>32)return [];
 const a=[1,3,5].map(i=>parseInt(from.slice(i,i+2),16)),b=[1,3,5].map(i=>parseInt(to.slice(i,i+2),16));
 return Array.from({length:steps},(_,i)=>'#'+a.map((c,j)=>Math.round(c+(b[j]-c)*i/(steps-1)).toString(16).padStart(2,'0')).join(''));
}