/** Memory-only drafts. The owner must be remounted when account/project scope changes. */
export class RoomDraftStore<T> {
 private entries=new Map<string,T>();
 private pending=new Set<string>();
 constructor(private readonly empty:()=>T,private readonly limit=20){}
 get(room:string):T{return this.entries.get(room)??this.empty()}
 update(room:string,fn:(draft:T)=>T):T{
  if(!room)return this.empty();
  const value=fn(this.get(room));this.entries.delete(room);this.entries.set(room,value);
  while(this.entries.size>this.limit){const first=this.entries.keys().next().value;if(first===undefined)break;this.entries.delete(first)}
  return value
 }
 claim(room:string){if(!room||this.pending.has(room))return false;this.pending.add(room);return true}
 release(room:string){this.pending.delete(room)}
 remove(room:string){this.entries.delete(room)}
 retain(allowed:ReadonlySet<string>){for(const room of this.entries.keys())if(!allowed.has(room))this.entries.delete(room)}
 clear(){this.entries.clear()}
}
export function mergeChatWindow<T extends {id:string;created_at:string}>(old:T[],latest:T[]):T[]{
 if(!latest.length)return [];
 const sorted=[...latest].sort((a,b)=>a.created_at.localeCompare(b.created_at)||a.id.localeCompare(b.id));
 const first=sorted[0]!,ids=new Set(sorted.map(m=>m.id));
 return [...old.filter(m=>!ids.has(m.id)&&(m.created_at<first.created_at||m.created_at===first.created_at&&m.id<first.id)),...sorted].slice(-1000)
}
