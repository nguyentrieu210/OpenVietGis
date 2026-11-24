import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:i===1?1500:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:i===2?"LineString":"Point",dimensions:"XY",fields:[{field_id:"fld_name",name:"Tên hiển thị",type:"text",nullable:false},{field_id:"fld_num",name:"Số",type:"integer",nullable:true}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{fld_name:"Điểm "+i,fld_num:i,name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:part===2?"LineString":"Point",coordinates:part===2?[[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],[106.60+(i%10)*.006+.002,10.58+part*.03+Math.floor(i/10)*.006+.002]]:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let modelStore=[],modelJobs=[];let topoCalls=0, geomCalls=0,tileCalls=0,schemaCalls=0,rasterTiles=0,stored=[]; let active=false,pluginCalls=0,chunkCalls=0,manual,commits=[];
 page.on("request",r=>{if(/\/assets\/studio-[^/]+\.js/.test(r.url()))chunkCalls++});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,method=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/map")&&method==="GET")body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/map")&&method==="PUT"){const payload=route.request().postDataJSON();body={operation_id:payload.operation_id,configuration_revision:2};}
  else if(p.endsWith("/processing/models")&&method==="GET")body={items:modelStore};
  else if(p.endsWith("/processing/models")&&method==="POST"){const m=route.request().postDataJSON();body={id:"model-test",revision:1,...m};modelStore=[body]}
  else if(p.endsWith("/processing/models/model-test")&&method==="PUT"){const m=route.request().postDataJSON();assert.equal(m.revision,1);body={...m,id:"model-test",revision:2};modelStore=[body]}
  else if(p.endsWith("/processing/jobs")&&method==="POST"){const m=route.request().postDataJSON();modelJobs.push(m);body={status:"succeeded",output_layer_id:"job-layer-"+modelJobs.length,feature_count:1}}
  else if(p.endsWith("/raster/layers")&&method==="GET")body={items:stored};
  else if(p.endsWith("/raster/layers")&&method==="POST"){
    assert.match(route.request().headers()["content-type"],/multipart\/form-data/);
    const form=route.request().postData();const pick=k=>form.match(new RegExp('name="'+k+'"\\r\\n\\r\\n([^\\r\\n]+)'))?.[1];
    const op=pick("method"),name=pick("name");
    if(op==="ndvi")assert.equal(name,"NDVI lưu kiểm thử");
    if(op==="calculator"){assert.equal(pick("source_id"),"raster-test");assert.equal(pick("formula"),"A + 1")}
    if(op==="rgb"){assert.equal(pick("band_c"),"3");assert.match(form,/filename="rgb.tif"/)}
    if(op==="clip"){assert.equal(pick("source_id"),"raster-calc");assert.match(pick("clip_geojson"),/"type":"Polygon"/)}
    if(op==="change"){assert.equal(pick("source_id"),"raster-test");assert.equal(pick("second_id"),"raster-calc")}
    const saved={id:op==="ndvi"?"raster-test":op==="calculator"?"raster-calc":"raster-"+op,name,method:op,width:4000,height:4000,bands:2,output_bands:op==="rgb"?3:1,epsg:32648,bounds:[106.60,10.60,106.66,10.66],maxzoom:14,tile_count:100,byte_size:5000,created_at:"2026-11-23T00:00:00Z",legend:[-1,1],stats:{valid:16000000,nodata:0,min:.5,max:.5,mean:.5,std:0},...(op==="calculator"?{formula:"A + 1"}:{})};
    stored=[saved,...stored];body=saved;
  }
  else if(p.includes("/raster/layers/")&&p.includes("/tiles/")){rasterTiles++;return route.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVQImWOU2xL1n4GBgYGJAQoAIKwCL9GYHLMAAAAASUVORK5CYII=","base64")});}
  else if(p.includes("/raster/layers/")&&method==="DELETE"){stored=stored.filter(r=>!p.endsWith("/"+r.id));return route.fulfill({status:204});}
  else if(p.endsWith("/raster/analyze")){assert.match(route.request().headers()["content-type"],/multipart\/form-data/);body={method:"ndvi",width:3,height:3,bands:2,epsg:4326,stats:{valid:8,nodata:1,min:.5,max:.5,mean:.5,std:0},legend:[-1,1],corners:[[106.60,10.70],[106.61,10.70],[106.61,10.69],[106.60,10.69]],png:"iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVQImWOU2xL1n4GBgYGJAQoAIKwCL9GYHLMAAAAASUVORK5CYII=",tiff:"SUkqAA==",persisted:false};}
  else if(p.endsWith("/pro/topology")){topoCalls++;body={source_id:"source1",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),items:[{feature_id:"f0",other:"f1",geometry:{type:"Point",coordinates:[106.60,10.61]}},{feature_id:"f2",other:"f1",geometry:{type:"Point",coordinates:[106.61,10.62]}}],complete:true};}
  else if(p.endsWith("/pro/geometry")){geomCalls++;const payload=route.request().postDataJSON();if(payload.operation==="move_batch"){assert.equal(payload.primary.feature_ids.length,2);body={source_id:"source2",content_revision:"a".repeat(64),items:payload.primary.feature_ids.map(feature_id=>({feature_id,geometry:{type:"LineString",coordinates:[[106.60,10.64],[106.603,10.643]]}}))}}else body={source_id:"source2",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),items:[{feature_id:"f0",geometry:{type:"LineString",coordinates:[[106.60,10.64],[106.603,10.643]]}}]};}
  else if(p.endsWith("/pro/schema")){schemaCalls++;body={content_revision:"c".repeat(64),schema_revision:"d".repeat(64)};}
  else if(p.includes("/pro/tiles/")){tileCalls++;return route.fulfill({status:200,contentType:"application/vnd.mapbox-vector-tile",body:Buffer.alloc(0)});}
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();commits.push(payload);body={operation_id:payload.operation_id,source_id:p.includes("source2")?"source2":"source1",base_content_revision:payload.base_content_revision};}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
  else if(p.endsWith("/layers/manual")){manual=route.request().postDataJSON();body={operation_id:manual.operation_id,layer_id:"new-layer",source_id:"new-source"}}
  else if(p.endsWith("/access/mine"))body={restricted:false,rules:[]};
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",version:"2.0.0",api_compatibility:"1.x",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);


 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};


 page.on("dialog",d=>d.accept());
 await command("MODELER");
 const model=page.getByRole("dialog",{name:"Processing Modeler",exact:true});
 await model.waitFor();
 await model.getByRole("button",{name:/Buffer · Vùng đệm/}).click();
 await model.getByRole("button",{name:/Dissolve · Gộp vùng/}).click();
 await model.getByRole("textbox",{name:"Tên mô hình"}).fill("Kiểm thử quy trình");
 assert.match(await model.getByLabel("Đầu vào A").inputValue(),/^step:/);
 await model.getByRole("button",{name:"Lưu",exact:true}).click();
 await model.getByText("Đã lưu mô hình vào dự án").waitFor();
 assert.equal(modelStore[0].graph.nodes.length,2);
 await model.getByRole("button",{name:"▶ Chạy mô hình"}).click();
 await model.getByText(/Đã hoàn thành 2\/2 bước/).waitFor({timeout:20000});
 assert.equal(modelJobs.length,2);assert.equal(modelJobs[0].algorithm_id,"buffer");assert.equal(modelJobs[1].algorithm_id,"dissolve");assert.equal(modelJobs[1].input_layer_id,"job-layer-1");
 await model.getByRole("button",{name:"Đóng Modeler"}).click();
 await command("MODELER");await model.waitFor();
 await model.getByLabel("Mô hình đã lưu").selectOption("model-test");
 assert.equal(await model.locator(".modeler-node").count(),2);
 await model.getByRole("button",{name:"Tự sắp xếp"}).click();
 await model.getByRole("button",{name:"Lưu",exact:true}).click();
 await model.getByText("Đã lưu mô hình vào dự án").waitFor();
 assert.equal(modelStore[0].revision,2);
 assert.deepEqual(errors,[]);console.log("Modeler UI: real flow mock, graph dependency, save/reopen/update passed");
}finally{await browser.close()}
