import React,{type ButtonHTMLAttributes,type InputHTMLAttributes,type HTMLAttributes,type SelectHTMLAttributes,type TextareaHTMLAttributes} from "react";
/** Opt-in primitives bridge legacy CSS and the prefixed Tailwind compiler. */
export function Button({className="",variant="default",type="button",...props}:ButtonHTMLAttributes<HTMLButtonElement>&{variant?:"default"|"primary"|"danger"}){
 return <button type={type} className={"ovg-ui-button ovg:min-w-0 "+(variant==="default"?"":variant+" ")+className} {...props}/>;
}
export function Input({className="",...props}:InputHTMLAttributes<HTMLInputElement>){
 return <input className={"ovg-ui-input ovg:min-w-0 ovg:max-w-full "+className} {...props}/>;
}
export function Actions({className="",...props}:HTMLAttributes<HTMLDivElement>){
 return <div className={"ovg-ui-actions ovg:min-w-0 "+className} {...props}/>;
}
export function TableScroll({className="",...props}:HTMLAttributes<HTMLDivElement>){
 return <div className={"ovg-ui-table-scroll "+className} {...props}/>;
}

export function Select({className="",...props}:SelectHTMLAttributes<HTMLSelectElement>){return <select className={"ovg-ui-input ovg:min-w-0 ovg:max-w-full "+className} {...props}/>}
export function Textarea({className="",...props}:TextareaHTMLAttributes<HTMLTextAreaElement>){return <textarea className={"ovg-ui-input ovg:min-w-0 ovg:max-w-full "+className} {...props}/>}
export function ScrollArea({className="",...props}:HTMLAttributes<HTMLDivElement>){return <div className={"ovg-ui-scroll "+className} {...props}/>}
export const TableScrollRef=React.forwardRef<HTMLDivElement,HTMLAttributes<HTMLDivElement>>(function TableScrollRef({className="",...props},ref){return <div ref={ref} className={"ovg-ui-table-scroll "+className} {...props}/>});
