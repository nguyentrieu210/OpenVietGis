import type {ReactNode} from 'react';
import {Dialog as BaseDialog} from '@base-ui/react/dialog';
import {Menu as BaseMenu} from '@base-ui/react/menu';
import {Popover as BasePopover} from '@base-ui/react/popover';
import {Tooltip as BaseTooltip} from '@base-ui/react/tooltip';
import {Button} from './primitives';
export type DialogSize='xs'|'sm'|'md'|'lg'|'xl'|'workbench';
export function Dialog({open,onOpenChange,title,description,children,footer,size='md',closeLabel='Close',closeOnBackdrop=true}:{open:boolean;onOpenChange:(open:boolean)=>void;title:string;description?:string;children:ReactNode;footer?:ReactNode;size?:DialogSize;closeLabel?:string;closeOnBackdrop?:boolean}){
 return <BaseDialog.Root open={open} onOpenChange={(next,details)=>{if(!next&&!closeOnBackdrop&&details.reason==='outside-press')return;onOpenChange(next)}}><BaseDialog.Portal><BaseDialog.Backdrop className="ovg-v2-backdrop"/><BaseDialog.Popup className="ovg-v2-dialog" data-size={size}><header className="ovg-v2-dialog-header"><BaseDialog.Title className="ovg-v2-title">{title}</BaseDialog.Title><BaseDialog.Close render={<Button variant="ghost" aria-label={closeLabel}/>}>×</BaseDialog.Close></header><div className="ovg-v2-dialog-body">{description&&<BaseDialog.Description className="ovg-v2-description">{description}</BaseDialog.Description>}{children}</div>{footer&&<footer className="ovg-v2-dialog-footer">{footer}</footer>}</BaseDialog.Popup></BaseDialog.Portal></BaseDialog.Root>;
}
export type MenuItem={id:string;label:string;disabled?:boolean;onSelect:()=>void};
export function Menu({label,items}:{label:string;items:readonly MenuItem[]}){
 return <BaseMenu.Root><BaseMenu.Trigger render={<Button/>}>{label}</BaseMenu.Trigger><BaseMenu.Portal><BaseMenu.Positioner className="ovg-v2-positioner" sideOffset={4}><BaseMenu.Popup className="ovg-v2-popup">{items.map(i=><BaseMenu.Item className="ovg-v2-menu-item" key={i.id} disabled={i.disabled} onClick={i.onSelect}>{i.label}</BaseMenu.Item>)}</BaseMenu.Popup></BaseMenu.Positioner></BaseMenu.Portal></BaseMenu.Root>;
}
export function Popover({label,title,children}:{label:string;title:string;children:ReactNode}){
 return <BasePopover.Root><BasePopover.Trigger render={<Button/>}>{label}</BasePopover.Trigger><BasePopover.Portal><BasePopover.Positioner className="ovg-v2-positioner" sideOffset={4}><BasePopover.Popup className="ovg-v2-popup ovg-v2-popover"><BasePopover.Title className="ovg-v2-title">{title}</BasePopover.Title>{children}</BasePopover.Popup></BasePopover.Positioner></BasePopover.Portal></BasePopover.Root>;
}
export function Tooltip({label,children}:{label:string;children:ReactNode}){
 return <BaseTooltip.Provider><BaseTooltip.Root><BaseTooltip.Trigger render={<Button/>}>{children}</BaseTooltip.Trigger><BaseTooltip.Portal><BaseTooltip.Positioner className="ovg-v2-positioner" sideOffset={4}><BaseTooltip.Popup role="tooltip" className="ovg-v2-popup ovg-v2-tooltip">{label}</BaseTooltip.Popup></BaseTooltip.Positioner></BaseTooltip.Portal></BaseTooltip.Root></BaseTooltip.Provider>;
}