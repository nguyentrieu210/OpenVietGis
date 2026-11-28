import {test,expect} from "@playwright/test";
import {readFileSync} from "node:fs";
const buildManifest=JSON.parse(readFileSync("apps/web-next/dist/.vite/manifest.json","utf8")) as Record<string,{file:string}>;
const prefix="/api/v1", project=prefix+"/organizations/org1/projects/prj1";
async function setup(page:import("@playwright/test").Page,restricted=false,resumeCad=true,state?:{denyMe:boolean}){
 const requests:string[]=[];
 if(resumeCad)await page.addInitScript(()=>localStorage.setItem("ovg:resume-mode:v1:actor1:org1:prj1","cad"));
 await page.route("**/api/v1/**",async route=>{
  const path=new URL(route.request().url()).pathname;requests.push(path);
  let data:unknown={items:[]};
  if(path===prefix+"/me"){if(state?.denyMe)return route.fulfill({status:401,json:{code:"UNAUTHENTICATED"}});data={id:"actor1",status:"active",display_name:"Fixture",email_snapshot:null};}
  else if(path===prefix+"/auth/providers")data=[];
  else if(path===prefix+"/me/organizations")data=[{id:"org1",name:"Org",slug:"org",role:"viewer",status:"active"}];
  else if(path===prefix+"/organizations/org1/projects")data=[{id:"prj1",organization_id:"org1",workspace_id:null,name:"Project",slug:"project",status:"active"}];
  else if(path===prefix+"/organizations/org1/effective-permissions")data={permissions:["project.read"]};
  else if(path===project+"/access/mine")data={restricted};
  else if(path===project+"/cad/documents")data={items:[]};
  else if(path===project+"/layers")data={items:[]};
  else if(path===project+"/map")data={configuration_revision:1,configuration:{layers:[]}};
  await route.fulfill({json:data,headers:{"Cache-Control":"no-store"}});
 });
 await page.route("https://**",r=>r.abort());
 return requests;
}
test("saved CAD opens without initializing a GIS canvas or reading GIS layers; GIS starts on demand",async({page})=>{
 const requests=await setup(page);
 await page.goto("/app");
 await expect(page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"})).toBeVisible();
 await expect(page.locator(".ovg-spatial-workspace")).toHaveAttribute("data-workspace-mode","cad");
 expect(requests).toContain(project+"/cad/documents");
 expect(requests).not.toContain(project+"/layers");
 expect(requests).not.toContain(project+"/map");
 expect(requests).not.toContain(project+"/raster/layers");
 await expect(page.locator(".maplibregl-canvas")).toHaveCount(0);
 requests.length=0;
 await page.reload();
 await expect(page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"})).toBeVisible();
 expect(requests).not.toContain(project+"/layers");
 expect(requests).not.toContain(project+"/raster/layers");
 await expect(page.locator(".maplibregl-canvas")).toHaveCount(0);
 await page.getByRole("group",{name:"Không gian làm việc"}).getByRole("button",{name:"GIS",exact:true}).click();
 await expect(page.locator(".ovg-spatial-workspace")).toHaveAttribute("data-workspace-mode","gis");
 await expect.poll(()=>requests.includes(project+"/layers")).toBe(true);
});
test("saved CAD preference cannot bypass restricted project authorization",async({page})=>{
 const requests=await setup(page,true);
 await page.goto("/app");
 await expect(page.locator(".ovg-spatial-workspace")).toHaveAttribute("data-workspace-mode","gis");
 await expect(page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"})).toHaveCount(0);
 expect(requests).not.toContain(project+"/cad/documents");
});


test("default GIS does not eagerly initialize CAD or fetch the optional Cesium engine",async({page,context})=>{
 const loaded=new Set<string>();context.on("request",r=>loaded.add(new URL(r.url()).pathname));
 const requests=await setup(page,false,false);
 await page.goto("/app");
 await expect(page.locator(".ovg-spatial-workspace")).toHaveAttribute("data-workspace-mode","gis");
 await expect(page.locator(".maplibregl-canvas")).toBeVisible();
 expect(requests).toContain(project+"/layers");
 expect(requests).not.toContain(project+"/cad/documents");
 const cesium=buildManifest["../../packages/spatial-workspace/src/cesium3d.ts"]?.file;
 expect(cesium).toBeTruthy();
 expect(loaded.has("/"+cesium)).toBe(false);
});


test("session expiry clears the previous actor spatial cache before returning to landing",async({page})=>{
 const state={denyMe:false};
 await setup(page,false,false,state);
 await page.goto("/app");
 await expect(page.locator(".maplibregl-canvas")).toBeVisible();
 await page.evaluate(async()=>{
   await new Promise<void>((resolve,reject)=>{
     const r=indexedDB.open("ovg-spatial-cache-v1",1);
     r.onupgradeneeded=()=>{if(!r.result.objectStoreNames.contains("layers"))r.result.createObjectStore("layers",{keyPath:"key"})};
     r.onsuccess=()=>{const tx=r.result.transaction("layers","readwrite");tx.objectStore("layers").put({key:JSON.stringify(["actor1","org1","prj1","layer-x"]),scope:{account:"actor1",org:"org1",project:"prj1"},layer:{},rows:[],saved:Date.now(),bytes:0,permissionKey:"fixture"});tx.oncomplete=()=>{r.result.close();resolve()};tx.onerror=()=>reject(tx.error)};
     r.onerror=()=>reject(r.error);
   });
 });
 state.denyMe=true;
 await page.evaluate(()=>window.dispatchEvent(new Event("focus")));
 await expect(page.locator(".landing-page")).toBeVisible();
 await expect.poll(()=>page.evaluate(async()=>await new Promise<number>((resolve,reject)=>{
   const r=indexedDB.open("ovg-spatial-cache-v1");
   r.onsuccess=()=>{const tx=r.result.transaction("layers","readonly"),q=tx.objectStore("layers").count();q.onsuccess=()=>{r.result.close();resolve(q.result)};q.onerror=()=>reject(q.error)};
   r.onerror=()=>reject(r.error);
 }))).toBe(0);
});
