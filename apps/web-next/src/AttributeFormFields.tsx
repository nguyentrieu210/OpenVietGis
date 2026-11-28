import {useId,useState} from "react";
import type {ApiClient,SpatialLayer} from "@openvietgis/client-sdk";
import {cleanFormLayout,formGroups,formStorageKey,moveFormField,readFormLayout,validateForm,valueMapSelection,type FormField,type ValueMapItem,type FormLayout,type FormValues} from "./attribute-form-layout";
import {RelationValuePicker} from "./RelationValuePicker";
import {attributeValue} from "./attribute-value";
import type {ProjectRelation} from "./relation-values";
import {Actions,Button,Input,Select,Textarea} from "./ui/primitives";
import "./attribute-form-layout.css";
export function FormLayoutDesigner({scope,layer}:{scope:string;layer:SpatialLayer}){
 const fields=layer.source.schema.fields,key=formStorageKey(scope,layer);
 const [draft,setDraft]=useState(()=>readFormLayout(key,fields)),[notice,setNotice]=useState("");
 const layout=cleanFormLayout(draft,fields);
 const [choiceError,setChoiceError]=useState("");
 const patch=(index:number,delta:Partial<FormLayout["fields"][number]>)=>{setDraft({version:1,fields:layout.fields.map((f,i)=>i===index?{...f,...delta}:f)});setNotice("")};
 return <details className="ovg-form-designer"><summary>Thiết kế biểu mẫu cá nhân</summary><p>Cấu hình theo tài khoản, dự án và lớp trên thiết bị này. Không thay đổi schema, quyền hoặc dữ liệu dự án.</p>
 {layout.fields.map((item,i)=>{const field=fields.find(f=>f.field_id===item.id)!;return <fieldset key={item.id}><legend>{field.name} · {field.type}</legend>
 <Actions className="ovg-form-order"><Button type="button" aria-label={"Đưa "+field.name+" lên"} disabled={!i} onClick={()=>{setDraft(moveFormField(layout,i,-1));setNotice("")}}>↑</Button><Button type="button" aria-label={"Đưa "+field.name+" xuống"} disabled={i===layout.fields.length-1} onClick={()=>{setDraft(moveFormField(layout,i,1));setNotice("")}}>↓</Button></Actions>
 <label>Nhóm<Input value={item.group} maxLength={60} onChange={e=>patch(i,{group:e.target.value})}/></label>
 <label>Nhãn hiển thị<Input value={item.label} placeholder={field.name} maxLength={120} onChange={e=>patch(i,{label:e.target.value})}/></label>
 <label>Hướng dẫn<Input value={item.hint} maxLength={250} onChange={e=>patch(i,{hint:e.target.value})}/></label>
 {field.type!=="boolean"&&<label>Kiểu nhập<Select value={item.widget} onChange={e=>{const widget=e.target.value as FormLayout["fields"][number]["widget"];patch(i,{widget});setChoiceError("")}}><option value="auto">Tự động</option>{field.type==="text"&&<option value="multiline">Nhiều dòng</option>}<option value="value-map">Danh sách giá trị (Value Map)</option></Select></label>}
 {item.widget==="value-map"&&<ValueMapDesigner key={item.id} field={field} choices={item.choices??[]} change={choices=>{patch(i,{choices});setChoiceError("")}} report={setChoiceError}/>}</fieldset>})}
 {choiceError&&<p role="alert">{choiceError}</p>}<Actions className="ovg-form-save-actions"><Button type="button" disabled={!key} onClick={()=>{try{localStorage.setItem(key,JSON.stringify(layout));setNotice("Đã lưu cấu hình biểu mẫu cá nhân.")}catch{setNotice("Không lưu được cấu hình trên thiết bị.")}}}>Lưu biểu mẫu cá nhân</Button>
 <Button type="button" onClick={()=>{setDraft(cleanFormLayout(null,fields));setNotice("Đã đặt lại bản nháp; bấm lưu để áp dụng.")}}>Đặt lại</Button></Actions>{notice&&<p role="status">{notice}</p>}</details>;
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
 {choices.map((c,i)=><div key={c.value}><span>{c.label||c.value||"(chuỗi rỗng)"} · {c.value}</span><Button type="button" aria-label={"Bỏ giá trị "+i} onClick={()=>change(choices.filter((_,j)=>i!==j))}>Bỏ</Button></div>)}
 <label>Giá trị gốc<Input value={raw} maxLength={4096} onChange={e=>setRaw(e.target.value)}/></label>
 <label>Nhãn lựa chọn<Input value={label} maxLength={120} onChange={e=>setLabel(e.target.value)}/></label>
 <Button type="button" disabled={choices.length>=100} onClick={add}>Thêm lựa chọn</Button></div>
}
export function AttributeFormFields({client,org,pid,scope,layer,layers,values,change,rules,disabled}:{client:ApiClient;org:string;pid:string;scope:string;layer:SpatialLayer;layers:SpatialLayer[];values:FormValues;change:(id:string,v:FormValues[string])=>void;rules:ProjectRelation[];disabled:boolean}){
 const uid=useId(),fields=layer.source.schema.fields,layout=readFormLayout(formStorageKey(scope,layer),fields);
 const {errors}=validateForm(fields,values);
 return <div className="ovg-attribute-form">{formGroups(layout,fields).map(group=><fieldset key={group.name} disabled={disabled}><legend>{group.name||"Thuộc tính"}</legend>{group.items.map(({field,layout})=>{
 const k=field.field_id,v=values[k]??{raw:"",nil:false},id=uid+k,label=layout.label.trim()||field.name;
 const describedBy=[layout.hint?id+"-hint":"",errors[k]?id+"-error":""].filter(Boolean).join(" ")||undefined;
 const update=(raw:string)=>change(k,{raw,nil:false});
 return <div key={k} className="ovg-form-field"><label htmlFor={id}>{label}{field.nullable?"":" *"}</label>
 {field.type==="boolean"?<Select id={id} aria-label={field.name} aria-describedby={describedBy} aria-invalid={!!errors[k]} aria-required={!field.nullable} disabled={v.nil} value={v.raw} onChange={e=>update(e.target.value)}><option value="">Chọn</option><option value="true">true</option><option value="false">false</option></Select>:layout.widget==="multiline"?<Textarea id={id} aria-label={field.name} aria-describedby={describedBy} aria-invalid={!!errors[k]} aria-required={!field.nullable} disabled={v.nil} value={v.raw} rows={3} maxLength={4096} onChange={e=>update(e.target.value)}/>:<Input id={id} aria-label={field.name} aria-describedby={describedBy} aria-invalid={!!errors[k]} aria-required={!field.nullable} disabled={v.nil} inputMode={field.type==="integer"?"numeric":field.type==="decimal"?"decimal":"text"} placeholder={field.type==="timestamp"?"2026-01-01T00:00:00Z":field.type} value={v.raw} maxLength={4096} onChange={e=>update(e.target.value)}/>}
 {layout.widget==="value-map"&&!!layout.choices?.length&&<label>Danh sách giá trị<Select aria-label={"Danh sách "+field.name} disabled={v.nil||disabled} value={String(valueMapSelection(field,v.raw,layout.choices))} onChange={e=>{const index=Number(e.target.value),choice=layout.choices?.[index];if(choice)update(choice.value)}}><option value="-1">Nhập thủ công / ngoài danh sách</option>{layout.choices.map((c,i)=><option key={c.value} value={i}>{c.label||c.value||"(chuỗi rỗng)"}</option>)}</Select></label>}
 {field.nullable&&<label><input type="checkbox" aria-label={"NULL "+field.name} checked={v.nil} onChange={e=>change(k,{...v,nil:e.target.checked})}/> NULL</label>}
 {layout.hint&&<small id={id+"-hint"}>{layout.hint}</small>}{errors[k]&&<small id={id+"-error"} className="error" role="status">{errors[k]}</small>}
 <RelationValuePicker client={client} org={org} pid={pid} source={layer} layers={layers} field={k} rules={rules} disabled={disabled} choose={update}/>
 </div>})}</fieldset>)}</div>;
}
