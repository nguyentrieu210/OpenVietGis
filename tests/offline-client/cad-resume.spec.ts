import {test,expect} from "@playwright/test";
const prefix="/api/v1", project=prefix+"/organizations/org1/projects/prj1";
async function setup(page:import("@playwright/test").Page,restricted=false){
 const requests:string[]=[];
 await page.addInitScript(()=>localStorage.setItem("ovg:resume-mode:v1:actor1:org1:prj1","cad"));
 await page.route("**/api/v1/**",async route=>{
  const path=new URL(route.request().url()).pathname;requests.push(path);
  let data:unknown={items:[]};
  if(path===prefix+"/me")data={id:"actor1",status:"active",display_name:"Fixture",email_snapshot:null};
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
