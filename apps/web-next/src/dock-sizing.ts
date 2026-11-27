export function clampSize(value:number,min:number,max:number):number{
 const upper=Math.max(0,max);
 const lower=Math.min(Math.max(0,min),upper);
 return Math.max(lower,Math.min(upper,Number.isFinite(value)?value:lower));
}
