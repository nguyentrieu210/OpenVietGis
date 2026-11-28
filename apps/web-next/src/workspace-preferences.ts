// Personal UI preferences only: never migrate unscoped values from another account.
export const preferenceKey=(account:string,org:string,project:string,section:string)=>
 "ovg:ui:v1:"+[account,org,project,section].map(encodeURIComponent).join(":");
export function readCommandIds(raw:string|null):string[]{
 try{const value=JSON.parse(raw??"null");return Array.isArray(value)?[...new Set(value.filter((x):x is string=>typeof x==="string"&&/^[A-Z][A-Z0-9_-]{0,63}$/.test(x)))].slice(0,50):[]}catch{return []}
}
export function recordRecent(previous:readonly string[],id:string):string[]{
 return [id,...previous.filter(x=>x!==id)].slice(0,20);
}
export function fitFloatingPanel(x:number,y:number,width:number,height:number,viewportWidth:number,viewportHeight:number){
 const w=Math.max(0,Math.min(width,viewportWidth)),h=Math.max(0,Math.min(height,viewportHeight));
 return {x:Math.max(0,Math.min(viewportWidth-w,Number.isFinite(x)?x:0)),y:Math.max(0,Math.min(viewportHeight-h,Number.isFinite(y)?y:0)),width:w,height:h};
}
