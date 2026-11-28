/** Bounded source catalogue paging, rejecting repeated cursors and duplicate rows. */
export type SourcePage<T>={items:T[];next_after_id:string|null};
export const SOURCE_BROWSER_LIMIT=1000;
export const SOURCE_BROWSER_PAGE_LIMIT=50;
export const SOURCE_BROWSER_SCAN_LIMIT=5000;
export function mergeSourcePage<T extends {id:string;lifecycle:string}>(previous:readonly T[],page:SourcePage<T>,cursor:string|null,seen:ReadonlySet<string>,seenRows:ReadonlySet<string>=new Set(previous.map(x=>x.id))){
 if(!Array.isArray(page.items)||page.items.length>100||!(page.next_after_id===null||typeof page.next_after_id==="string"&&!!page.next_after_id&&page.next_after_id.length<=512))throw Error("Trang nguồn không hợp lệ.");
 if(page.next_after_id!==null&&(page.next_after_id===cursor||seen.has(page.next_after_id)||!page.items.length))throw Error("Cursor nguồn không tiến triển.");
 const result=[...previous],ids=new Set(seenRows),incoming=new Set<string>();
 for(const item of page.items){
  if(typeof item?.id!=="string"||!item.id||item.id.length>512||typeof item.lifecycle!=="string"||ids.has(item.id)||incoming.has(item.id))throw Error("Nguồn bị trùng hoặc không hợp lệ trong danh mục.");
  incoming.add(item.id);if(item.lifecycle==="active")result.push(item);
 }
 if(result.length>SOURCE_BROWSER_LIMIT||ids.size+incoming.size>SOURCE_BROWSER_SCAN_LIMIT)throw Error("Danh mục vượt giới hạn hiển thị. Dùng Data Source Manager.");
 const scanned=new Set([...ids,...incoming]);
 const limit=page.next_after_id!==null&&(result.length>=SOURCE_BROWSER_LIMIT||seen.size+1>=SOURCE_BROWSER_PAGE_LIMIT||scanned.size>=SOURCE_BROWSER_SCAN_LIMIT);
 return {items:result,next:page.next_after_id,limit,scanned};
}
