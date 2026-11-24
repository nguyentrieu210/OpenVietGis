/** Cesium geographic viewer, using validated local scene inputs and no ion/CDN. */
import type * as Cesium from "cesium";
import type {FootprintView} from "./footprint3d";
import type {GeographicCamera,NamedModel,SelectableFootprintEngine,SharedVector} from "./maplibre3d";
import type {Tile} from "./tileset";
import type {SelectableTileEngine} from "./tiles3d";
import type {PlacedModel} from "./mesh-layer";
import {checkSceneBudget} from "./mesh-picking";
import {geographic3857,terrainTriangles,type TerrainGrid} from "./terrain";

type C=typeof Cesium;
type Pick={kind:"footprint"|"model"|"terrain";id:string};
type MeshGroup={id:Pick;positions:number[];color:Cesium.Color};
export type ProjectImagery={name:string;url:string;bounds:[number,number,number,number];maxzoom:number};
export type LegacyImagery={id:"google-satellite"|"google-hybrid";name:string;url:string;maxzoom:number;credit:string};
type SceneOptions={vectors?:readonly SharedVector[];initialBasemap?:string;onBasemapChange?:(id:string)=>void;legacyImagery?:readonly LegacyImagery[];imagery?:readonly ProjectImagery[];terrain?:TerrainGrid;model?:PlacedModel;models?:readonly NamedModel[];onModelPick?:(id:string)=>void};
const abortError=()=>new DOMException("Aborted","AbortError");
let runtime:Promise<C>|undefined;
export function loadCesium():Promise<C>{
 const base=new URL(import.meta.env.BASE_URL+"cesium/1.147.0/",location.origin).href;
 (globalThis as typeof globalThis&{CESIUM_BASE_URL:string}).CESIUM_BASE_URL=base;
 if(!document.querySelector("link[data-ovg-cesium-css]")){
  const link=document.createElement("link");link.rel="stylesheet";link.href=base+"Widgets/widgets.css";link.dataset.ovgCesiumCss="1";document.head.append(link);
 }
 const moduleUrl=base+"index.js";
 return runtime??=(import(/* @vite-ignore */ moduleUrl) as Promise<C>).catch(e=>{runtime=undefined;throw e});
}
const clamp=(n:number,l:number,h:number)=>Math.max(l,Math.min(h,n));
const rangeFor=(zoom:number,latitude:number)=>Math.max(5,40075016.6856*Math.max(.08,Math.cos(latitude*Math.PI/180))/2**zoom);
function validCamera(camera:GeographicCamera){
 if(![camera.longitude,camera.latitude,camera.zoom,camera.bearing,camera.pitch??58].every(Number.isFinite)||Math.abs(camera.longitude)>180||Math.abs(camera.latitude)>85||camera.zoom<0||camera.zoom>22||(camera.pitch??58)<0||(camera.pitch??58)>75)throw Error("Invalid 3D camera");
}
const cameraPins=new WeakMap<Cesium.CesiumWidget,{view:GeographicCamera;position:Cesium.Cartesian3;direction:Cesium.Cartesian3;up:Cesium.Cartesian3}>();
function setCamera(c:C,widget:Cesium.CesiumWidget,camera:GeographicCamera){
 validCamera(camera);
 const center=c.Cartesian3.fromDegrees(camera.longitude,camera.latitude);
 widget.camera.lookAt(center,new c.HeadingPitchRange(c.Math.toRadians(camera.bearing),c.Math.toRadians((camera.pitch??58)-90),rangeFor(camera.zoom,camera.latitude)));
 widget.camera.lookAtTransform(c.Matrix4.IDENTITY);
 cameraPins.set(widget,{view:{...camera},position:c.Cartesian3.clone(widget.camera.positionWC),direction:c.Cartesian3.clone(widget.camera.directionWC),up:c.Cartesian3.clone(widget.camera.upWC)});
}
function getCamera(c:C,widget:Cesium.CesiumWidget,maxPitch=75):GeographicCamera{
 const camera=widget.camera,canvas=widget.canvas,center=new c.Cartesian2(canvas.clientWidth/2,canvas.clientHeight/2),pin=cameraPins.get(widget);
 if(pin&&c.Cartesian3.distance(pin.position,camera.positionWC)<.001&&c.Cartesian3.distance(pin.direction,camera.directionWC)<1e-8&&c.Cartesian3.distance(pin.up,camera.upWC)<1e-8)return {...pin.view};
 const target=camera.pickEllipsoid(center)??c.Cartesian3.fromRadians(camera.positionCartographic.longitude,camera.positionCartographic.latitude);
 const ll=c.Cartographic.fromCartesian(target),latitude=clamp(c.Math.toDegrees(ll.latitude),-85,85);
 const offset=c.Matrix4.multiplyByPoint(c.Matrix4.inverseTransformation(c.Transforms.eastNorthUpToFixedFrame(target),new c.Matrix4()),camera.positionWC,new c.Cartesian3()),range=c.Cartesian3.magnitude(offset);
 const heading=Math.hypot(offset.x,offset.y)>.00001?Math.atan2(-offset.x,-offset.y):camera.heading;
 const pitch=range>0?Math.asin(clamp(-offset.z/range,-1,1)):camera.pitch;
 return {longitude:c.Math.toDegrees(ll.longitude),latitude,zoom:clamp(Math.log2(40075016.6856*Math.max(.08,Math.cos(ll.latitude))/Math.max(5,range)),0,22),bearing:c.Math.toDegrees(c.Math.negativePiToPi(heading)),pitch:clamp(90+c.Math.toDegrees(pitch),0,maxPitch)};
}
function addNavigation(c:C,widget:Cesium.CesiumWidget,surface:HTMLElement,initial:GeographicCamera,imagery:readonly ProjectImagery[]=[],legacyImagery:readonly LegacyImagery[]=[],initialBasemap?:string,onBasemapChange?:(id:string)=>void){
 const scene=widget.scene,controller=scene.screenSpaceCameraController,E=c.CameraEventType,M=c.KeyboardEventModifier;
 // Globe-ray camera navigation avoids synchronous GPU depth-buffer readback on drag.
 scene.useDepthPicking=false;
 controller.rotateEventTypes=E.LEFT_DRAG;
 controller.zoomEventTypes=[E.PINCH];
 controller.tiltEventTypes=[E.MIDDLE_DRAG,E.PINCH,{eventType:E.LEFT_DRAG,modifier:M.CTRL},{eventType:E.LEFT_DRAG,modifier:M.SHIFT}];
 controller.lookEventTypes=undefined;controller.enableLook=false;
 controller.inertiaSpin=0;controller.inertiaTranslate=0;controller.inertiaZoom=.5;controller.zoomFactor=4;controller.minimumZoomDistance=3;controller.maximumTiltAngle=c.Math.toRadians(75);
 scene.globe.maximumScreenSpaceError=2;scene.globe.tileCacheSize=160;scene.globe.preloadAncestors=true;scene.globe.preloadSiblings=true;
 const tools=document.createElement("div");tools.className="ovg-cesium-navigation";
 const buttons=document.createElement("div");buttons.className="ovg-earth-buttons";tools.append(buttons);
 const button=(label:string,content:string,action:()=>void)=>{const b=document.createElement("button");b.type="button";b.title=label;b.setAttribute("aria-label",label);b.textContent=content;b.onclick=()=>{action();scene.requestRender()};buttons.append(b);return b};
 const zoom=(factor:number)=>{const v=getCamera(c,widget);setCamera(c,widget,{...v,zoom:clamp(v.zoom+factor,0,22)})};
 const north=()=>setCamera(c,widget,{...getCamera(c,widget),bearing:0}),top=()=>setCamera(c,widget,{...getCamera(c,widget),bearing:0,pitch:0});
 button("Phóng to 3D","+",()=>zoom(1));button("Thu nhỏ 3D","−",()=>zoom(-1));button("Hướng Bắc","N",north);button("Nhìn thẳng từ trên","2D",top);button("Nghiêng 3D","3D",()=>setCamera(c,widget,{...getCamera(c,widget),pitch:60}));button("Về góc nhìn ban đầu","⌂",()=>setCamera(c,widget,initial));
 const settings=document.createElement("div");settings.className="ovg-earth-settings";
 const label=(name:string,el:HTMLElement)=>{const l=document.createElement("label");l.textContent=name;l.append(el);settings.append(l)};
 const choice=(name:string,options:readonly (readonly [string,string])[])=>{const el=document.createElement("select");el.setAttribute("aria-label",name);for(const [value,text] of options){const o=document.createElement("option");o.value=value;o.textContent=text;el.append(o)}label(name,el);return el};
 const quality=choice("Chất lượng 3D",[["smooth","Mượt"],["balanced","Cân bằng"],["sharp","Sắc nét"]]);quality.value="balanced";
 const resolution=()=>{
  if(widget.isDestroyed())return;
  const dpr=Math.max(1,window.devicePixelRatio||1),pixels=Math.max(1,surface.clientWidth*surface.clientHeight);
  const mode=quality.value,desired=mode==="smooth"?.8:mode==="sharp"?Math.min(2,dpr*1.5):Math.min(1.35,dpr),budget=mode==="smooth"?1200000:mode==="sharp"?4500000:2200000;
  widget.resolutionScale=Math.max(.25,Math.min(desired,Math.sqrt(budget/pixels)))/dpr;
  scene.globe.maximumScreenSpaceError=mode==="smooth"?3:mode==="sharp"?1.25:2;
  surface.dataset.ovgQuality=mode;widget.resize();scene.requestRender();
 };
 quality.onchange=resolution;
 const resize=new ResizeObserver(resolution);resize.observe(surface);
 const cleanImagery=imagery.filter(i=>i.name&&i.bounds.length===4&&i.bounds.every(Number.isFinite)&&i.bounds[0]>=-180&&i.bounds[2]<=180&&i.bounds[1]>=-85.0512&&i.bounds[3]<=85.0512&&i.bounds[0]<i.bounds[2]&&i.bounds[1]<i.bounds[3]&&Number.isInteger(i.maxzoom)&&i.maxzoom>=0&&i.maxzoom<=24&&/^\/api\/v1\/organizations\/[A-Za-z0-9_-]+\/projects\/[A-Za-z0-9_-]+\/raster\/layers\/[A-Za-z0-9_-]+\/tiles\/\{z\}\/\{x\}\/\{y\}\.png$/.test(i.url));
 const legacy=legacyImagery.filter(i=>i.id==="google-satellite"&&i.url==="https://mt0.google.com/vt/lyrs=s&x={x}&y={y}&z={z}"||i.id==="google-hybrid"&&i.url==="https://mt0.google.com/vt/lyrs=y&x={x}&y={y}&z={z}");
 const imagerySelect=choice("Ảnh nền 3D",[...legacy.map(i=>[i.id,i.name+" (legacy)"] as const),["natural","Natural Earth"],["sentinel","Vệ tinh Sentinel-2 (10 m)"],...cleanImagery.map((i,n)=>["project-"+n,i.name] as const)]);
 let overlay:Cesium.ImageryLayer|undefined;imagerySelect.value="natural";
 const info=document.createElement("div");info.className="ovg-earth-imagery-status";info.setAttribute("role","status");info.textContent="Natural Earth · ảnh tổng quan";
 imagerySelect.onchange=()=>{
  if(["google-satellite","google-hybrid","sentinel"].includes(imagerySelect.value))onBasemapChange?.(imagerySelect.value);
  if(overlay){widget.imageryLayers.remove(overlay,true);overlay=undefined}
  const selected=legacy.find(i=>i.id===imagerySelect.value);
  if(selected){
   const provider=new c.UrlTemplateImageryProvider({url:selected.url,tilingScheme:new c.WebMercatorTilingScheme(),maximumLevel:Math.min(20,selected.maxzoom),credit:new c.Credit(selected.credit,true)});
   overlay=widget.imageryLayers.addImageryProvider(provider);
   info.textContent=selected.name+" · nguồn legacy không chính thức";
   provider.errorEvent.addEventListener(()=>{if(imagerySelect.value===selected.id)info.textContent="Nguồn Google legacy không phản hồi; ảnh tổng quan vẫn hiển thị"});
  }else if(imagerySelect.value==="sentinel"){
   const provider=new c.UrlTemplateImageryProvider({url:"https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless_3857/default/g/{z}/{y}/{x}.jpg",tilingScheme:new c.WebMercatorTilingScheme(),maximumLevel:14,credit:new c.Credit("EOX · Sentinel-2 cloudless (Copernicus 2016/2017)",true)});
   overlay=widget.imageryLayers.addImageryProvider(provider);
   info.textContent="Sentinel-2 · 10 m, không phải ảnh độ phân giải thửa đất";
   provider.errorEvent.addEventListener(()=>{if(imagerySelect.value==="sentinel")info.textContent="Ảnh Sentinel chưa tải được; ảnh tổng quan vẫn hiển thị"});
  }else if(imagerySelect.value.startsWith("project-")){
   const item=cleanImagery[Number(imagerySelect.value.slice(8))];if(!item)return;
   const provider=new c.UrlTemplateImageryProvider({url:new URL(item.url,location.origin).href.replace(/%7B/gi,"{").replace(/%7D/gi,"}"),tilingScheme:new c.WebMercatorTilingScheme(),rectangle:c.Rectangle.fromDegrees(...item.bounds),maximumLevel:item.maxzoom,credit:new c.Credit("Raster dự án",true)});
   overlay=widget.imageryLayers.addImageryProvider(provider);info.textContent=item.name;
   provider.errorEvent.addEventListener(()=>{if(imagerySelect.value.startsWith("project-"))info.textContent="Một số tile raster chưa tải được"});
  }else info.textContent="Natural Earth · ảnh tổng quan";
  scene.requestRender();
 };
 if(legacy.some(i=>i.id===initialBasemap)||initialBasemap==="sentinel"){imagerySelect.value=initialBasemap!;imagerySelect.dispatchEvent(new Event("change"))}
 settings.append(info);
 const help=document.createElement("small");help.textContent="Trái: nắm và kéo địa cầu · Phải xuống/lên: zoom · Cuộn: zoom tại con trỏ · Giữa/Ctrl: xoay 360° và nghiêng · N/U/R: đặt lại";settings.append(help);
 const details=document.createElement("details");details.className="ovg-earth-settings-menu";const summary=document.createElement("summary");summary.textContent="Hiển thị 3D";details.append(summary,settings);tools.append(details);
 const credits=document.createElement("details");credits.className="ovg-earth-credit";const creditButton=document.createElement("summary");creditButton.textContent="ⓘ";creditButton.setAttribute("aria-label","Thông tin nguồn bản đồ");const creditText=document.createElement("p");creditText.textContent="CesiumJS self-hosted · Apache-2.0. Nguồn ảnh và ghi công nhà cung cấp hiển thị trên bản đồ. Google legacy là nguồn không chính thức.";credits.append(creditButton,creditText);tools.append(credits);
 surface.append(tools);
 const hud=document.createElement("div");hud.className="ovg-earth-camera-hud";hud.setAttribute("aria-label","Thông số camera 3D");surface.append(hud);
 const updateHud=()=>{if(widget.isDestroyed())return;const v=getCamera(c,widget,85),alt=widget.camera.positionCartographic.height;hud.dataset.altitude=String(alt);hud.dataset.longitude=String(v.longitude);hud.dataset.latitude=String(v.latitude);hud.dataset.bearing=String(v.bearing);hud.dataset.pitch=String(v.pitch??0);const next=v.latitude.toFixed(5)+"°, "+v.longitude.toFixed(5)+"° · "+(alt>=1000?(alt/1000).toFixed(2)+" km":Math.round(alt)+" m")+" · N "+Math.round(v.bearing)+"° · Nghiêng "+Math.round(v.pitch??0)+"°";if(hud.textContent!==next)hud.textContent=next};
 const removeHud=scene.postRender.addEventListener(updateHud);
 widget.canvas.tabIndex=0;widget.canvas.setAttribute("aria-label","Bản đồ 3D · điều hướng kiểu Google Earth");
 const canvas=widget.canvas;
 const preventMenu=(e:Event)=>e.preventDefault();canvas.addEventListener("contextmenu",preventMenu);
 // Cursor-anchored dolly: movement along the camera-to-surface ray preserves
 // the picked location's screen position, including tilted views.
 const cursor=(e:MouseEvent)=>{const b=canvas.getBoundingClientRect();return new c.Cartesian2(e.clientX-b.left,e.clientY-b.top)};
 const dolly=(pos:Cesium.Cartesian2,steps:number)=>{
  const camera=widget.camera,target=camera.pickEllipsoid(pos);if(!target)return;
  camera.cancelFlight();
  const delta=c.Cartesian3.subtract(target,camera.positionWC,new c.Cartesian3()),distance=c.Cartesian3.magnitude(delta);
  if(distance<3||!Number.isFinite(distance))return;
  const factor=Math.exp(clamp(steps,-.75,.75)),height=camera.positionCartographic.height;
  const next=Math.max(3,Math.min(8e7,distance*factor));
  // Prevent below-ellipsoid zoom and limit the maximum orbital altitude.
  if(next<distance&&height<=3)return;
  const move=distance-next,unit=c.Cartesian3.normalize(delta,new c.Cartesian3()),proposed=c.Cartesian3.add(camera.positionWC,c.Cartesian3.multiplyByScalar(unit,move,new c.Cartesian3()),new c.Cartesian3()),h=c.Cartographic.fromCartesian(proposed)?.height;
  if(h===undefined||h<3||h>8e7)return;
  camera.move(unit,move);scene.requestRender();
 };
 // Mouse-only controller. Touch/pinch continue through Cesium's native adapter.
 type Drag={id:number;mode:"pan"|"orbit"|"zoom";start:Cesium.Cartesian2;previous:Cesium.Cartesian2;anchor?:Cesium.Cartesian3;target?:Cesium.Cartesian3;moved:boolean};
 let drag:Drag|undefined,lastRight=0;
 const cam=widget.camera;
 const setPose=(position:Cesium.Cartesian3,direction:Cesium.Cartesian3,up:Cesium.Cartesian3)=>{
  const height=c.Cartographic.fromCartesian(position)?.height;
  if(height===undefined||!Number.isFinite(height)||height<3||height>8e7)return;
  cam.setView({destination:position,orientation:{direction,up}});
  scene.requestRender();
 };
 const rotateWorld=(axis:Cesium.Cartesian3,angle:number,preserveHeight:boolean)=>{
  if(!Number.isFinite(angle)||Math.abs(angle)<1e-9)return;
  const matrix=c.Matrix3.fromQuaternion(c.Quaternion.fromAxisAngle(axis,angle));
  const transform=(v:Cesium.Cartesian3)=>c.Matrix3.multiplyByVector(matrix,v,new c.Cartesian3());
  let position=transform(cam.positionWC);
  if(preserveHeight){const ll=c.Cartographic.fromCartesian(position);if(!ll)return;ll.height=cam.positionCartographic.height;position=c.Cartographic.toCartesian(ll)}
  setPose(position,transform(cam.directionWC),transform(cam.upWC));
 };
 const pan=(d:Drag,point:Cesium.Cartesian2,dx:number,dy:number)=>{
  const hit=cam.pickEllipsoid(point);
  if(hit&&d.anchor){
   const from=c.Cartesian3.normalize(hit,new c.Cartesian3()),to=c.Cartesian3.normalize(d.anchor,new c.Cartesian3());
   const axis=c.Cartesian3.cross(from,to,new c.Cartesian3()),length=c.Cartesian3.magnitude(axis);
   if(length>1e-10){rotateWorld(c.Cartesian3.divideByScalar(axis,length,axis),Math.atan2(length,c.Cartesian3.dot(from,to)),true);return}
  }
  // Off-globe drag behaves like a virtual trackball, not a locked geographic pan.
  const scale=Math.min(.006,Math.max(.00000001,cam.positionCartographic.height/6378137/Math.max(1,canvas.clientHeight)*1.5));
  rotateWorld(c.Cartesian3.normalize(cam.upWC,new c.Cartesian3()),-dx*scale,true);
  rotateWorld(c.Cartesian3.normalize(cam.rightWC,new c.Cartesian3()),dy*scale,true);
  d.anchor=cam.pickEllipsoid(point);
 };
 const orbit=(d:Drag,dx:number,dy:number)=>{
  const pivot=d.target;if(!pivot)return;
  const frame=c.Transforms.eastNorthUpToFixedFrame(pivot),offset=c.Matrix4.multiplyByPoint(c.Matrix4.inverseTransformation(frame,new c.Matrix4()),cam.positionWC,new c.Cartesian3());
  const range=c.Cartesian3.magnitude(offset);if(range<3)return;
  const heading=Math.hypot(offset.x,offset.y)<.001?cam.heading:Math.atan2(-offset.x,-offset.y);
  const tilt=Math.acos(clamp(offset.z/range,-1,1));
  const h=heading-dx*.008,t=clamp(tilt+dy*.008,0,c.Math.toRadians(85));
  const local=new c.Cartesian3(-Math.sin(h)*Math.sin(t)*range,-Math.cos(h)*Math.sin(t)*range,Math.cos(t)*range);
  const position=c.Matrix4.multiplyByPoint(frame,local,new c.Cartesian3());
  const direction=c.Cartesian3.normalize(c.Cartesian3.subtract(pivot,position,new c.Cartesian3()),new c.Cartesian3());
  const rightLocal=new c.Cartesian3(Math.cos(h),-Math.sin(h),0),rightWorld=c.Matrix4.multiplyByPointAsVector(frame,rightLocal,new c.Cartesian3());
  const up=c.Cartesian3.normalize(c.Cartesian3.cross(rightWorld,direction,new c.Cartesian3()),new c.Cartesian3());
  setPose(position,direction,up);
 };
 const down=(e:PointerEvent)=>{
  if(e.pointerType!=="mouse"||![0,1,2].includes(e.button))return;
  e.preventDefault();e.stopImmediatePropagation();cam.cancelFlight();cameraPins.delete(widget);
  const point=cursor(e),mode=e.button===2?"zoom":e.button===1||e.ctrlKey||e.shiftKey?"orbit":"pan";
  const center=new c.Cartesian2(canvas.clientWidth/2,canvas.clientHeight/2);
  drag={id:e.pointerId,mode,start:point,previous:point,anchor:cam.pickEllipsoid(point),target:cam.pickEllipsoid(center)??cam.pickEllipsoid(point),moved:false};
  canvas.focus({preventScroll:true});canvas.setPointerCapture(e.pointerId);
 };
 const move=(e:PointerEvent)=>{
  if(!drag||e.pointerId!==drag.id)return;
  e.preventDefault();e.stopImmediatePropagation();const point=cursor(e);
  if(!drag.moved&&c.Cartesian2.distance(point,drag.start)<3)return;
  drag.moved=true;const dx=point.x-drag.previous.x,dy=point.y-drag.previous.y;drag.previous=point;
  if(drag.mode==="pan")pan(drag,point,dx,dy);
  else if(drag.mode==="orbit")orbit(drag,dx,dy);
  else dolly(drag.start,-dy*.008);
 };
 const up=(e:PointerEvent)=>{
  if(!drag||e.pointerId!==drag.id)return;
  e.preventDefault();e.stopImmediatePropagation();const d=drag;drag=undefined;
  if(!d.moved&&e.type==="pointerup"){
   if(d.mode==="zoom"){const now=performance.now();if(now-lastRight<350){dolly(d.start,.55);lastRight=0}else lastRight=now}
   else if(d.mode==="pan"){const action=widget.screenSpaceEventHandler.getInputAction(c.ScreenSpaceEventType.LEFT_CLICK);if(action)(action as (e:{position:Cesium.Cartesian2})=>void)({position:d.start})}
  }
  if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);
 };
 const wheel=(e:WheelEvent)=>{if(e.ctrlKey||e.shiftKey)return;e.preventDefault();e.stopImmediatePropagation();cameraPins.delete(widget);dolly(cursor(e),e.deltaY*(e.deltaMode===1?.045:e.deltaMode===2?.5:.0018))};
 const lost=()=>{drag=undefined};canvas.addEventListener("lostpointercapture",lost);canvas.addEventListener("pointerdown",down,true);canvas.addEventListener("pointermove",move,true);canvas.addEventListener("pointerup",up,true);canvas.addEventListener("pointercancel",up,true);canvas.addEventListener("wheel",wheel,{capture:true,passive:false});

 const keyboard=(e:KeyboardEvent)=>{if(e.ctrlKey||e.altKey||e.metaKey)return;const key=e.key.toLowerCase();if(!["n","u","r"," ","+","=","-","_"].includes(key))return;e.preventDefault();e.stopPropagation();if(key==="n")north();else if(key==="r"||key==="u")top();else if(key===" "){widget.camera.cancelFlight();}else zoom(key==="+"||key==="="?1:-1);scene.requestRender()};
 widget.canvas.addEventListener("keydown",keyboard);
 widget.screenSpaceEventHandler.setInputAction((e:{position:Cesium.Cartesian2})=>{
  const target=widget.camera.pickEllipsoid(e.position);if(!target)return;
  const v=getCamera(c,widget),range=rangeFor(clamp(v.zoom+1,0,22),v.latitude);
  widget.camera.flyToBoundingSphere(new c.BoundingSphere(target,1),{offset:new c.HeadingPitchRange(c.Math.toRadians(v.bearing),c.Math.toRadians((v.pitch??0)-90),range),duration:.55,maximumHeight:Math.max(range*2,20)});scene.requestRender();
 },c.ScreenSpaceEventType.LEFT_DOUBLE_CLICK);
 resolution();
 return ()=>{removeHud();resize.disconnect();canvas.removeEventListener("lostpointercapture",lost);canvas.removeEventListener("pointerdown",down,true);canvas.removeEventListener("pointermove",move,true);canvas.removeEventListener("pointerup",up,true);canvas.removeEventListener("pointercancel",up,true);canvas.removeEventListener("wheel",wheel,true);widget.canvas.removeEventListener("keydown",keyboard);widget.canvas.removeEventListener("contextmenu",preventMenu)};
}
function meshPrimitive(c:C,groups:MeshGroup[]):Cesium.Primitive{
 const instances=groups.filter(g=>g.positions.length>=9).map(g=>{
  const attributes=new c.GeometryAttributes();
  attributes.position=new c.GeometryAttribute({componentDatatype:c.ComponentDatatype.DOUBLE,componentsPerAttribute:3,values:new Float64Array(g.positions)});
  return new c.GeometryInstance({id:g.id,geometry:new c.Geometry({attributes,primitiveType:c.PrimitiveType.TRIANGLES,boundingSphere:c.BoundingSphere.fromVertices(g.positions)}),attributes:{color:c.ColorGeometryInstanceAttribute.fromColor(g.color)}});
 });
 if(!instances.length)throw Error("No valid mesh triangles");
 return new c.Primitive({geometryInstances:instances,appearance:new c.PerInstanceColorAppearance({flat:true,translucent:false,closed:false}),asynchronous:false});
}
function terrainGroups(c:C,g:TerrainGrid):MeshGroup[]{
 const [l,b,r,t]=g.extent,points=g.values.map((h,i)=>h===null?null:c.Cartesian3.fromDegrees(...geographic3857(l+(r-l)*(i%g.width)/(g.width-1),t-(t-b)*Math.floor(i/g.width)/(g.height-1)),h-g.render_origin_m));
 const bins=new Map<number,MeshGroup>(),span=Math.max(1,g.max_m-g.min_m),indices=terrainTriangles(g);
 for(let i=0;i<indices.length;i+=3){
  const k=[indices[i]!,indices[i+1]!,indices[i+2]!],h=k.reduce((s,n)=>s+g.values[n]!,0)/3,q=clamp(Math.round((h-g.min_m)/span*15),0,15),f=q/15;
  let group=bins.get(q);if(!group){group={id:{kind:"terrain",id:"terrain"},positions:[],color:new c.Color(.17+.55*f,.39+.36*f,.22+.39*f,1)};bins.set(q,group)}
  for(const n of k){const p=points[n]!;group.positions.push(p.x,p.y,p.z)}
 }
 return [...bins.values()];
}
function modelGroups(c:C,id:string,p:PlacedModel):MeshGroup[]{
 if(![p.longitude,p.latitude,p.altitude,p.rotationDeg,p.scale].every(Number.isFinite)||Math.abs(p.longitude)>180||Math.abs(p.latitude)>85||p.altitude< -12000||p.altitude>22000||Math.abs(p.rotationDeg)>360||p.scale<.01||p.scale>100)throw Error("Invalid model placement");
 const origin=c.Cartesian3.fromDegrees(p.longitude,p.latitude,p.altitude),transform=c.Transforms.eastNorthUpToFixedFrame(origin),a=p.rotationDeg*Math.PI/180,co=Math.cos(a),si=Math.sin(a),v=p.model.positions,colors=p.model.colors,bins=new Map<number,MeshGroup>();
 for(let i=0;i<v.length;i+=9){
  // Quantize triangle-average colors into 64 bounded material groups.
  const rgb=[0,1,2].map(j=>clamp(Math.round((colors[i+j]!+colors[i+3+j]!+colors[i+6+j]!)/3*3),0,3));
  const key=rgb[0]!*16+rgb[1]!*4+rgb[2]!;
  let group=bins.get(key);if(!group){group={id:{kind:"model",id},positions:[],color:new c.Color(rgb[0]!/3,rgb[1]!/3,rgb[2]!/3,1)};bins.set(key,group)}
  for(let j=i;j<i+9;j+=3){
   const x=v[j]!*p.scale,y=v[j+1]!*p.scale,z=v[j+2]!*p.scale;
   if(Math.max(Math.abs(x),Math.abs(y),Math.abs(z))>10000)throw Error("Model exceeds 10 km");
   const point=c.Matrix4.multiplyByPoint(transform,new c.Cartesian3(x*co-z*si,-x*si-z*co,y),new c.Cartesian3());
   group.positions.push(point.x,point.y,point.z);
  }
 }
 return [...bins.values()];
}
export async function prepareCesium3d(host:HTMLElement,view:FootprintView,camera:GeographicCamera,signal:AbortSignal,onPick:(id:string)=>void,extras?:SceneOptions):Promise<SelectableFootprintEngine>{
 validCamera(camera);if(signal.aborted)throw abortError();
 const models=extras?.models??(extras?.model?[{id:"local",placement:extras.model}]:[]);checkSceneBudget(models.map(v=>v.placement.model));
 if(new Set(models.map(v=>v.id)).size!==models.length||models.some(v=>!v.id||v.id==="terrain"||v.id==="footprints"))throw Error("Invalid model identities");
 const c=await loadCesium();if(signal.aborted)throw abortError();
 c.CreditDisplay.cesiumCredit=new c.Credit('<a href="https://cesium.com/platform/cesiumjs/" target="_blank" rel="noopener" title="CesiumJS · self-hosted Apache-2.0">CesiumJS</a>',true);
 const surface=document.createElement("div");surface.className="ovg-cesium-3d";surface.dataset.ovg3dEngine="cesium";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5;overflow:hidden";
 host.append(surface);
 let widget:Cesium.CesiumWidget|undefined,disposed=false,active=false,cleanupNav:(()=>void)|undefined;
 const modelLayers=new Map<string,{primitive:Cesium.Primitive;groups:MeshGroup[]}>();
 const footprintEntities=new Map<string,Cesium.Entity[]>();
 let terrain:Cesium.Primitive|undefined,footprintsVisible=true,selectedModel="";
 const vectorEntities=new Map<string,{items:Cesium.Entity[];color:Cesium.Color}>();
 const replaceVectors=(rows:readonly SharedVector[],selected:string)=>{
  if(!widget||disposed)return;
  for(const item of vectorEntities.values())for(const entity of item.items)widget.entities.remove(entity);
  vectorEntities.clear();
  const positions=(coords:number[][])=>c.Cartesian3.fromDegreesArray(coords.flatMap(p=>[p[0]!,p[1]!]));
  let vertices=0;
  for(const row of rows.slice(0,5000)){
   if(footprintEntities.has(row.id))continue;
   const items:Cesium.Entity[]=[],color=c.Color.fromCssColorString(row.color)??c.Color.DODGERBLUE;
   const add=(g:import("geojson").Geometry):void=>{
    if(!widget||vertices>=100000)return;
    const base={properties:{sourceId:row.id}};
    if(g.type==="Point"){vertices++;items.push(widget.entities.add({...base,position:c.Cartesian3.fromDegrees(g.coordinates[0]!,g.coordinates[1]!),point:{pixelSize:7,color,outlineColor:c.Color.WHITE,outlineWidth:1,disableDepthTestDistance:1000}}))}
    else if(g.type==="LineString"){vertices+=g.coordinates.length;if(vertices<=100000)items.push(widget.entities.add({...base,polyline:{positions:positions(g.coordinates),width:2,material:color,clampToGround:true}}))}
    else if(g.type==="Polygon"){vertices+=g.coordinates.reduce((n,r)=>n+r.length,0);if(vertices<=100000&&g.coordinates[0]?.length>=4)items.push(widget.entities.add({...base,polygon:{hierarchy:new c.PolygonHierarchy(positions(g.coordinates[0]),g.coordinates.slice(1).map(r=>new c.PolygonHierarchy(positions(r)))),material:color.withAlpha(.4),outline:false}}))}
    else if(g.type==="MultiPoint")g.coordinates.forEach(coordinates=>add({type:"Point",coordinates}));
    else if(g.type==="MultiLineString")g.coordinates.forEach(coordinates=>add({type:"LineString",coordinates}));
    else if(g.type==="MultiPolygon")g.coordinates.forEach(coordinates=>add({type:"Polygon",coordinates}));
    else if(g.type==="GeometryCollection")g.geometries.forEach(add);
   };
   try{add(row.geometry)}catch{/* invalid source geometry is not rendered */}
   if(items.length)vectorEntities.set(row.id,{items,color});
   if(vertices>=100000)break;
  }
  surface.dataset.ovgVectorCount=String(vectorEntities.size);surface.dataset.ovgVectorLimited=String(rows.length>5000||vertices>=100000);select(selected);
 };
 const render=()=>widget?.scene.requestRender();
 const dispose=()=>{if(disposed)return;disposed=true;active=false;cleanupNav?.();if(widget&&!widget.isDestroyed())widget.destroy();surface.remove()};
 const color=c.Color.fromCssColorString("#326dba"),highlight=c.Color.fromCssColorString("#f59e0b");
 const select=(id:string)=>{for(const [key,entities] of footprintEntities)for(const entity of entities)if(entity.polygon)entity.polygon.material=new c.ColorMaterialProperty(key===id?highlight:color);for(const [key,v] of vectorEntities)for(const entity of v.items){const tint=key===id?highlight:v.color;if(entity.point)entity.point.color=new c.ConstantProperty(tint);if(entity.polyline)entity.polyline.material=new c.ColorMaterialProperty(tint);if(entity.polygon)entity.polygon.material=new c.ColorMaterialProperty(tint.withAlpha(.4))}render()};
 const applyModelHighlight=()=>{for(const [id,{primitive,groups}] of modelLayers){if(!primitive.ready)continue;for(const group of groups){const a=primitive.getGeometryInstanceAttributes(group.id);if(a)a.color=c.ColorGeometryInstanceAttribute.toValue(id===selectedModel?highlight:group.color,a.color)}}};
 const selectModel=(id:string)=>{selectedModel=id;applyModelHighlight();render()};
 const setLayerVisible=(id:string,visible:boolean)=>{if(id==="footprints"){footprintsVisible=visible;for(const items of footprintEntities.values())for(const item of items)item.show=visible}else if(id==="terrain"&&terrain)terrain.show=visible;else{const m=modelLayers.get(id);if(m)m.primitive.show=visible}render()};
 try{
  widget=new c.CesiumWidget(surface,{baseLayer:false,terrainProvider:new c.EllipsoidTerrainProvider(),skyBox:false,skyAtmosphere:false,scene3DOnly:true,requestRenderMode:true,maximumRenderTimeChange:Infinity,showRenderLoopErrors:false,shouldAnimate:false,targetFrameRate:60,useBrowserRecommendedResolution:false,msaaSamples:1});
  const scene=widget.scene;scene.backgroundColor=c.Color.fromCssColorString("#dce6e8");scene.globe.baseColor=c.Color.fromCssColorString("#dce6e8");scene.globe.depthTestAgainstTerrain=true;
  scene.fog.enabled=false;scene.globe.enableLighting=false;
  // Bundled low-resolution Natural Earth imagery only; no external basemap requests.
  const base=await c.TileMapServiceImageryProvider.fromUrl((globalThis as typeof globalThis&{CESIUM_BASE_URL:string}).CESIUM_BASE_URL+"Assets/Textures/NaturalEarthII");
  if(signal.aborted)throw abortError();
  widget.imageryLayers.addImageryProvider(base);
  for(const f of view.features){
   const polygons=f.geometry.type==="Polygon"?[f.geometry.coordinates]:f.geometry.coordinates,entities:Cesium.Entity[]=[];
   for(const [i,rings] of polygons.entries()){
    const positions=(ring:number[][])=>c.Cartesian3.fromDegreesArray(ring.flatMap(p=>[p[0]!,p[1]!]));
    const hierarchy=new c.PolygonHierarchy(positions(rings[0]!),rings.slice(1).map(r=>new c.PolygonHierarchy(positions(r))));
    const entity=widget.entities.add({id:"footprint:"+f.properties.featureId+":"+i,properties:{sourceId:f.properties.featureId},polygon:{hierarchy,height:f.properties.base,extrudedHeight:f.properties.top,material:color,arcType:c.ArcType.GEODESIC}});
    entities.push(entity);
   }
   footprintEntities.set(f.properties.featureId,entities);
  }
  replaceVectors(extras?.vectors??[],"");
  if(extras?.terrain){terrain=meshPrimitive(c,terrainGroups(c,extras.terrain));scene.primitives.add(terrain)}
  for(const m of models){const groups=modelGroups(c,m.id,m.placement),primitive=meshPrimitive(c,groups);scene.primitives.add(primitive);modelLayers.set(m.id,{primitive,groups})}
  setCamera(c,widget,camera);
  cleanupNav=addNavigation(c,widget,surface,camera,extras?.imagery,extras?.legacyImagery,extras?.initialBasemap,extras?.onBasemapChange);
  let waitingForModels=true;scene.postRender.addEventListener(()=>{if(waitingForModels&&[...modelLayers.values()].every(v=>v.primitive.ready)){waitingForModels=false;applyModelHighlight();render()}});
  widget.screenSpaceEventHandler.setInputAction((e:{position:Cesium.Cartesian2})=>{
   if(!active||disposed||!widget)return;
   const hit=scene.pick(e.position),id=hit?.id;
   if(id instanceof c.Entity){const key=id.properties?.sourceId?.getValue(widget.clock.currentTime);if(typeof key==="string"&&(vectorEntities.has(key)||(footprintsVisible&&footprintEntities.has(key)))){selectModel("");select(key);onPick(key)}}
   else if(id?.kind==="model"&&modelLayers.has(id.id)){select("");selectModel(id.id);extras?.onModelPick?.(id.id)}
  },c.ScreenSpaceEventType.LEFT_CLICK);
  // Render once before the mode transaction can commit.
  widget.resize();widget.render();
  if(signal.aborted)throw abortError();
  return {select,selectModel,setLayerVisible,replaceVectors,getCamera(){if(!widget||disposed)throw Error("Disposed Cesium");return getCamera(c,widget)},activate(){if(disposed||!widget)throw Error("Disposed Cesium");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";widget.resize();render()},dispose};
 }catch(e){dispose();throw e}
}

/** Native Cesium tile streaming. Generated triangle-provenance tiles remain on the
 * legacy adapter until native feature-id semantics are equivalent. */
export async function prepareCesiumTiles3d(host:HTMLElement,root:Tile,manifestPath:string,signal:AbortSignal,status:(message:string)=>void,imagery?:readonly ProjectImagery[],legacyImagery?:readonly LegacyImagery[]):Promise<SelectableTileEngine>{
 const url=new URL(manifestPath,location.origin);
 if(url.origin!==location.origin||!/^\/api\/v1\/organizations\/[A-Za-z0-9_-]+\/projects\/[A-Za-z0-9_-]+\/3d\/tilesets\/[A-Za-z0-9_-]+\/tileset\.json$/.test(url.pathname)||url.search||url.hash)throw Error("Invalid project tileset path");
 const c=await loadCesium();if(signal.aborted)throw abortError();
 c.CreditDisplay.cesiumCredit=new c.Credit('<a href="https://cesium.com/platform/cesiumjs/" target="_blank" rel="noopener" title="CesiumJS · self-hosted Apache-2.0">CesiumJS</a>',true);
 const surface=document.createElement("div");surface.className="ovg-cesium-3d";surface.dataset.ovg3dEngine="cesium-tiles";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5;overflow:hidden";host.append(surface);
 let widget:Cesium.CesiumWidget|undefined,tiles:Cesium.Cesium3DTileset|undefined,disposed=false,active=false,failed=0,loaded=0,cleanupNav:(()=>void)|undefined;
 const dispose=()=>{if(disposed)return;disposed=true;active=false;cleanupNav?.();if(widget&&!widget.isDestroyed())widget.destroy();if(tiles&&!tiles.isDestroyed())tiles.destroy();surface.remove()};
 try{
  widget=new c.CesiumWidget(surface,{baseLayer:false,terrainProvider:new c.EllipsoidTerrainProvider(),skyBox:false,skyAtmosphere:false,scene3DOnly:true,requestRenderMode:true,maximumRenderTimeChange:Infinity,showRenderLoopErrors:false,shouldAnimate:false,targetFrameRate:60,useBrowserRecommendedResolution:false,msaaSamples:1});
  widget.scene.globe.baseColor=c.Color.fromCssColorString("#dce6e8");
  const base=await c.TileMapServiceImageryProvider.fromUrl((globalThis as typeof globalThis&{CESIUM_BASE_URL:string}).CESIUM_BASE_URL+"Assets/Textures/NaturalEarthII");
  if(signal.aborted)throw abortError();widget.imageryLayers.addImageryProvider(base);
  tiles=await c.Cesium3DTileset.fromUrl(url.href,{maximumScreenSpaceError:24,cacheBytes:64*1024*1024,maximumCacheOverflowBytes:64*1024*1024});
  if(signal.aborted)throw abortError();widget.scene.primitives.add(tiles);
  const r=root.region,longitude=c.Math.toDegrees((r[0]+r[2])/2),latitude=c.Math.toDegrees((r[1]+r[3])/2),span=Math.max(.0001,c.Math.toDegrees(r[2]-r[0]),c.Math.toDegrees(r[3]-r[1])/Math.max(.08,Math.cos(latitude*Math.PI/180)));
  const initial={longitude,latitude,zoom:clamp(Math.log2(180/span),0,19),bearing:0,pitch:45};setCamera(c,widget,initial);cleanupNav=addNavigation(c,widget,surface,initial,imagery,legacyImagery);
  tiles.tileLoad.addEventListener(()=>{loaded++;if(active&&!disposed)status("CesiumJS · đã tải nội dung tile"+(failed?" · "+failed+" lỗi":""))});
  tiles.tileFailed.addEventListener((e:{message?:string})=>{failed++;console.warn("Cesium tile failed:",e.message?.slice(0,300)??"Unknown error");if(active&&!disposed)status("CesiumJS · "+failed+" tile lỗi hoặc không có quyền truy cập")});
  tiles.allTilesLoaded.addEventListener(()=>{if(active&&!disposed&&!failed)status("CesiumJS · 3D Tiles sẵn sàng")});
  widget.resize();widget.render();if(signal.aborted)throw abortError();
  return {select(){},selectModel(){},clearFeature(){},highlightFeature(){return false},async focusFeature(){return false},getCamera(){if(!widget||disposed)throw Error("Disposed Cesium");return getCamera(c,widget)},setLayerVisible(id,value){if(id==="tileset"&&tiles&&!disposed){tiles.show=value;widget?.scene.requestRender()}},activate(){if(disposed||!widget)throw Error("Disposed Cesium");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";widget.resize();status(loaded?"CesiumJS · đã tải nội dung tile":failed?"CesiumJS · "+failed+" tile lỗi hoặc không có quyền truy cập":"CesiumJS · đang tải 3D Tiles…");widget.scene.requestRender()},dispose};
 }catch(e){dispose();throw e}
}
