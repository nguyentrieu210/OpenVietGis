import {chromium} from "@playwright/test";import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1100,height:800}}),errors=[],external=[];
 page.on("pageerror",e=>errors.push(e.message));page.on("console",m=>{if(m.type()==="error")console.log("browser:",m.text().slice(0,300))});
 await page.route("**/*",r=>{if(new URL(r.request().url()).hostname!=="127.0.0.1"){external.push(r.request().url());return r.abort()}return r.continue()});
 await page.goto("http://127.0.0.1:5199/scene.html");
 await page.getByText("scene ready",{exact:true}).waitFor({timeout:30000});
 const surface=page.locator("#surface");let pixels={green:0,red:0};
 for(let i=0;i<4;i++){
  const png=await surface.screenshot();pixels=await page.evaluate(async data=>{const image=await createImageBitmap(await (await fetch("data:image/png;base64,"+data)).blob());const c=document.createElement("canvas");c.width=image.width;c.height=image.height;const x=c.getContext("2d");x.drawImage(image,0,0);const a=x.getImageData(0,0,c.width,c.height).data;let green=0,red=0;for(let i=0;i<a.length;i+=4){if(a[i+1]>a[i]+25&&a[i+1]>a[i+2]+20)green++;if(a[i]>100&&a[i+1]<50&&a[i+2]<50)red++}return {green,red}},png.toString("base64"));if(pixels.green>100&&pixels.red>100)break;
 }
 console.log(pixels);assert.ok(pixels.green>100&&pixels.red>100,"Terrain and GLB must both actually render");
 await page.getByRole("button",{name:"Dispose"}).click();assert.equal(await page.locator(".ovg-footprint-3d").count(),0);assert.equal(errors.length,0,errors.join("\n"));assert.equal(external.length,0);
 console.log("PASS terrain and GLB real WebGL pixel rendering, no external I/O, disposal");
}finally{await browser.close()}
