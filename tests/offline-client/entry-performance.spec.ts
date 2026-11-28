import {test,expect} from "@playwright/test";
import {readFileSync} from "node:fs";
const manifest=JSON.parse(readFileSync("apps/web-next/dist/.vite/manifest.json","utf8"));
test("public landing does not request the protected GIS workspace or optional engines",async({page,context})=>{
 const loaded=new Set<string>();
 context.on("request",r=>loaded.add(new URL(r.url()).pathname));
 await page.goto("/");
 await expect(page.getByRole("heading",{name:/GIS cho mọi người/i})).toBeVisible();
 await page.evaluate(async()=>{await navigator.serviceWorker.ready});
 const prohibited=Object.keys(manifest).filter(k=>k==="src/LiveApp.tsx"||k==="src/TerrainEngineering.tsx"||k==="src/ProcessingModeler.tsx"||k==="src/RasterWorkbench.tsx");
 expect(prohibited.length).toBeGreaterThan(1);
 for(const k of prohibited) expect(loaded.has("/"+manifest[k].file),k).toBe(false);
 const cached=await page.evaluate(async()=>{
   const names=await caches.keys();const paths:string[]=[];
   for(const name of names)if(name.startsWith("ovg-offline-shell-"))for(const req of await (await caches.open(name)).keys())paths.push(new URL(req.url).pathname);
   return paths;
 });
 for(const k of prohibited) expect(cached).not.toContain("/"+manifest[k].file);
 expect(cached.every(p=>p==="/index.html"||p.startsWith("/assets/"))).toBe(true);
 expect(cached).not.toContain("/api/v1/me");
});

test("service-worker upgrade deletes only prior shell caches and preserves unrelated storage",async({page})=>{
 await page.goto("/");
 await page.evaluate(async()=>{for(const r of await navigator.serviceWorker.getRegistrations())await r.unregister();for(const k of await caches.keys())await caches.delete(k)});
 await page.evaluate(async()=>{
   const old=await caches.open("ovg-offline-shell-old-build");
   await old.put("/index.html",new Response("old"));
   const unrelated=await caches.open("fixture-unrelated-cache");
   await unrelated.put("/api/v1/private-fixture",new Response("private sentinel"));
   await new Promise((resolve,reject)=>{
     const r=indexedDB.open("fixture-private-idb",1);
     r.onupgradeneeded=()=>r.result.createObjectStore("drafts");
     r.onsuccess=()=>{const tx=r.result.transaction("drafts","readwrite");tx.objectStore("drafts").put("draft-value","draft");tx.oncomplete=()=>{r.result.close();resolve(null)};tx.onerror=()=>reject(tx.error)};
     r.onerror=()=>reject(r.error);
   });
 });
 await page.reload();
 await page.evaluate(async()=>{await navigator.serviceWorker.ready});
 await expect.poll(async()=>page.evaluate(async()=>(await caches.keys()).filter(k=>k.startsWith("ovg-offline-shell-")).length)).toBe(1);
 const state=await page.evaluate(async()=>{
   const names=await caches.keys(),unrelated=await (await caches.open("fixture-unrelated-cache")).match("/api/v1/private-fixture");
   const draft=await new Promise((resolve,reject)=>{
     const r=indexedDB.open("fixture-private-idb");
     r.onsuccess=()=>{const tx=r.result.transaction("drafts","readonly"),g=tx.objectStore("drafts").get("draft");g.onsuccess=()=>{r.result.close();resolve(g.result)};g.onerror=()=>reject(g.error)};
     r.onerror=()=>reject(r.error);
   });
   return {names,unrelated:await unrelated?.text(),draft};
 });
 expect(state.names).not.toContain("ovg-offline-shell-old-build");
 expect(state.names).toContain("fixture-unrelated-cache");
 expect(state.unrelated).toBe("private sentinel");
 expect(state.draft).toBe("draft-value");
});


test("special entry routes do not bootstrap the authenticated workspace",async({browser})=>{
 const m=manifest as Record<string,{file:string}>;
 const workspaceFiles=["src/SessionEntry.tsx","src/LiveApp.tsx"].map(k=>m[k]?.file).filter(Boolean);
 expect(workspaceFiles).toHaveLength(2);
 for(const query of ["?qgis_connect=1","?auth_complete=1","?marketplace=1","?developer=1"]){
   const context=await browser.newContext({serviceWorkers:"block"}),loaded=new Set<string>();
   context.on("request",r=>loaded.add(new URL(r.url()).pathname));
   const page=await context.newPage();
   await page.goto("/"+query,{waitUntil:"domcontentloaded"});
   await page.waitForTimeout(100);
   for(const file of workspaceFiles)expect(loaded.has("/"+file),query+" loaded "+file).toBe(false);
   await context.close();
 }
});
