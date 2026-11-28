import React,{type ButtonHTMLAttributes,type InputHTMLAttributes,type HTMLAttributes,type SelectHTMLAttributes,type TextareaHTMLAttributes} from "react";
/** Opt-in primitives bridge legacy CSS and the prefixed Tailwind compiler. */
export const Button=React.forwardRef<HTMLButtonElement,ButtonHTMLAttributes<HTMLButtonElement>&{variant?:"default"|"primary"|"danger"}>(function Button({className="",variant="default",type="button",...props},ref){
 return <button ref={ref} type={type} className={"ovg-ui-button ovg:min-w-0 "+(variant==="default"?"":variant+" ")+className} {...props}/>;
});
export const Input=React.forwardRef<HTMLInputElement,InputHTMLAttributes<HTMLInputElement>>(function Input({className="",...props},ref){
 return <input ref={ref} className={"ovg-ui-input ovg:min-w-0 ovg:max-w-full "+className} {...props}/>;
});
export function Actions({className="",...props}:HTMLAttributes<HTMLDivElement>){
 return <div className={"ovg-ui-actions ovg:min-w-0 "+className} {...props}/>;
}
export function TableScroll({className="",...props}:HTMLAttributes<HTMLDivElement>){
 return <div className={"ovg-ui-table-scroll "+className} {...props}/>;
}

export const Select=React.forwardRef<HTMLSelectElement,SelectHTMLAttributes<HTMLSelectElement>>(function Select({className="",...props},ref){return <select ref={ref} className={"ovg-ui-input ovg:min-w-0 ovg:max-w-full "+className} {...props}/>});
export const Textarea=React.forwardRef<HTMLTextAreaElement,TextareaHTMLAttributes<HTMLTextAreaElement>>(function Textarea({className="",...props},ref){return <textarea ref={ref} className={"ovg-ui-input ovg:min-w-0 ovg:max-w-full "+className} {...props}/>});
export function ScrollArea({className="",...props}:HTMLAttributes<HTMLDivElement>){return <div className={"ovg-ui-scroll "+className} {...props}/>}
export const TableScrollRef=React.forwardRef<HTMLDivElement,HTMLAttributes<HTMLDivElement>>(function TableScrollRef({className="",...props},ref){return <div ref={ref} className={"ovg-ui-table-scroll "+className} {...props}/>});

export function Tabs<T extends string>({tabs,tab,set,className="",label="Phân hệ",orientation="horizontal",icons}:{tabs:readonly (readonly [T,string])[];tab:T;set:(v:T)=>void;className?:string;label?:string;orientation?:"horizontal"|"vertical";icons?:readonly React.ReactNode[]}){
 return <div className={"ovg-ui-tabs "+className} role="tablist" aria-label={label} aria-orientation={orientation} onKeyDown={e=>{const at=tabs.findIndex(([id])=>id===tab);if(!["ArrowLeft","ArrowRight","ArrowUp","ArrowDown","Home","End"].includes(e.key)||!tabs.length)return;if(orientation==="horizontal"&&(e.key==="ArrowUp"||e.key==="ArrowDown"))return;e.preventDefault();const i=e.key==="Home"?0:e.key==="End"?tabs.length-1:(Math.max(0,at)+(e.key==="ArrowRight"||e.key==="ArrowDown"?1:tabs.length-1))%tabs.length;set(tabs[i][0]);e.currentTarget.querySelectorAll<HTMLButtonElement>('[role=tab]')[i]?.focus()}}>{tabs.map(([id,name],i)=><Button key={id} role="tab" aria-label={name} tabIndex={id===tab?0:-1} aria-selected={id===tab} className={id===tab?"active":""} onClick={()=>set(id)}>{icons?.[i]&&<span aria-hidden="true">{icons[i]}</span>}{name}</Button>)}</div>
}
export function Field({label,children,className="",...props}:React.LabelHTMLAttributes<HTMLLabelElement>&{label:React.ReactNode}){return <label {...props} className={"ovg-ui-field "+className}><span>{label}</span>{children}</label>}
