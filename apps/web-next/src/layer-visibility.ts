export type VisibilityLayer={id:string;group?:string|undefined};
export type VisibilityPolicy={layers:readonly VisibilityLayer[];exclusive:readonly string[]};
const belongs=(group:string|undefined,path:string)=>!!group&&(group===path||group.startsWith(path+"/"));
export function cleanExclusivePaths(raw:unknown):string[]{
 if(!Array.isArray(raw))return [];
 return [...new Set(raw.filter((x):x is string=>typeof x==="string"&&x.length<=200&&x.split("/").length<=4&&x.split("/").every(p=>p.length>0&&p===p.trim()&&!/[\x00-\x1f\x7f]/.test(p))))].slice(0,50);
}
export function enforceExclusive(hidden:ReadonlySet<string>,policy:VisibilityPolicy,preferred:readonly string[]=[]):Set<string>{
 const next=new Set(hidden),rank=new Map(preferred.map((id,i)=>[id,i]));
 for(const path of cleanExclusivePaths(policy.exclusive).sort((a,b)=>a.split("/").length-b.split("/").length||a.localeCompare(b))){
  const visible=policy.layers.filter(l=>belongs(l.group,path)&&!next.has(l.id));if(visible.length<=1)continue;
  const winner=visible.reduce((a,b)=>(rank.get(b.id)??Infinity)<(rank.get(a.id)??Infinity)?b:a);
  for(const l of visible)if(l.id!==winner.id)next.add(l.id);
 }
 return next;
}
export const equalVisibility=(a:ReadonlySet<string>,b:ReadonlySet<string>)=>a.size===b.size&&[...a].every(id=>b.has(id));
export function changeVisibility(hidden:ReadonlySet<string>,policy:VisibilityPolicy,show:readonly string[],hide:readonly string[]){
 const next=new Set(hidden),known=new Set(policy.layers.map(l=>l.id));
 for(const id of hide)if(known.has(id))next.add(id);
 for(const id of show)if(known.has(id))next.delete(id);
 return enforceExclusive(next,policy,show);
}
export function groupVisibilityState(ids:readonly string[],hidden:ReadonlySet<string>,policy:VisibilityPolicy){
 const current=ids.filter(id=>!hidden.has(id));
 const want=changeVisibility(hidden,policy,[...current,...ids.filter(id=>hidden.has(id))],[]);
 const complete=ids.length>0&&ids.every(id=>hidden.has(id)===want.has(id))&&current.length>0;
 return {checked:complete,mixed:!complete&&current.length>0,show:complete?[]:ids.filter(id=>!want.has(id)),hide:complete?[...ids]:ids.filter(id=>want.has(id))};
}
export function remapExclusivePaths(paths:readonly string[],from:string,to:string|null,keepRoot=true){
 return cleanExclusivePaths(paths.flatMap(p=>p===from?(to&&keepRoot?[to]:[]):p.startsWith(from+"/")?[to?to+p.slice(from.length):p.slice(from.length+1)]:[p]));
}
