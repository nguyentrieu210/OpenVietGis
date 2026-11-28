import {useId,useState} from "react";
import type {ApiClient,SpatialLayer} from "@openvietgis/client-sdk";
import {cleanFormLayout,formGroups,formStorageKey,moveFormField,readFormLayout,validateForm,valueMapSelection,type FormField,type ValueMapItem,type FormLayout,type FormValues} from "./attribute-form-layout";
import {RelationValuePicker} from "./RelationValuePicker";
import {attributeValue} from "./attribute-value";
import type {ProjectRelation} from "./relation-values";
import "./attribute-form-layout.css";
export function FormLayoutDesigner({scope,layer}:{scope:string;layer:SpatialLayer}){
 const fields=layer.source.schema.fields,key=formStorageKey(scope,layer);
 const [draft,setDraft]=useState(()=>readFormLayout(key,fields)),[notice,setNotice]=useState("");
 const layout=cleanFormLayout(draft,fields);
 const [choiceError,setChoiceError]=useState("");
 const patch=(index:number,delta:Partial<FormLayout["fields"][number]>)=>{setDraft({version:1,fields:layout.fields.map((f,i)=>i===index?{...f,...delta}:f)});setNotice("")};
 return <details className="ovg-form-designer"><summary>Thiết kế biểu mẫu cá nhân</summary><p>Cấu hình theo tài khoản, dự án và lớp trên thiết bị này. Không thay đổi schema, quyền hoặc dữ liệu dự án.</p>
 {layout.fields.map((item,i)=>{const field=fields.find(f=>f.field_id===item.id)!;return <fieldset key={item.id}><legend>{field.name} · {field.type}</legend>
 <div className="ovg-form-order"><button type="button" aria-label={"Đưa "+field.name+" lên"} disabled={!i} onClick={()=>{setDraft(moveFormField(layout,i,-1));setNotice("")}}>↑</button><button type="button" aria-label={"Đưa "+field.name+" xuống"} disabled={i===layout.fields.length-1} onClick={()=>{setDraft(moveFormField(layout,i,1));setNotice("")}}>↓</button></div>
 <label>Nhóm<input value={item.group} maxLength={60} onChange={e=>patch(i,{group:e.target.value})}/></label>
 <label>Nhãn hiển thị<input value={item.label} placeholder={field.name} maxLength={120} onChange={e=>patch(i,{label:e.target.value})}/></label>
 <label>Hướng dẫn<input value={item.hint} maxLength={250} onChange={e=>patch(i,{hint:e.target.value})}/></label>
 {field.type!=="boolean"&&<label>Kiểu nhập<select value={item.widget} onChange={e=>{const widget=e.target.value as FormLayout["fields"][number]["widget"];patch(i,{widget});setChoiceError("")}}><option value="auto">Tự động</option>{field.type==="text"&&<option value="multiline">Nhiều dòng</option>}<option value="value-map">Danh sách giá trị (Value Map)</option></select></label>}
 {item.widget==="value-map"&&<ValueMapDesigner key={item.id} field={field} choices={item.choices??[]} change={choices=>{patch(i,{choices});setChoiceError("")}} report={setChoiceError}/>}</fieldset>})}
 {choiceError&&<p role="alert">{choiceError}</p>}<button type="button" disabled={!key} onClick={()=>{try{localStorage.setItem(key,JSON.stringify(layout));setNotice("Đã lưu cấu hình biểu mẫu cá nhân.")}catch{setNotice("Không lưu được cấu hình trên thiết bị.")}}}>Lưu biểu mẫu cá nhân</button>
 <button type="button" onClick={()=>{setDraft(cleanFormLayout(null,fields));setNotice("Đã đặt lại bản nháp; bấm lưu để áp dụng.")}}>Đặt lại</button>{notice&&<p role="status">{notice}</p>}</details>;
}
function ValueMapDesigner({field,choices,change,report}:{field:FormField;choices:ValueMapItem[];change:(items:ValueMapItem[])=>void;report:(error:string)=>void}){
 const [raw,setRaw]=useState(""),[label,setLabel]=useState("");
 function add(){try{
  const value=String(attributeValue(field,raw));
  if(choices.some(c=>c.value===value))throw Error("Giá trị đã tồn tại.");
  if(choices.length>=100)throw Error("Tối đa 100 lựa chọn mỗi trường.");
  change([...choices,{value,label:label.trim().slice(0,120)}]);setRaw("");setLabel("");
 }catch(e){report(e instanceof Error?e.message:String(e))}}
 return <div className="ovg-value-map"><small>Danh sách gợi ý cá nhân, không phải ràng buộc dữ liệu. Giá trị ngoài danh sách vẫn chỉnh sửa được.</small>
 {choices.map((c,i)=><div key={c.value}><span>{c.label||c.value||"(chuỗi rỗng)"} · {c.value}</span><button type="button" aria-label={"Bỏ giá trị "+i} onClick={()=>change(choices.filter((_,j)=>i!==j))}>Bỏ</button></div>)}
 <label>Giá trị gốc<input value={raw} maxLength={4096} onChange={e=>setRaw(e.target.value)}/></label>
 <label>Nhãn lựa chọn<input value={label} maxLength={120} onChange={e=>setLabel(e.target.value)}/></label>
 <button type="button" disabled={choices.length>=100} onClick={add}>Thêm lựa chọn</button></div>
}
export function AttributeFormFields({client,org,pid,scope,layer,layers,values,change,rules,disabled}:{client:ApiClient;org:string;pid:string;scope:string;layer:SpatialLayer;layers:SpatialLayer[];values:FormValues;change:(id:string,v:FormValues[string])=>void;rules:ProjectRelation[];disabled:boolean}){
 const uid=useId(),fields=layer.source.schema.fields,layout=readFormLayout(formStorageKey(scope,layer),fields);
 const {errors}=validateForm(fields,values);
 return <div className="ovg-attribute-form">{formGroups(layout,fields).map(group=><fieldset key={group.name} disabled={disabled}><legend>{group.name||"Thuộc tính"}</legend>{group.items.map(({field,layout})=>{
 const k=field.field_id,v=values[k]??{raw:"",nil:false},id=uid+k,label=layout.label.trim()||field.name;
 const update=(raw:string)=>change(k,{raw,nil:false});
 return <div key={k} className="ovg-form-field"><label htmlFor={id}>{label}{field.nullable?"":" *"}</label>
 {field.type==="boolean"?<select id={id} aria-label={field.name} disabled={v.nil} value={v.raw} onChange={e=>update(e.target.value)}><option value="">Chọn</option><option value="true">true</option><option value="false">false</option></select>:layout.widget==="multiline"?<textarea id={id} aria-label={field.name} disabled={v.nil} value={v.raw} rows={3} maxLength={4096} onChange={e=>update(e.target.value)}/>:<input id={id} aria-label={field.name} disabled={v.nil} inputMode={field.type==="integer"?"numeric":field.type==="decimal"?"decimal":"text"} placeholder={field.type==="timestamp"?"2026-01-01T00:00:00Z":field.type} value={v.raw} maxLength={4096} onChange={e=>update(e.target.value)}/>}
 {layout.widget==="value-map"&&!!layout.choices?.length&&<label>Danh sách giá trị<select aria-label={"Danh sách "+field.name} disabled={v.nil||disabled} value={String(valueMapSelection(field,v.raw,layout.choices))} onChange={e=>{const index=Number(e.target.value),choice=layout.choices?.[index];if(choice)update(choice.value)}}><option value="-1">Nhập thủ công / ngoài danh sách</option>{layout.choices.map((c,i)=><option key={c.value} value={i}>{c.label||c.value||"(chuỗi rỗng)"}</option>)}</select></label>}
 {field.nullable&&<label><input type="checkbox" aria-label={"NULL "+field.name} checked={v.nil} onChange={e=>change(k,{...v,nil:e.target.checked})}/> NULL</label>}
 {layout.hint&&<small>{layout.hint}</small>}{errors[k]&&<small className="error" role="status">{errors[k]}</small>}
 <RelationValuePicker client={client} org={org} pid={pid} source={layer} layers={layers} field={k} rules={rules} disabled={disabled} choose={update}/>
 </div>})}</fieldset>)}</div>;
}
