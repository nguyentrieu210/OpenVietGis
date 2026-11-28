import {attributeValue} from "./attribute-value";
import {compareTableValue} from "./table-columns";
export type RuleField={field_id:string;name:string;type:string;nullable:boolean};
export const ruleOps=["eq","neq","gt","gte","lt","lte","is_null","not_null"] as const;
export type RuleOp=typeof ruleOps[number];
export type StyleRule={id:string;label:string;field:string;op:RuleOp;value:string;color:string};
export const RULE_LIMIT=12;
export const fallbackRuleKey="rule:else";
export const ruleKey=(r:StyleRule)=>"rule:"+r.id;
export function cleanRules(x:unknown):StyleRule[]{
 if(!Array.isArray(x))return [];
 const ids=new Set<string>();
 return x.slice(0,RULE_LIMIT).flatMap((r:unknown)=>{
  if(!r||typeof r!=="object")return [];
  const v=r as Partial<StyleRule>;
  if(typeof v.id!=="string"||!(/^[a-z0-9-]{1,64}$/).test(v.id)||v.id==="else"||ids.has(v.id)||typeof v.field!=="string"||v.field.length>64||!ruleOps.includes(v.op as RuleOp)||typeof v.value!=="string"||typeof v.label!=="string"||typeof v.color!=="string"||!/^#[a-f0-9]{6}$/i.test(v.color))return [];
  ids.add(v.id);return [{id:v.id,label:v.label.slice(0,80),field:v.field,op:v.op!,value:v.value.slice(0,256),color:v.color}];
 });
}
export function ruleError(r:StyleRule,fields:readonly RuleField[]):string|null{
 const f=fields.find(x=>x.field_id===r.field);if(!f)return "Chọn trường có trong schema.";
 if(!r.label.trim())return "Nhập tên quy tắc.";
 if(r.op==="is_null"||r.op==="not_null")return null;
 if(!["text","boolean","integer","decimal","timestamp"].includes(f.type))return "Kiểu trường chưa hỗ trợ.";
 if(["gt","gte","lt","lte"].includes(r.op)&&!["integer","decimal","timestamp"].includes(f.type))return "So sánh thứ tự chỉ dùng số hoặc timestamp.";
 try{attributeValue(f,r.value,false);return null}catch(e){return e instanceof Error?e.message:String(e)}
}
export function ruleFor(attrs:Record<string,unknown>,rules:readonly StyleRule[],fields:readonly RuleField[]):StyleRule|undefined{
 for(const r of rules){
  const f=fields.find(x=>x.field_id===r.field);if(!f||!Object.hasOwn(attrs,r.field))continue;
  const v=attrs[r.field];if(r.op==="is_null"){if(v===null)return r;continue}if(r.op==="not_null"){if(v!==null&&v!==undefined)return r;continue}
  if(v===null||v===undefined||ruleError(r,fields))continue;
  try{const actual=attributeValue(f,String(v),false),expected=attributeValue(f,r.value,false);
   const cmp=f.type==="text"?(actual===expected?0:1):compareTableValue(actual,expected,f.type,"asc");
   if(r.op==="eq"?cmp===0:r.op==="neq"?cmp!==0:r.op==="gt"?cmp>0:r.op==="gte"?cmp>=0:r.op==="lt"?cmp<0:cmp<=0)return r;
  }catch{/* malformed input is unmatched */}
 }
 return undefined;
}
export function ruleLegend(rows:readonly {feature:{attributes:Record<string,unknown>}}[],rules:StyleRule[],fields:readonly RuleField[],fallback:string){
 const counts=new Map<string,number>();
 for(const row of rows){const r=ruleFor(row.feature.attributes,rules,fields),key=r?ruleKey(r):fallbackRuleKey;counts.set(key,(counts.get(key)??0)+1)}
 return [...rules.map(r=>({key:ruleKey(r),label:r.label,color:r.color,count:counts.get(ruleKey(r))??0})),{key:fallbackRuleKey,label:"Còn lại",color:fallback,count:counts.get(fallbackRuleKey)??0}];
}
