import { expect, test } from "@playwright/test";
import { isDeepStrictEqual } from "node:util";
import type { MapViewport, MapSaveReceipt, SaveMapInput, SpatialFeatureDetail, SpatialLayerList } from "../../packages/client-sdk/src";
import {
  createMapProject, currentMap, expectUsableMap, fixtureConfiguration, mapMutation, mapRead, mapSignIn,
  normalMapViewpoint, registerMapReferences, type MapFixture
} from "./map-fixtures";

test.use({viewport:{width:1280,height:900},trace:"off",screenshot:"off",video:"off"});

test("three real 10000-feature reference layers settle and pick a real rendered feature",async ({page},testInfo)=>{
  test.setTimeout(240_000);
  await mapSignIn(page);
  // Reuse only an explicitly identified synthetic project with the exact
  // reference corpus. This excludes repeated registration from renderer timing.
  const reuse=process.env.OPENVIETGIS_E2E_R004_SYNTHETIC_FIXTURE;
  let fixture:MapFixture;
  if(reuse){
    if(!/^[0-9a-f-]{36}\/[0-9a-f-]{36}$/.test(reuse))throw new Error("Invalid explicit synthetic fixture");
    const [organizationId,projectId]=reuse.split("/") as [string,string];
    const root="/organizations/"+organizationId+"/projects/"+projectId;
    fixture={organizationId,projectId,root,api:"/api/v1"+root,sources:[],layers:[]};
    const actual=await mapRead<SpatialLayerList>(page,fixture.api+"/layers?limit=100");
    expect(actual.items).toHaveLength(3);
    for(const name of ["Points","Lines","Polygons"]){
      const layer=actual.items.find(item=>item.name===name);
      expect(layer).toBeDefined();
      expect(layer!.source.kind).toBe("reference_fixture");
      expect(layer!.source.organization_id).toBe(organizationId);
      expect(layer!.source.project_id).toBe(projectId);
      fixture.layers.push(layer!);fixture.sources.push(layer!.source);
    }
  }else{
    fixture=await createMapProject(page,Date.now()+"-large-reference-renderer");
    await registerMapReferences(page,fixture);
  }
  expect(fixture.sources.map(s=>s.feature_count)).toEqual([4000,3000,3000]);
  expect(fixture.sources.map(s=>s.schema.geometry_family)).toEqual(["Point","LineString","Polygon"]);
  expect(fixture.sources.every(s=>s.validation_state==="ready"&&s.stored_crs?.code===4326)).toBe(true);

  const current=await currentMap(page,fixture), configuration=fixtureConfiguration(fixture);
  // The authoritative save contract canonicalizes colors to uppercase.
  configuration.layers=configuration.layers.map(layer=>({...layer,style:{...layer.style,color:layer.style.color.toUpperCase()}}));
  if(!isDeepStrictEqual(current.configuration,configuration)){
    const input:SaveMapInput={operation_id:crypto.randomUUID(),expected_revision:current.configuration_revision,
      layers:configuration.layers,basemap:"grid",default_viewpoint:{mode:"set",value:normalMapViewpoint}};
    const saved=await mapMutation<MapSaveReceipt>(page,fixture.api+"/map","PUT",input);
    expect(saved.status).toBe(200);
  }

  const external:string[]=[];
  await page.route(/^https?:\/\/(?!(?:127\.0\.0\.1|localhost)(?::|\/|$))/i,async route=>{
    external.push(new URL(route.request().url()).origin);await route.abort();
  });
  const opened=performance.now();
  await page.goto(fixture.root + "/spatial");
  await expectUsableMap(page);
  const initialOpenMs=Math.round(performance.now()-opened);
  const frames=new Map<string,number>();
  const snapshots:Promise<void>[]=[];
  const started=performance.now();
  let allAt:number|undefined;
  const listener=(response:import("@playwright/test").Response)=>{
    const url=new URL(response.url());
    if(!url.pathname.startsWith(fixture.api+"/layers/")||!url.pathname.endsWith("/viewport")||response.status()!==200)return;
    snapshots.push((async()=>{
      const result=await response.json() as MapViewport;
      if(result.complete)return;
      const layer=fixture.layers.find(l=>l.id===result.context.layer_id);
      expect(layer).toBeDefined();
      expect(result.context).toMatchObject({
        organization_id:fixture.organizationId,project_id:fixture.projectId,source_id:layer!.source_id,
        content_revision:layer!.source.content_revision,schema_revision:layer!.source.schema_revision,
        geometry_included:true,response_crs:{authority:"EPSG",code:4326,coordinate_order:"xy"}
      });
      expect(result.limitation).toBe("feature_limit");
      expect(result.items).toHaveLength(500);
      frames.set(layer!.id,result.items.length);
      if(frames.size===3&&allAt===undefined)allAt=performance.now();
    })());
  };
  page.on("response",listener);
  try{
    await page.getByRole("button",{name:"Thu phóng toàn bộ lớp hiển thị",exact:true}).click();
    await expect.poll(()=>frames.size,{timeout:30_000}).toBe(3);
    await Promise.all(snapshots);
  }finally{page.off("response",listener);}
  const readyStarted=performance.now();
  try{
    // The same 5-second renderer-ready checkpoint as the existing R004 E2E.
    await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready","true");
  }finally{
    const report={
      initialOpenMs,fitResponsesMs:Math.round((allAt??readyStarted)-started),
      rendererWaitMs:Math.round(performance.now()-readyStarted),
      allResponsesToReadyMs:Math.round(performance.now()-(allAt??readyStarted)),
      actualReady:await page.getByTestId("map-engine-ready").getAttribute("data-ready"),
      frameCounts:[...frames.values()],storedCounts:fixture.sources.map(s=>s.feature_count),
      pins:fixture.sources.map(s=>({content:s.content_revision,schema:s.schema_revision,manifest:s.provenance.manifest_digest})),
      reusedSyntheticFixture:Boolean(reuse)
    };
    console.log("[R004 renderer measured]",JSON.stringify(report));
    await testInfo.attach("R004 exact fitted frame measurements",{body:JSON.stringify(report),contentType:"application/json"});
  }
  await expect(page.getByTestId("map-status")).toHaveAttribute("data-state","partial");
  await expect(page.getByTestId("map-status")).toHaveText("Hiển thị một phần");

  const points=fixture.layers[0]!;
  await page.getByRole("button",{name:"Chọn Points làm lớp hiện hành",exact:true}).click();
  const table=page.getByRole("table",{name:"Bảng thuộc tính theo phiên bản"});
  const detailPromise=page.waitForResponse(r=>new URL(r.url()).pathname===fixture.api+"/layers/"+points.id+"/features/point-000001");
  await table.getByRole("button",{name:"point-000001",exact:true}).click();
  const detailResponse=await detailPromise;
  expect(detailResponse.status()).toBe(200);
  const record=await detailResponse.json() as SpatialFeatureDetail;
  expect(record.context).toMatchObject({layer_id:points.id,source_id:points.source_id,
    content_revision:points.source.content_revision,schema_revision:points.source.schema_revision});
  expect(record.feature.feature_id).toBe("point-000001");
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id","point-000001");
  const extentDetail=page.waitForResponse(r=>new URL(r.url()).pathname===fixture.api+"/layers/"+points.id+"/features/point-000001");
  const selectedViewport=page.waitForResponse(r=>new URL(r.url()).pathname===fixture.api+"/layers/"+points.id+"/viewport");
  const command = page.getByRole("textbox", { name: "Nhập lệnh bản đồ" });
  await command.fill("ZS");
  await command.press("Enter");
  await command.blur();
  expect((await extentDetail).status()).toBe(200);
  const selectedFrame=await selectedViewport;
  expect(selectedFrame.status()).toBe(200);
  const selectedGeometry=await selectedFrame.json() as MapViewport;
  expect(selectedGeometry.items.some(item=>item.feature_id==="point-000001")).toBe(true);
  await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready","true");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("map-inspector")).not.toHaveAttribute("data-feature-id","point-000001");
  await page.getByTestId("map-canvas").click();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id","point-000001");
  expect(external).toEqual([]);
});
