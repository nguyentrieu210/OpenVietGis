import {forwardRef,useId,type ReactNode,type ComponentPropsWithoutRef} from 'react';
import {Checkbox as BaseCheckbox} from '@base-ui/react/checkbox';
import {Switch as BaseSwitch} from '@base-ui/react/switch';
import {Tabs as BaseTabs} from '@base-ui/react/tabs';
export const cx=(...values:(string|false|undefined|null)[])=>values.filter(Boolean).join(' ');
export type Variant='default'|'primary'|'danger'|'ghost';
export const Button=forwardRef<HTMLButtonElement,ComponentPropsWithoutRef<'button'>&{variant?:Variant}>(
 function Button({variant='default',type='button',className,...props},ref){return <button ref={ref} type={type} className={cx('ovg-v2-button','ovg-v2-button-'+variant,className)} {...props}/>});
export const Input=forwardRef<HTMLInputElement,ComponentPropsWithoutRef<'input'>>(function Input({className,...props},ref){return <input ref={ref} className={cx('ovg-v2-input',className)} {...props}/>});
export const Select=forwardRef<HTMLSelectElement,ComponentPropsWithoutRef<'select'>>(function Select({className,...props},ref){return <select ref={ref} className={cx('ovg-v2-input',className)} {...props}/>});
export const Textarea=forwardRef<HTMLTextAreaElement,ComponentPropsWithoutRef<'textarea'>>(function Textarea({className,...props},ref){return <textarea ref={ref} className={cx('ovg-v2-input',className)} {...props}/>});
export function Field({label,hint,error,children}:{label:string;hint?:string;error?:string;children:(attrs:{id:string;'aria-describedby'?:string;'aria-invalid'?:true})=>ReactNode}){
 const id=useId(),hintId=id+'-hint',errorId=id+'-error';const described=[hint?hintId:'',error?errorId:''].filter(Boolean).join(' ');
 return <div className="ovg-v2-field"><label htmlFor={id}>{label}</label>{children({id,'aria-describedby':described||undefined,'aria-invalid':error?true:undefined})}{hint&&<small id={hintId}>{hint}</small>}{error&&<span id={errorId} role="alert" className="ovg-v2-error">{error}</span>}</div>;
}
export function Actions({className,...props}:ComponentPropsWithoutRef<'div'>){return <div className={cx('ovg-v2-actions',className)} {...props}/>}
export function ScrollArea({className,...props}:ComponentPropsWithoutRef<'div'>){return <div className={cx('ovg-v2-scroll',className)} {...props}/>}
export function TableScroll({className,...props}:ComponentPropsWithoutRef<'div'>){return <div className={cx('ovg-v2-table-scroll',className)} {...props}/>}
export function Checkbox({label,checked,onChange,disabled}:{label:string;checked:boolean;onChange:(value:boolean)=>void;disabled?:boolean}){
 return <label className="ovg-v2-check-label"><BaseCheckbox.Root className="ovg-v2-checkbox" checked={checked} onCheckedChange={onChange} disabled={disabled}><BaseCheckbox.Indicator>✓</BaseCheckbox.Indicator></BaseCheckbox.Root>{label}</label>;
}
export function Switch({label,checked,onChange,disabled}:{label:string;checked:boolean;onChange:(value:boolean)=>void;disabled?:boolean}){
 return <label className="ovg-v2-check-label"><BaseSwitch.Root className="ovg-v2-switch" checked={checked} onCheckedChange={onChange} disabled={disabled}><BaseSwitch.Thumb className="ovg-v2-thumb"/></BaseSwitch.Root>{label}</label>;
}
export function Tabs({label,items,value,onChange}:{label:string;items:readonly {id:string;label:string;content:ReactNode;disabled?:boolean}[];value:string;onChange:(value:string)=>void}){
 return <BaseTabs.Root value={value} onValueChange={v=>{if(typeof v==='string')onChange(v)}}><BaseTabs.List aria-label={label} className="ovg-v2-tabs">{items.map(i=><BaseTabs.Tab className="ovg-v2-tab" key={i.id} value={i.id} disabled={i.disabled}>{i.label}</BaseTabs.Tab>)}</BaseTabs.List>{items.map(i=><BaseTabs.Panel className="ovg-v2-tabpanel" key={i.id} value={i.id}>{i.content}</BaseTabs.Panel>)}</BaseTabs.Root>;
}
