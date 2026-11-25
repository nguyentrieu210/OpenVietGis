import assert from "node:assert/strict";
import {createServer} from "vite";
import {chromium} from "@playwright/test";
const server=await createServer({configFile:false,root:new URL(".",import.meta.url).pathname,server:{host:"127.0.0.1",port:5199,strictPort:true}});
await server.listen();
let browser;
try{
 browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
 const page=await browser.newPage();
 const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const alice={id:"alice",display_name:"Alice",contact:false};
 const baseRoom={id:"r1",kind:"org",name:"Tổ chức thử",member_count:2,unread:0,mentioned:false,peer:null,last_message:null};
 let rooms=[baseRoom], messages={r1:[]}, sent=[];
 await page.route("**/org-chat-test.html",r=>r.fulfill({contentType:"text/html",body:'<div id="root"></div><script type="module" src="/org-chat-test-harness.tsx"></script>'}));
 await page.route("**/api/v1/**",async route=>{
  const req=route.request(),path=new URL(req.url()).pathname,method=req.method(),body=req.postDataJSON?.();
  const send=(data,status=200)=>route.fulfill({status,contentType:"application/json",body:JSON.stringify(data)});
  if(path.endsWith("/events"))return route.abort();
  if(path.includes("csrf"))return send({csrf_token:"test",token:"test"});
  if(path.endsWith("/people"))return send({items:[alice]});
  if(path.endsWith("/contacts"))return send({items:[],incoming:[],outgoing:[]});
  if(path.endsWith("/rooms/direct")){const room={...baseRoom,id:"dm",kind:"direct",name:"Alice",peer:alice};rooms.push(room);messages.dm=[];return send(room)}
  if(path.endsWith("/rooms")&&method==="GET")return send({items:rooms});
  if(path.endsWith("/rooms")&&method==="POST"){const room={...baseRoom,id:"group",kind:"group",name:body.name};rooms.push(room);messages.group=[];return send(room,201)}
  const match=path.match(/\/rooms\/([^/]+)\/(messages|read|pins)$/);
  if(match){const [,id,action]=match;if(action==="read")return send({read:true});if(action==="pins")return send({items:[]});if(method==="GET")return send({items:messages[id]??[]});sent.push({id,body});const msg={id:"m"+sent.length,author_id:"me",author_name:"Tôi",body:body.body,created_at:new Date().toISOString(),pinned:false,reply_to:null,mentions:[],attachments:[],reactions:[]};messages[id].push(msg);return send(msg,201)}
  return send({detail:"Unexpected "+method+" "+path},404);
 });
 await page.goto("http://127.0.0.1:5199/org-chat-test.html");
 await page.getByRole("button",{name:"# Tổ chức thử"}).waitFor();
 await page.getByRole("textbox",{name:"Tin nhắn"}).fill("Tin kiểm thử");
 await page.getByRole("button",{name:"Gửi",exact:true}).click();
 await page.getByText("Tin kiểm thử",{exact:true}).waitFor();
 await page.getByRole("button",{name:"Trả lời",exact:true}).click();
 await page.getByRole("textbox",{name:"Tin nhắn"}).fill("Trả lời thử");
 await page.getByRole("button",{name:"Gửi",exact:true}).click();
 await page.getByText("Trả lời thử",{exact:true}).waitFor();
 assert.equal(sent[1].body.reply_to_id,"m1");
 await page.getByRole("button",{name:"Tìm người",exact:true}).click();
 await page.getByRole("button",{name:"Nhắn",exact:true}).click();
 await page.getByRole("textbox",{name:"Tin nhắn"}).fill("Tin riêng");
 await page.getByRole("button",{name:"Gửi",exact:true}).click();
 await page.getByText("Tin riêng",{exact:true}).waitFor();
 assert.equal(sent[2].id,"dm");
 await page.getByRole("button",{name:"＋ Nhóm",exact:true}).click();
 await page.getByPlaceholder("Tên nhóm").fill("Nhóm thử");
 await page.getByRole("button",{name:"Chọn",exact:true}).click();
 await page.getByRole("button",{name:"Tạo nhóm",exact:true}).click();
 await page.getByRole("button",{name:"♟ Nhóm thử"}).waitFor();
 assert.deepEqual(errors,[]);
 console.log("PASS: organization send, reply payload, direct room isolation, group creation; no browser runtime errors");
}finally{await browser?.close();await server.close()}
