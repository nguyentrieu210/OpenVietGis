import {useEffect,useState} from "react";
import {cleanPreferences,defaults,preferenceKey,writePreferences,type Preferences} from "@openvietgis/ui-react/preferences/model";
export type ThemePreference=Preferences["theme"];
export const normalizeTheme=(v:unknown):ThemePreference=>v==="dark"||v==="system"?v:"light";
export const resolveTheme=(v:ThemePreference,dark:boolean)=>v==="system"?(dark?"dark":"light"):v;
const key=(account:string)=>"ovg:ui-theme:v1:"+account;
export function readUISettings(storage:Pick<Storage,"getItem">,account:string):Preferences{
 try{const raw=storage.getItem(preferenceKey(account));if(raw!==null){try{const parsed=JSON.parse(raw);if(parsed&&typeof parsed==="object"&&!Array.isArray(parsed))return {...cleanPreferences(parsed),locale:"vi"}}catch{}}
 return {...defaults,theme:normalizeTheme(storage.getItem(key(account))),locale:"vi"};
 }catch{return {...defaults,theme:"light"}}
}
export function applyUISettings(root:HTMLElement,p:Preferences,dark:boolean){
 root.dataset.ovgTheme=resolveTheme(p.theme,dark);root.dataset.ovgDensity=p.density;root.dataset.ovgContrast=p.contrast;root.dataset.ovgMotion=p.motion;
}
export function useUITheme(account:string){
 const read=()=>readUISettings(localStorage,account);
 const [state,setState]=useState<{account:string;value:Preferences}>(()=>({account,value:read()}));
 const settings=state.account===account?state.value:read();
 const {theme,density,contrast,motion}=settings;
 useEffect(()=>{const mq=matchMedia("(prefers-color-scheme: dark)"),apply=()=>applyUISettings(document.documentElement,{theme,density,contrast,motion,locale:"vi"},mq.matches);apply();mq.addEventListener("change",apply);return()=>{mq.removeEventListener("change",apply);for(const k of ["ovgTheme","ovgDensity","ovgContrast","ovgMotion"])delete document.documentElement.dataset[k]}},[account,theme,density,contrast,motion]);
 const changeSettings=(patch:Partial<Pick<Preferences,"theme"|"density"|"contrast"|"motion">>)=>{const v={...cleanPreferences({...settings,...patch}),locale:"vi" as const};setState({account,value:v});writePreferences(localStorage,account,v)};
 return {value:theme,settings,change:(next:ThemePreference)=>changeSettings({theme:normalizeTheme(next)}),changeSettings};
}