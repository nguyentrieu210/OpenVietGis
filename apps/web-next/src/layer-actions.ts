import type {LayerView} from "./LayerProperties";
export const LABEL_STYLE_KEYS=["labelPlacement","labelSize","labelColor","labelHalo","labelHaloWidth","labelBold","labelBackground","labelOffset","labelPriority","labelOverlap","labelUnique","labelMinZoom","labelMaxZoom","labelPartial","labelSelectedOnly"] as const;
export type LabelStyle=Pick<LayerView,typeof LABEL_STYLE_KEYS[number]>;
export function copyLabelStyle(v:LayerView):LabelStyle{return Object.fromEntries(LABEL_STYLE_KEYS.map(k=>[k,v[k]])) as LabelStyle}
export function patchLayerViews(views:Record<string,LayerView>,ids:readonly string[],defaults:LayerView,patch:Partial<LayerView>|((v:LayerView)=>Partial<LayerView>)){
 const next={...views};for(const id of new Set(ids)){if(!id||["__proto__","constructor","prototype"].includes(id))continue;const v=next[id]??defaults;next[id]={...v,...(typeof patch==="function"?patch(v):patch)}}return next;
}
export function soloVisibility(ids:readonly string[],target:string,previous:ReadonlySet<string>|null,hidden:ReadonlySet<string>){
 if(previous)return {show:ids.filter(id=>!previous.has(id)),hide:ids.filter(id=>previous.has(id)),restore:null};
 return {show:[target],hide:ids.filter(id=>id!==target),restore:new Set(hidden)};
}
export function batchLabelVisibility(v:LayerView,show:boolean):Partial<LayerView>{return v.labelField||v.labelRules?.length?{labelHidden:!show}:{}}

export function mixedState(values:readonly boolean[]):boolean|"mixed"{return values.length&&values.some(Boolean)?values.every(Boolean)?true:"mixed":false}
export function layerLockState(canEdit:boolean,locked:boolean,editing:boolean):"readonly"|"editing"|"locked"|"unlocked"{return !canEdit?"readonly":editing?"editing":locked?"locked":"unlocked"}
