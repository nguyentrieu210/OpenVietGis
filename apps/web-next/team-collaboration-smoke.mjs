import {chromium} from "@playwright/test";
import assert from "node:assert/strict";

import {createServer} from "node:http";
import {createReadStream,existsSync,statSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {resolve,extname} from "node:path";
const dist=fileURLToPath(new URL("./dist/",import.meta.url)),streams=new Set();
const mime={".html":"text/html",".js":"text/javascript",".mjs":"text/javascript",".css":"text/css",".png":"image/png",".svg":"image/svg+xml",".woff2":"font/woff2",".json":"application/json"};
const server=createServer((req,res)=>{
 const path=new URL(req.url,"http://127.0.0.1").pathname;
 if(path.endsWith("/collaboration/events")){
  res.writeHead(200,{"Content-Type":"text/event-stream","Cache-Control":"no-cache","Connection":"keep-alive"});
  res.write("retry: 1000\nevent: ready\ndata: {}\n\n");streams.add(res);res.on("close",()=>streams.delete(res));return;
 }
 const candidate=resolve(dist,"."+path),file=candidate.startsWith(dist)&&existsSync(candidate)&&statSync(candidate).isFile()?candidate:resolve(dist,"index.html");
 res.writeHead(200,{"Content-Type":mime[extname(file)]||"application/octet-stream"});createReadStream(file).pipe(res);
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
const url="http://127.0.0.1:"+server.address().port+"/app";
const push=()=>{for(const res of streams)res.write("event: change\ndata: {}\n\n")};
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}}),errors=[];
 page.setDefaultNavigationTimeout(60000);
 page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project";let actor="owner",restricted=false,messages=[],tasks=[],reads=new Set();
 const members=[{id:"owner",display_name:"Quản lý"},{id:"member",display_name:"Khảo sát viên"}];
 await page.route("**/api/v1/**",route=>{
   const req=route.request(),u=new URL(req.url()),p=u.pathname,method=req.method();let body;
   if(p.endsWith("/collaboration/events"))return route.continue();
   if(p.endsWith("/me"))body={id:actor,display_name:members.find(m=>m.id===actor).display_name,status:"active"};
   else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
   else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",role:"owner",status:"active"}];
   else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Project fixture",status:"active"}];
   else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Project fixture",status:"active"};
   else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
   else if(p.endsWith("/access/mine"))body={restricted,rules:[]};
   else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
   else if(p.endsWith("/avatar"))body={data_url:null};
   else if(p.endsWith("/layers"))body={items:[],next_after_id:null};
   else if(p.endsWith("/extensions/activations")||p.endsWith("/extensions/manifests")||p.endsWith("/extensions/partner-contributions"))body={items:[]};
   else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
   else if(p.endsWith("/collaboration/members"))body={items:members};
   else if(p.endsWith("/collaboration/messages")&&method==="GET"){const peer=u.searchParams.get("peer");body={items:messages.filter(m=>peer?m.author_id===actor&&m.recipient_id===peer||m.author_id===peer&&m.recipient_id===actor:!m.recipient_id)}}
   else if(p.endsWith("/collaboration/notifications")&&method==="GET"){const items=tasks.filter(t=>t.assignee_id===actor&&t.creator_id!==actor).map(t=>({id:t.id,title:t.title,creator_name:t.creator_name,created_at:new Date().toISOString(),read_at:reads.has(t.id)?new Date().toISOString():null}));body={items,unread:items.filter(t=>!t.read_at).length}}
   else if(p.endsWith("/collaboration/notifications/t1/read")&&method==="POST"){reads.add("t1");body={read:true};push()}
   else if(p.endsWith("/collaboration/messages")&&method==="POST"){assert.equal(req.headers()["x-openvietgis-csrf"],"fixture-csrf");body={id:"m"+messages.length,author_id:actor,author_name:members.find(m=>m.id===actor).display_name,created_at:new Date().toISOString(),...req.postDataJSON()};messages.push(body);push()}
   else if(p.endsWith("/collaboration/tasks")&&method==="GET")body={items:u.searchParams.has("mine")?tasks.filter(t=>t.assignee_id===actor):tasks};
   else if(p.endsWith("/collaboration/tasks")&&method==="POST"){const payload=req.postDataJSON();assert.equal(payload.assignee_id,"member");assert.equal(payload.priority,"high");assert.equal(typeof payload.longitude,"number");assert.equal(typeof payload.latitude,"number");assert.equal(payload.source_message_id,"m0");body={...payload,id:"t1",creator_id:actor,creator_name:"Quản lý",assignee_name:"Khảo sát viên",status:"todo",revision:1};tasks=[body];push()}
   else if(p.endsWith("/collaboration/tasks/t1")&&method==="PATCH"){const payload=req.postDataJSON();assert.equal(payload.revision,1);assert.equal(payload.status,"done");body={...tasks[0],status:"done",revision:2};tasks=[body];push()}
   else return route.fulfill({status:404,json:{}});
   return route.fulfill({json:body});
 });
 await page.route("https://tile.openstreetmap.org/**",r=>r.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 const toggle=page.getByRole("button",{name:/Cộng tác/});
 let socket;
 await page.routeWebSocket("**/collaboration/presence",ws=>{socket=ws;ws.onMessage(()=>{});ws.send(JSON.stringify({type:"presence",self_id:"a",users:[{id:"a",actor:"owner",name:"Quản lý",color:"#2563eb",cursor:null,view:null},{id:"b",actor:"member",name:"Khảo sát viên",color:"#059669",cursor:[106,14],view:[106,14,8,0,0]}]}))});
 await page.goto(url,{waitUntil:"domcontentloaded"});await toggle.waitFor({timeout:25000});
 const live=page.getByRole("button",{name:/^● Live/});await live.click();
 await page.locator(".live-cursor").getByText("Khảo sát viên").waitFor();
 const follow=page.getByRole("button",{name:/Khảo sát viên · Theo dõi/});await follow.click();
 await page.getByRole("button",{name:/Khảo sát viên · Đang theo dõi/}).waitFor();
 socket.send(JSON.stringify({type:"presence",self_id:"a",users:[{id:"a",actor:"owner",name:"Quản lý",color:"#2563eb",cursor:null,view:null}]}));
 await page.locator(".live-cursor").waitFor({state:"detached"});
 for(const res of streams)res.write("event: spatial_change\ndata: {}\n\n");
 await page.getByText("Đã đồng bộ thay đổi bản đồ từ đồng nghiệp",{exact:true}).waitFor();
 assert.equal(await live.getAttribute("aria-pressed"),"true");await live.click();
 await toggle.click();
 const panel=page.getByRole("complementary",{name:"Cộng tác dự án"});
 await panel.getByRole("textbox",{name:"Tin nhắn"}).fill("Kiểm tra lô 4 <script>");
 await panel.getByRole("button",{name:"Gửi",exact:true}).click();
 await panel.getByText("Kiểm tra lô 4 <script>",{exact:true}).waitFor();
 await panel.getByText("Realtime · Đã kết nối",{exact:false}).waitFor();
 // Simulate a commit by another client; there is no manual refresh.
 messages.push({id:"remote",author_id:"member",author_name:"Khảo sát viên",body:"Tin mới qua SSE",created_at:new Date().toISOString(),recipient_id:null});push();
 await panel.getByText("Tin mới qua SSE",{exact:true}).waitFor({timeout:3000});
 await panel.getByLabel("Cuộc trò chuyện").selectOption("member");
 await panel.getByRole("textbox",{name:"Tin nhắn"}).fill("Riêng tư");
 await panel.getByRole("button",{name:"Gửi",exact:true}).click();
 await panel.getByText("Riêng tư",{exact:true}).waitFor();
 assert.equal(await panel.getByRole("button",{name:/Tạo việc từ tin nhắn/}).count(),0);
 await panel.getByLabel("Cuộc trò chuyện").selectOption("");
 await panel.getByText("Kiểm tra lô 4 <script>",{exact:true}).waitFor();
 await panel.locator("article").filter({hasText:"Kiểm tra lô 4 <script>"}).getByRole("button",{name:/Tạo việc từ tin nhắn/}).click();
 await panel.getByLabel("Tiêu đề").fill("Khảo sát lô 4");
 await panel.getByLabel("Giao cho").selectOption("member");
 await panel.getByLabel("Ưu tiên").selectOption("high");
 await panel.getByRole("button",{name:/Ghim tâm bản đồ/}).click();
 await panel.getByRole("button",{name:"Tạo công việc",exact:true}).click();
 await panel.getByText("Khảo sát lô 4",{exact:true}).waitFor();
 await panel.getByRole("button",{name:"Việc của tôi",exact:true}).click();
 await panel.getByText("Chưa có công việc.",{exact:true}).waitFor();
 actor="member";await page.reload({waitUntil:"domcontentloaded"});await toggle.waitFor({timeout:25000});await toggle.click();
 await panel.getByText("Kiểm tra lô 4 <script>",{exact:true}).waitFor();
 await panel.getByRole("button",{name:/Thông báo \(1\)/}).click();
 await panel.getByText("Khảo sát lô 4",{exact:true}).waitFor();
 await panel.getByRole("button",{name:"Đánh dấu đã đọc"}).click();
 await panel.getByText("0 chưa đọc",{exact:true}).waitFor();
 await panel.getByRole("button",{name:"Việc của tôi",exact:true}).click();
 await panel.getByText("Khảo sát lô 4",{exact:true}).waitFor();
 await panel.getByRole("button",{name:/Kanban/}).click();
 await panel.locator(".team-kanban-column").filter({hasText:"Cần làm"}).locator(".team-collab-task").waitFor();
 await panel.getByRole("button",{name:/Đến vị trí/}).click();
 await panel.locator(".team-collab-task").dragTo(panel.locator(".team-kanban-column").filter({has:page.getByRole("heading",{name:"Hoàn thành · 0",exact:true})}));
 await page.waitForFunction(()=>document.querySelector('select[aria-label="Trạng thái Khảo sát lô 4"]')?.value==="done");
 assert.equal(tasks[0].revision,2);
 restricted=true;await page.reload({waitUntil:"domcontentloaded"});await page.getByRole("button",{name:/^◈ Plugin/}).waitFor({timeout:25000});assert.equal(await toggle.count(),0);
 assert.deepEqual(errors,[]);console.log("PASS Studio collaboration real browser: SSE push, private chat, message-to-task, inbox, map pin, Kanban, scoped UI");
}finally{await browser.close();for(const res of streams)res.end();await new Promise(resolve=>server.close(resolve))}
