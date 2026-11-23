import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const base="https://vps1.gpcds.site:2087",studio=base+"/rewrite/?live=1";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
async function login(page,user="owner-a"){await page.getByRole("link",{name:/Đăng nhập với/}).click();await page.locator("#username").fill(user);await page.locator("#password").fill("openvietgis-test-"+user);await page.locator("#kc-login").click();await page.getByRole("button",{name:user==="owner-a"?"Đăng xuất":"Tham gia tổ chức",exact:true}).waitFor({timeout:30000})}
const mutate=(page,path,body)=>page.evaluate(async({path,body})=>{const csrf=await(await fetch("/api/v1/auth/csrf")).json();const r=await fetch(path,{method:"POST",headers:{"content-type":"application/json","x-openvietgis-csrf":csrf.token},body:JSON.stringify(body)});if(!r.ok)throw Error("API HTTP "+r.status);return r.json()},{path,body});
try{
 const owner=await browser.newPage({viewport:{width:1440,height:900},locale:"vi-VN"});await owner.goto(studio);await login(owner);
 await owner.getByTitle("Tạo tổ chức",{exact:true}).click();await owner.getByLabel("Tên",{exact:true}).fill("UX020 invitation test "+Date.now());await owner.getByRole("button",{name:"Tạo và mở"}).click();await owner.getByRole("heading",{name:"Chưa chọn dự án"}).waitFor();
 const org=await owner.getByLabel("Tổ chức",{exact:true}).inputValue();
 await owner.getByRole("button",{name:"Team",exact:true}).click();await owner.getByLabel("Email người nhận").fill("delivered@resend.dev");await owner.getByRole("button",{name:"Gửi lời mời qua email"}).click();await owner.getByText(/Resend đã tiếp nhận thư mời/).waitFor({timeout:40000});console.log("PASS: API created invitation and Resend accepted test email.");
 await owner.getByRole("button",{name:"Thu hồi lời mời"}).click();await owner.getByText("Đã thu hồi lời mời.",{exact:true}).waitFor();console.log("PASS: pending email invitation revoked.");
 const invitation=await mutate(owner,"/api/v1/organizations/"+org+"/invitations",{target:{kind:"email",email:"member-a@example.test"},organization_role:"member",expires_at:new Date(Date.now()+3600000).toISOString()});
 const fragment=new URLSearchParams({organization_id:org,invitation_id:invitation.id,secret:invitation.secret});
 const context=await browser.newContext({viewport:{width:1440,height:900},locale:"vi-VN"});const member=await context.newPage();
 await member.goto(studio+"#"+fragment.toString());await member.getByText("Bạn được mời vào team",{exact:true}).waitFor();assert.equal(new URL(member.url()).hash,"");await login(member,"member-a");
 await member.getByRole("heading",{name:"Chấp nhận lời mời"}).waitFor();await member.getByRole("button",{name:"Tham gia tổ chức",exact:true}).click();await member.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor();await member.waitForFunction(id=>document.querySelector('select[aria-label="Tổ chức"]')?.value===id,org,{timeout:15000});assert.equal(await member.getByLabel("Tổ chức",{exact:true}).inputValue(),org);console.log("PASS: invite link survives OIDC, verified recipient accepted and organization selected.");
 const anon=await browser.newPage({locale:"vi-VN"});await anon.goto(studio);await anon.getByRole("link",{name:/Đăng nhập với/}).click();await anon.getByRole("link",{name:"Đăng ký",exact:true}).waitFor();await anon.getByRole("link",{name:"Quên mật khẩu?",exact:true}).waitFor();await anon.getByRole("link",{name:"Đăng ký",exact:true}).click();await anon.locator('input[name="email"]').waitFor();console.log("PASS: native registration and recovery links enabled; registration form loaded.");
} finally{await browser.close()}
