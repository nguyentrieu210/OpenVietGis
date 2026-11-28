import type {SpatialLayer} from "@openvietgis/client-sdk";
import {attributeValue} from "./attribute-value";
export type FormField=SpatialLayer["source"]["schema"]["fields"][number];
export type FieldLayout={id:string;group:string;label:string;hint:string;widget:"auto"|"multiline"};
export type FormLayout={version:1;fields:FieldLayout[]};
export type FormValues=Record<string,{raw:string;nil:boolean}>;
const safe=(v:unknown,max:number)=>typeof v==="string"?v.slice(0,max):"";
export function cleanFormLayout(raw:unknown,fields:readonly FormField[]):FormLayout{
 const byId=new Map(fields.map(f=>[f.field_id,f])),seen=new Set<string>(),result:FieldLayout[]=[];
 const data=raw as Partial<FormLayout>|null;
 if(data?.version===1&&Array.isArray(data.fields))for(const v of data.fields.slice(0,100)){
  if(!v||typeof v.id!=="string"||!byId.has(v.id)||seen.has(v.id))continue;
  seen.add(v.id);result.push({id:v.id,group:safe(v.group,60),label:safe(v.label,120),hint:safe(v.hint,250),widget:v.widget==="multiline"&&byId.get(v.id)?.type==="text"?"multiline":"auto"});
 }
 for(const f of fields)if(!seen.has(f.field_id))result.push({id:f.field_id,group:"",label:"",hint:"",widget:"auto"});
 return {version:1,fields:result};
}
export function formStorageKey(scope:string,layer:SpatialLayer){
 return scope?scope+":form:v1:"+encodeURIComponent(layer.source_id)+":"+encodeURIComponent(layer.id):"";
}
export function readFormLayout(key:string,fields:readonly FormField[]):FormLayout{
 try{return cleanFormLayout(key?JSON.parse(localStorage.getItem(key)??"null"):null,fields)}catch{return cleanFormLayout(null,fields)}
}
export function moveFormField(layout:FormLayout,index:number,delta:number):FormLayout{
 const items=[...layout.fields],next=index+delta;
 if(!Number.isInteger(index)||![-1,1].includes(delta)||index<0||index>=items.length||next<0||next>=items.length)return layout;
 [items[index],items[next]]=[items[next]!,items[index]!];return {version:1,fields:items};
}
export function formGroups(layout:FormLayout,fields:readonly FormField[]){
 const map=new Map(fields.map(f=>[f.field_id,f])),groups=new Map<string,{field:FormField;layout:FieldLayout}[]>();
 for(const item of cleanFormLayout(layout,fields).fields){
  const group=item.group.trim(),entry={field:map.get(item.id)!,layout:item};
  const existing=groups.get(group);if(existing)existing.push(entry);else groups.set(group,[entry]);
 }
 return [...groups].map(([name,items])=>({name,items}));
}
export function formValues(fields:readonly FormField[],attrs:Record<string,unknown>):FormValues{
 return Object.fromEntries(fields.map(f=>{const v=attrs[f.field_id];return [f.field_id,{raw:v==null?"":String(v),nil:v==null&&f.nullable}]}));
}
export function validateForm(fields:readonly FormField[],values:FormValues){
 const properties:Record<string,unknown>={},errors:Record<string,string>={};
 for(const f of fields){const v=values[f.field_id];try{if(!v)throw Error("Thiếu giá trị "+f.name);properties[f.field_id]=attributeValue(f,v.raw,v.nil)}catch(e){errors[f.field_id]=e instanceof Error?e.message:String(e)}}
 return {properties,errors};
}
