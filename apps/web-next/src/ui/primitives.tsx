import React,{type ButtonHTMLAttributes,type InputHTMLAttributes,type HTMLAttributes} from "react";
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
