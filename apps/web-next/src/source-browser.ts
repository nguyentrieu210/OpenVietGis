/** Bounded source catalogue paging, rejecting repeated cursors and duplicate rows. */
export type SourcePage<T>={items:T[];next_after_id:string|null};
export const SOURCE_BROWSER_LIMIT=1000;
export function mergeSourcePage<T extends {id:string;lifecycle:string}>(previous:readonly T[],page:SourcePage<T>,cursor:string|null,seen:ReadonlySet<string>){
 if(!Array.isArray(page.items)||page.items.length>100||!(page.next_after_id===null||typeof page.next_after_id==="string"&&!!page.next_after_id))throw Error("Trang nguồn không hợp lệ.");
 if(page.next_after_id!==null&&(page.next_after_id===cursor||seen.has(page.next_after_id)))throw Error("Cursor nguồn bị lặp.");
 const result=[...previous],ids=new Set(previous.map(x=>x.id)),incoming=new Set<string>();
 for(const item of page.items){if(typeof item?.id!=="string"||!item.id||ids.has(item.id)||incoming.has(item.id))throw Error("Nguồn bị trùng trong danh mục.");incoming.add(item.id);if(item.lifecycle==="active")result.push(item)}
 if(result.length>SOURCE_BROWSER_LIMIT)throw Error("Danh mục vượt giới hạn hiển thị. Dùng Data Source Manager.");
 return {items:result,next:page.next_after_id,limit:result.length===SOURCE_BROWSER_LIMIT&&page.next_after_id!==null};
}
