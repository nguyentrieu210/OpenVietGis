import {chromium} from "@playwright/test";import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1100,height:800}}),errors=[],external=[];
 page.on("pageerror",e=>errors.push(e.message));await page.route("**/*",r=>{if(new URL(r.request().url()).hostname!=="127.0.0.1"){external.push(r.request().url());return r.abort()}return r.continue()});
 await page.goto("http://127.0.0.1:5199/multi-scene.html");await page.getByText("scene ready",{exact:true}).waitFor({timeout:30000});
 const canvas=page.locator(".maplibregl-canvas");
 async function sample(){const png=await canvas.screenshot();return page.evaluate(async data=>{const image=await createImageBitmap(await(await fetch("data:image/png;base64,"+data)).blob());const c=document.createElement("canvas");c.width=image.width;c.height=image.height;const ctx=c.getContext("2d");ctx.drawImage(image,0,0);const a=ctx.getImageData(0,0,c.width,c.height).data;const groups={red:[],blue:[],yellow:[]};for(let i=0;i<a.length;i+=4){const r=a[i],g=a[i+1],b=a[i+2];const k=r>100&&g<50&&b<50?"red":b>120&&r<50&&g<60?"blue":r>170&&g>100&&g<210&&b<100?"yellow":null;if(k)groups[k].push([(i/4)%c.width,Math.floor(i/4/c.width)])}return Object.fromEntries(Object.entries(groups).map(([k,ps])=>{if(!ps.length)return[k,{count:0}];const x=ps.reduce((s,p)=>s+p[0],0)/ps.length,y=ps.reduce((s,p)=>s+p[1],0)/ps.length;let best=ps[0],d=Infinity;for(const p of ps){const e=(p[0]-x)**2+(p[1]-y)**2;if(e<d){d=e;best=p}}return[k,{count:ps.length,point:best}]}))},png.toString("base64"))}
 async function waitPixels(predicate){let p;for(let i=0;i<8;i++){p=await sample();if(predicate(p))return p;await page.waitForTimeout(160)}throw Error("Unexpected pixels: "+JSON.stringify(p))}
 async function click(p){const rect=await canvas.boundingBox();await page.mouse.click(rect.x+p[0],rect.y+p[1])}
 const first=await waitPixels(p=>p.red.count>100&&p.blue.count>100);console.log("Two models",first);
 await click(first.blue.point);await page.locator("#picked").filter({hasText:"blue"}).waitFor({timeout:10000});
 await waitPixels(p=>p.yellow.count>100&&p.red.count>100);
 await page.getByRole("button",{name:"Hide blue"}).click();await waitPixels(p=>p.blue.count===0&&p.red.count>100&&p.yellow.count===0);
 await click(first.blue.point);assert.equal(await page.locator("#picked").textContent(),"none");
 await page.getByRole("button",{name:"Show blue"}).click();const visible=await waitPixels(p=>p.red.count>100&&p.blue.count>100);
 await click(visible.red.point);await page.locator("#picked").filter({hasText:"red"}).waitFor({timeout:10000});
 await page.getByRole("button",{name:"Dispose"}).click();assert.equal(await canvas.count(),0);assert.deepEqual(errors,[]);assert.deepEqual(external,[]);
 console.log("PASS actual pixels for two GLB models, triangle picking, highlight, hide/show, no hidden pick, disposal");
}finally{await browser.close()}
