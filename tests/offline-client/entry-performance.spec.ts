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
