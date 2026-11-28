export function clampSize(value:number,min:number,max:number):number{
 const upper=Math.max(0,max);
 const lower=Math.min(Math.max(0,min),upper);
 return Math.max(lower,Math.min(upper,Number.isFinite(value)?value:lower));
}

/** Keyboard equivalents for a dock separator, respecting its edge and current viewport. */
export function separatorKeySize(key:string,axis:"x"|"y",edge:"start"|"end",size:number,min:number,max:number,step=20):number|null{
 if(key==="Home")return clampSize(min,min,max);
 if(key==="End")return clampSize(max,min,max);
 const grow=axis==="x"?(edge==="start"?"ArrowLeft":"ArrowRight"):(edge==="start"?"ArrowUp":"ArrowDown");
 const shrink=axis==="x"?(edge==="start"?"ArrowRight":"ArrowLeft"):(edge==="start"?"ArrowDown":"ArrowUp");
 if(key!==grow&&key!==shrink)return null;
 return clampSize(size+(key===grow?step:-step),min,max);
}
