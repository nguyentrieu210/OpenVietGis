import {useId,useState} from "react";
import type {ApiClient,SpatialLayer} from "@openvietgis/client-sdk";
import {cleanFormLayout,formGroups,formStorageKey,moveFormField,readFormLayout,validateForm,type FormLayout,type FormValues} from "./attribute-form-layout";
import {RelationValuePicker} from "./RelationValuePicker";
import type {ProjectRelation} from "./relation-values";
import "./attribute-form-layout.css";
export function FormLayoutDesigner({scope,layer}:{scope:string;layer:SpatialLayer}){
 const fields=layer.source.schema.fields,key=formStorageKey(scope,layer);
 const [draft,setDraft]=useState(()=>readFormLayout(key,fields)),[notice,setNotice]=useState("");
 const layout=cleanFormLayout(draft,fields);
 const patch=(index:number,delta:Partial<FormLayout["fields"][number]>)=>{setDraft({version:1,fields:layout.fields.map((f,i)=>i===index?{...f,...delta}:f)});setNotice("")};
 return <details className="ovg-form-designer"><summary>Thiết kế biểu mẫu cá nhân</summary><p>Cấu hình theo tài khoản, dự án và lớp trên thiết bị này. Không thay đổi schema, quyền hoặc dữ liệu dự án.</p>
 {layout.fields.map((item,i)=>{const field=fields.find(f=>f.field_id===item.id)!;return <fieldset key={item.id}><legend>{field.name} · {field.type}</legend>
 <div className="ovg-form-order"><button type="button" aria-label={"Đưa "+field.name+" lên"} disabled={!i} onClick={()=>{setDraft(moveFormField(layout,i,-1));setNotice("")}}>↑</button><button type="button" aria-label={"Đưa "+field.name+" xuống"} disabled={i===layout.fields.length-1} onClick={()=>{setDraft(moveFormField(layout,i,1));setNotice("")}}>↓</button></div>
 <label>Nhóm<input value={item.group} maxLength={60} onChange={e=>patch(i,{group:e.target.value})}/></label>
 <label>Nhãn hiển thị<input value={item.label} placeholder={field.name} maxLength={120} onChange={e=>patch(i,{label:e.target.value})}/></label>
 <label>Hướng dẫn<input value={item.hint} maxLength={250} onChange={e=>patch(i,{hint:e.target.value})}/></label>
 {field.type==="text"&&<label>Kiểu nhập<select value={item.widget} onChange={e=>patch(i,{widget:e.target.value==="multiline"?"multiline":"auto"})}><option value="auto">Một dòng</option><option value="multiline">Nhiều dòng</option></select></label>}</fieldset>})}
 <button type="button" disabled={!key} onClick={()=>{try{localStorage.setItem(key,JSON.stringify(layout));setNotice("Đã lưu cấu hình biểu mẫu cá nhân.")}catch{setNotice("Không lưu được cấu hình trên thiết bị.")}}}>Lưu biểu mẫu cá nhân</button>
 <button type="button" onClick={()=>{setDraft(cleanFormLayout(null,fields));setNotice("Đã đặt lại bản nháp; bấm lưu để áp dụng.")}}>Đặt lại</button>{notice&&<p role="status">{notice}</p>}</details>;
}
export function AttributeFormFields({client,org,pid,scope,layer,layers,values,change,rules,disabled}:{client:ApiClient;org:string;pid:string;scope:string;layer:SpatialLayer;layers:SpatialLayer[];values:FormValues;change:(id:string,v:FormValues[string])=>void;rules:ProjectRelation[];disabled:boolean}){
 const uid=useId(),fields=layer.source.schema.fields,layout=readFormLayout(formStorageKey(scope,layer),fields);
 const {errors}=validateForm(fields,values);
 return <div className="ovg-attribute-form">{formGroups(layout,fields).map(group=><fieldset key={group.name} disabled={disabled}><legend>{group.name||"Thuộc tính"}</legend>{group.items.map(({field,layout})=>{
 const k=field.field_id,v=values[k]??{raw:"",nil:false},id=uid+k,label=layout.label.trim()||field.name;
 const update=(raw:string)=>change(k,{raw,nil:false});
 return <div key={k} className="ovg-form-field"><label htmlFor={id}>{label}{field.nullable?"":" *"}</label>
 {field.type==="boolean"?<select id={id} aria-label={field.name} disabled={v.nil} value={v.raw} onChange={e=>update(e.target.value)}><option value="">Chọn</option><option value="true">true</option><option value="false">false</option></select>:layout.widget==="multiline"?<textarea id={id} aria-label={field.name} disabled={v.nil} value={v.raw} rows={3} maxLength={4096} onChange={e=>update(e.target.value)}/>:<input id={id} aria-label={field.name} disabled={v.nil} inputMode={field.type==="integer"?"numeric":field.type==="decimal"?"decimal":"text"} placeholder={field.type==="timestamp"?"2026-01-01T00:00:00Z":field.type} value={v.raw} maxLength={4096} onChange={e=>update(e.target.value)}/>}
 {field.nullable&&<label><input type="checkbox" aria-label={"NULL "+field.name} checked={v.nil} onChange={e=>change(k,{...v,nil:e.target.checked})}/> NULL</label>}
 {layout.hint&&<small>{layout.hint}</small>}{errors[k]&&<small className="error" role="status">{errors[k]}</small>}
 <RelationValuePicker client={client} org={org} pid={pid} source={layer} layers={layers} field={k} rules={rules} disabled={disabled} choose={update}/>
 </div>})}</fieldset>)}</div>;
}
