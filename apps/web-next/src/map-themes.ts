import type {LayerView,MapTheme} from "./LayerProperties";
type LayerPin={id:string;source_id:string;lifecycle:string;source:{schema_revision:string};capabilities:readonly {name:string;available:boolean;actor_authorized:boolean}[]};
const readable=(l:LayerPin)=>l.lifecycle==="active"&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized);
export function themeNameAllowed(name:string,themes:Record<string,unknown>){return !!name.trim()&&name.length<=60&&!["__proto__","constructor","prototype"].includes(name)&&!Object.hasOwn(themes,name)&&Object.keys(themes).length<20}
export function snapshotTheme(layers:readonly LayerPin[],hidden:ReadonlySet<string>,views:Record<string,LayerView>,fallback:LayerView):MapTheme{
 const pins:NonNullable<MapTheme["pins"]>=Object.create(null),saved:Record<string,LayerView>=Object.create(null);
 for(const l of layers.filter(readable).slice(0,100)){pins[l.id]={source:l.source_id,schema:l.source.schema_revision};saved[l.id]={...(Object.hasOwn(views,l.id)?views[l.id]!:fallback),savedStyles:{},styleName:""}}
 return {hidden:Object.keys(pins).filter(id=>hidden.has(id)),views:saved,pins};
}
export function applyThemeSnapshot(theme:MapTheme,layers:readonly LayerPin[],hidden:ReadonlySet<string>,current:Record<string,LayerView>){
 const next={...current},want=new Set(theme.hidden),show:string[]=[],hide:string[]=[];
 let skipped=0;
 for(const l of layers){
  const pin=theme.pins&&Object.hasOwn(theme.pins,l.id)?theme.pins[l.id]:undefined;
  // Legacy snapshots had no membership or schema provenance: fail closed.
  if(!pin||!readable(l)||pin.source!==l.source_id||pin.schema!==l.source.schema_revision){skipped++;continue}
  if(want.has(l.id)!==hidden.has(l.id))(want.has(l.id)?hide:show).push(l.id);
  const v=Object.hasOwn(theme.views,l.id)?theme.views[l.id]:undefined;
  if(v)next[l.id]={...v,locked:current[l.id]?.locked??false,selectable:current[l.id]?.selectable??true,savedStyles:current[l.id]?.savedStyles??{},styleName:""};
 }
 return {views:next,show,hide,skipped};
}
