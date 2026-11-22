import {chromium} from "@playwright/test";
import assert from "node:assert/strict";

const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4194/rewrite/?live=1";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}}),errors=[];
 page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project";let actor="owner",restricted=false,messages=[],tasks=[];
 const members=[{id:"owner",display_name:"Quản lý"},{id:"member",display_name:"Khảo sát viên"}];
 await page.route("**/api/v1/**",route=>{
   const req=route.request(),u=new URL(req.url()),p=u.pathname,method=req.method();let body;
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
   else if(p.endsWith("/collaboration/messages")&&method==="GET")body={items:messages};
   else if(p.endsWith("/collaboration/messages")&&method==="POST"){assert.equal(req.headers()["x-openvietgis-csrf"],"fixture-csrf");body={id:"m"+messages.length,author_id:actor,author_name:members.find(m=>m.id===actor).display_name,created_at:new Date().toISOString(),...req.postDataJSON()};messages.push(body)}
   else if(p.endsWith("/collaboration/tasks")&&method==="GET")body={items:u.searchParams.has("mine")?tasks.filter(t=>t.assignee_id===actor):tasks};
   else if(p.endsWith("/collaboration/tasks")&&method==="POST"){const payload=req.postDataJSON();assert.equal(payload.assignee_id,"member");assert.equal(payload.priority,"high");assert.equal(typeof payload.longitude,"number");assert.equal(typeof payload.latitude,"number");body={...payload,id:"t1",creator_id:actor,creator_name:"Quản lý",assignee_name:"Khảo sát viên",status:"todo",revision:1};tasks=[body]}
   else if(p.endsWith("/collaboration/tasks/t1")&&method==="PATCH"){const payload=req.postDataJSON();assert.equal(payload.revision,1);assert.equal(payload.status,"done");body={...tasks[0],status:"done",revision:2};tasks=[body]}
   else return route.fulfill({status:404,json:{}});
   return route.fulfill({json:body});
 });
 await page.route("https://tile.openstreetmap.org/**",r=>r.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 const toggle=page.getByRole("button",{name:/Cộng tác/});
 await page.goto(url,{waitUntil:"domcontentloaded"});await toggle.waitFor({timeout:25000});await toggle.click();
 const panel=page.getByRole("complementary",{name:"Cộng tác dự án"});
 await panel.getByRole("textbox",{name:"Tin nhắn"}).fill("Kiểm tra lô 4 <script>");
 await panel.getByRole("button",{name:"Gửi",exact:true}).click();
 await panel.getByText("Kiểm tra lô 4 <script>",{exact:true}).waitFor();
 await panel.getByRole("button",{name:"Công việc",exact:true}).click();
 await panel.getByRole("button",{name:"+ Giao việc",exact:true}).click();
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
 await panel.getByRole("button",{name:"Việc của tôi",exact:true}).click();
 await panel.getByText("Khảo sát lô 4",{exact:true}).waitFor();
 await panel.getByRole("button",{name:/Đến vị trí/}).click();
 await panel.getByLabel("Trạng thái Khảo sát lô 4").selectOption("done");
 await page.waitForFunction(()=>document.querySelector('select[aria-label="Trạng thái Khảo sát lô 4"]')?.value==="done");
 assert.equal(tasks[0].revision,2);
 restricted=true;await page.reload({waitUntil:"domcontentloaded"});await page.getByRole("button",{name:/^◈ Plugin/}).waitFor({timeout:25000});assert.equal(await toggle.count(),0);
 assert.deepEqual(errors,[]);console.log("PASS Studio collaboration browser: chat, task creation, map pin, account switch, to-do, scoped UI");
}finally{await browser.close()}
