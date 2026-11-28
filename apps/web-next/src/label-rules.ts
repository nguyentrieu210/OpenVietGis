import {validateLabel} from "./label-expr";
import type {LayerView} from "./LayerProperties";
export type LabelRule={min:number;max:number;field:string;expression:string;size:number;color:string};
export function cleanLabelRules(raw:unknown):LabelRule[]{
 if(!Array.isArray(raw))return [];
 const out:LabelRule[]=[];
 for(const a of raw.slice(0,8)){
  if(!a||typeof a!=="object"||!Number.isFinite(a.min)||!Number.isFinite(a.max)||a.min<0||a.max>24||a.max<=a.min||typeof a.field!=="string"||!a.field||a.field.length>128)continue;
  out.push({min:a.min,max:a.max,field:a.field,expression:typeof a.expression==="string"?a.expression.slice(0,500):"",size:Number.isFinite(a.size)?Math.max(8,Math.min(32,a.size)):12,color:typeof a.color==="string"&&/^#[0-9a-f]{6}$/i.test(a.color)?a.color:"#223d35"});
 }
 out.sort((a,b)=>a.min-b.min||a.max-b.max);
 if(out.some((r,i)=>i>0&&r.min<out[i-1].max))return [];
 return out;
}
export function labelRulesError(rules:readonly LabelRule[],fields:readonly string[]){
 if(rules.length>8||cleanLabelRules(rules).length!==rules.length)return "Các khoảng zoom phải hợp lệ, không chồng lấn; tối đa 8 quy tắc.";
 for(const r of rules){if(r.field==="__expression__"){const error=validateLabel(r.expression,fields);if(error)return error}else if(!fields.includes(r.field))return "Trường nhãn không tồn tại: "+r.field}
 return null;
}
export function labelVariants(v:LayerView):{id:string;view:LayerView}[]{
 const rules=cleanLabelRules(v.labelRules);
 if(!rules.length)return [{id:"",view:v}];
 return rules.map((r,i)=>({id:":scale"+i,view:{...v,labelField:r.field,labelExpression:r.expression,labelSize:r.size,labelColor:r.color,labelMinZoom:Math.max(v.labelMinZoom,r.min),labelMaxZoom:Math.min(v.labelMaxZoom,r.max)}}));
}
