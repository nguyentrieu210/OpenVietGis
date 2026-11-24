import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {gltfFixture} from "../../packages/spatial-workspace/tests/terrain-model-fixture.mjs";
const {doc,bin}=gltfFixture();doc.accessors[0].min=[0,0,0];doc.accessors[0].max=[25,25,25];doc.bufferViews[0].target=34962;doc.bufferViews[1].target=34963;
const raw=Buffer.from(JSON.stringify(doc)),js=(raw.length+3)&~3,bs=(bin.length+3)&~3,glb=Buffer.alloc(28+js+bs);glb.writeUInt32LE(0x46546c67,0);glb.writeUInt32LE(2,4);glb.writeUInt32LE(glb.length,8);glb.writeUInt32LE(js,12);glb.writeUInt32LE(0x4e4f534a,16);glb.fill(32,20,20+js);raw.copy(glb,20);glb.writeUInt32LE(bs,20+js);glb.writeUInt32LE(0x004e4942,24+js);Buffer.from(bin).copy(glb,28+js);
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1380,height:900}}),errors=[],external=[],requests=[];
 let restricted=false;
 const org="fixture-org",pid="fixture-project",id="tileset-fixture",region=[-.00003,-.00003,.00003,.00003,-1,50],transform=[0,1,0,0,0,0,1,0,1,0,0,0,6378137,0,0,1];
 const record={id,name:"Native Cesium tiles",byte_size:1000,tile_count:2,created_at:"2026-01-01"};
 const manifest={asset:{version:"1.1"},geometricError:20,root:{boundingVolume:{region},geometricError:10,refine:"REPLACE",transform,content:{uri:"coarse.glb"},children:[{boundingVolume:{region},geometricError:0,content:{uri:"detail.glb"}}]}};
 page.on("pageerror",e=>errors.push(e.message));page.on("console",m=>{if(m.type()==="warning"&&m.text().includes("Cesium tile failed"))console.log(m.text().slice(0,400))});
 await page.route("**/*",route=>{const url=new URL(route.request().url());if(url.hostname==="tile.openstreetmap.org")return route.fulfill({status:404,body:""});if(url.hostname!=="127.0.0.1"){external.push(url.hostname);return route.abort()}return route.continue()});
 await page.route("**/api/v1/**",async route=>{
  const path=new URL(route.request().url()).pathname;let body;
  if(path.endsWith("/me"))body={id:"fixture",display_name:"Tiles Test",status:"active"};
  else if(path.endsWith("/auth/providers"))body=[];
  else if(path.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(path.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
  else if(path.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Tile project",status:"active"}];
  else if(path.endsWith("/access/mine"))body={restricted};
  else if(path.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(path.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(path.endsWith("/3d/tilesets"))body={items:[record]};
  else if(path.endsWith("/tileset.json"))body=manifest;
  else if(path.endsWith(".glb")){requests.push(path);return route.fulfill({body:glb,contentType:"model/gltf-binary"})}
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4218/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 const modes=page.getByRole("group",{name:"Chế độ bản đồ"});await modes.waitFor();
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=false);await modes.getByRole("button",{name:"Địa cầu",exact:true}).click();
 await page.locator('[data-ovg3d-engine="cesium"] canvas').waitFor({timeout:60000});
 await modes.getByRole("button",{name:"2D",exact:true}).click();await modes.getByRole("button",{name:"2D",exact:true}).waitFor();await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);
 await page.getByRole("button",{name:"3D Tiles",exact:true}).last().click();
 await page.getByLabel("Bộ dữ liệu").selectOption(id);
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=false);await modes.getByRole("button",{name:"Địa cầu",exact:true}).click();
 const canvas=page.locator('[data-ovg3d-engine="cesium-tiles"] canvas');await canvas.waitFor({timeout:60000});
 await page.getByText(/CesiumJS · (3D Tiles sẵn sàng|đã tải nội dung tile)/).waitFor({timeout:30000}).catch(async e=>{console.log("DEBUG",requests,errors,external,await page.locator(".ovg-3d-note").innerText());throw e});
 assert.ok(requests.length>0);
 let visible=0;
 for(let i=0;i<10;i++){
  const png=await canvas.screenshot();
  visible=await page.evaluate(async data=>{const image=await createImageBitmap(await(await fetch("data:image/png;base64,"+data)).blob());const c=document.createElement("canvas");c.width=image.width;c.height=image.height;const ctx=c.getContext("2d");ctx.drawImage(image,0,0);const p=ctx.getImageData(0,0,c.width,c.height).data;let n=0;for(let i=0;i<p.length;i+=4)if(p[i]>75&&p[i]>p[i+1]*1.8&&p[i]>p[i+2]*1.8)n++;return n},png.toString("base64"));
  if(visible>20)break;
 }
 assert.ok(visible>20,"native tile visible pixels="+visible);console.log("Native tile red pixels:",visible);
 await page.getByRole("button",{name:"Chế độ tương thích",exact:true}).click();
 await page.locator('[data-ovg-alternate-active="3d"] .maplibregl-canvas').waitFor({timeout:30000});
 await page.getByRole("button",{name:"Dùng CesiumJS",exact:true}).click();
 await canvas.waitFor({timeout:60000});
 assert.ok(requests.every(p=>p.startsWith("/api/v1/organizations/"+org+"/projects/"+pid+"/3d/tilesets/"+id+"/")));
 restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));
 await canvas.waitFor({state:"detached"});
 assert.deepEqual(errors,[]);assert.deepEqual(external,[]);
 console.log("PASS Cesium empty globe, native tile request/streaming, project-scoped paths and revocation");
}finally{await browser.close()}
