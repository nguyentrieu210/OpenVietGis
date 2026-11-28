import {useEffect,useState} from "react";
export type ThemePreference="light"|"dark"|"system";
export const normalizeTheme=(v:unknown):ThemePreference=>v==="dark"||v==="system"?v:"light";
export const resolveTheme=(v:ThemePreference,dark:boolean)=>v==="system"?(dark?"dark":"light"):v;
const key=(account:string)=>"ovg:ui-theme:v1:"+account;
export function useUITheme(account:string){
 const read=()=>{try{return normalizeTheme(localStorage.getItem(key(account)))}catch{return "light" as const}};
 const [state,setState]=useState<{account:string;value:ThemePreference}>(()=>({account,value:read()}));
 const value=state.account===account?state.value:read();
 useEffect(()=>{const mq=matchMedia("(prefers-color-scheme: dark)"),apply=()=>{document.documentElement.dataset.ovgTheme=resolveTheme(value,mq.matches)};apply();mq.addEventListener("change",apply);return()=>{mq.removeEventListener("change",apply);delete document.documentElement.dataset.ovgTheme}},[account,value]);
 return {value,change:(next:ThemePreference)=>{const v=normalizeTheme(next);setState({account,value:v});try{localStorage.setItem(key(account),v)}catch{}}};
}
