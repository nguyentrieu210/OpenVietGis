import {chromium} from "@playwright/test";import assert from "node:assert/strict";import {readFile} from "node:fs/promises";import {join} from "node:path";
const dir=process.env.OVG_GENERATED_TILE_FIXTURE;if(!dir)throw Error("Set OVG_GENERATED_TILE_FIXTURE to the real API test's tmp_path");
const read=async name=>JSON.parse(await readFile(join(dir,name),"utf8"));
const layer=await read("layer.json"),record=await read("record.json"),manifest=await read("tileset.json"),picks=await read("picks.json");
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1250,height:1000}}),errors=[],external=[],posts=[];
 let delayed=false;page.on("pageerror",e=>errors.push(e.message));
 await page.route("**/*",async route=>{
  const request=route.request(),url=new URL(request.url());
  if(url.hostname!=="127.0.0.1"){external.push(url.href);return route.abort()}
  if(!url.pathname.startsWith("/fixture/"))return route.continue();
  const p=url.pathname.slice("/fixture/".length);
  if(p==="layer.json")return route.fulfill({json:layer});
  if(p==="auth/csrf")return route.fulfill({json:{token:"fixture"}});
  if(p==="tilesets"&&request.method()==="GET")return route.fulfill({json:{items:[]}});
  if(p==="tilesets/generate"){posts.push(request.postDataJSON());assert.equal(request.headers()["x-openvietgis-csrf"],"fixture");return route.fulfill({status:201,json:record})}
  if(p==="tilesets/"+record.id+"/tileset.json")return route.fulfill({json:manifest});
  if(p==="tilesets/"+record.id+"/feature"){const tri=Number(url.searchParams.get("triangle")),v=picks.find(v=>v.uri===url.searchParams.get("uri")&&tri>=v.first_triangle&&tri<v.first_triangle+v.triangle_count);if(delayed){await new Promise(r=>setTimeout(r,500));return (v?route.fulfill({json:v}):route.fulfill({status:404})).catch(()=>{})}return v?route.fulfill({json:v}):route.fulfill({status:404})}
  const uri=p.slice(("tilesets/"+record.id+"/").length);
  if(manifest.root.children.some(c=>c.content.uri===uri))return route.fulfill({body:await readFile(join(dir,uri)),contentType:"model/gltf-binary"});
  return route.fulfill({status:404});
 });
 await page.goto("http://127.0.0.1:5199/generator.html");
 await page.getByLabel("Lớp polygon").selectOption(layer.id);
 await page.getByLabel("Chiều cao (m)").selectOption("fld_height");
 await page.getByLabel("Tên bộ tile").fill("Buildings");
 await page.getByLabel("Cao độ nền ellipsoid (m)").fill("20");
 const generate=page.getByRole("button",{name:"Tạo 3D Tiles",exact:true});
 assert.equal(await generate.isEnabled(),false);
 await page.getByRole("checkbox",{name:"Xác nhận cao độ nền ellipsoid WGS84 và chiều cao theo mét"}).check();await generate.click();
 await page.getByText("Đã tạo "+record.tile_count+" tile.",{exact:false}).waitFor();
 assert.equal(posts.length,1);assert.deepEqual(posts[0],{name:"Buildings",layer_id:layer.id,expected_layer_revision:layer.layer_revision,expected_content_revision:layer.source.content_revision,expected_schema_revision:layer.source.schema_revision,height_field:"fld_height",base_ellipsoid_m:20,acknowledge_ellipsoid:true});
 assert.equal(await page.getByLabel("Bộ dữ liệu").inputValue(),record.id);
 await page.getByRole("button",{name:"Xem mô hình đã tạo"}).click();
 await page.locator("#status").filter({hasText:"44 tam giác"}).waitFor({timeout:30000});
 const canvas=page.locator(".maplibregl-canvas");
 const sample=async()=>page.evaluate(async data=>{const image=await createImageBitmap(await(await fetch("data:image/png;base64,"+data)).blob());const c=document.createElement("canvas");c.width=image.width;c.height=image.height;const ctx=c.getContext("2d");ctx.drawImage(image,0,0);const a=ctx.getImageData(0,0,c.width,c.height).data;let blue=[],yellow=0;for(let i=0;i<a.length;i+=4){if(a[i]<100&&a[i+1]>90&&a[i+2]>150&&a[i+2]>a[i+1]+30)blue.push([i/4%c.width,Math.floor(i/4/c.width)]);if(a[i]>160&&a[i+1]>90&&a[i+2]<130)yellow++}return {blue,yellow}},(await canvas.screenshot()).toString("base64"));
 const initial=await sample(),pixels=initial.blue.length;assert.ok(pixels>100,"actual generated mesh pixels "+pixels);
 const points=initial.blue.sort((a,b)=>a[0]-b[0]);const left=points[Math.floor(points.length*.12)],right=points[Math.floor(points.length*.91)];
 const panel=page.getByRole("region",{name:"Công trình đang chọn"});
 for(const [xy,id] of [[left,"court"],[right,"other"]]){await canvas.click({position:{x:xy[0],y:xy[1]}});await panel.getByText(id,{exact:true}).waitFor();await panel.getByText("Height",{exact:true}).waitFor();await page.waitForTimeout(150);const current=await sample();assert.ok(current.yellow>100&&current.blue.length>100,"only chosen building highlighted");await panel.getByRole("button",{name:"Xem đối tượng GIS"}).click();await panel.getByText("GIS navigation "+id,{exact:true}).waitFor()}
 await canvas.click({position:{x:5,y:5}});await panel.getByText("Bấm một công trình để xem thuộc tính.").waitFor();await page.waitForTimeout(150);assert.equal((await sample()).yellow,0);
 await canvas.click({position:{x:left[0],y:left[1]}});await panel.getByText("court",{exact:true}).waitFor();await page.getByRole("button",{name:"Hide",exact:true}).click();await panel.getByText("Bấm một công trình để xem thuộc tính.").waitFor();await canvas.click({position:{x:left[0],y:left[1]}});assert.equal(await panel.getByText("court",{exact:true}).count(),0);await page.getByRole("button",{name:"Show",exact:true}).click();await page.waitForTimeout(150);
 delayed=true;await canvas.click({position:{x:left[0],y:left[1]}});await canvas.click({position:{x:5,y:5}});await page.waitForTimeout(800);assert.equal(await panel.getByText("court",{exact:true}).count(),0);await panel.getByText("Bấm một công trình để xem thuộc tính.").waitFor();
 await page.getByRole("button",{name:"Dispose"}).click();assert.equal(await canvas.count(),0);assert.deepEqual(errors,[]);assert.deepEqual(external,[]);
 console.log("PASS panel required acknowledgement, exact source pins, generated tile selected and actual API GLB rendered, two exact feature picks, individual highlight, empty/hide/late-result clearing, pixels="+pixels);
}finally{await browser.close()}
