/** Cesium geographic viewer, using validated local scene inputs and no ion/CDN. */
import type * as Cesium from "cesium";
import type {FootprintView} from "./footprint3d";
import type {GeographicCamera,NamedModel,SelectableFootprintEngine} from "./maplibre3d";
import type {Tile} from "./tileset";
import type {SelectableTileEngine} from "./tiles3d";
import type {PlacedModel} from "./mesh-layer";
import {checkSceneBudget} from "./mesh-picking";
import {geographic3857,terrainTriangles,type TerrainGrid} from "./terrain";

type C=typeof Cesium;
type Pick={kind:"footprint"|"model"|"terrain";id:string};
type MeshGroup={id:Pick;positions:number[];color:Cesium.Color};
type SceneOptions={terrain?:TerrainGrid;model?:PlacedModel;models?:readonly NamedModel[];onModelPick?:(id:string)=>void};
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
function getCamera(c:C,widget:Cesium.CesiumWidget):GeographicCamera{
 const camera=widget.camera,canvas=widget.canvas,center=new c.Cartesian2(canvas.clientWidth/2,canvas.clientHeight/2),pin=cameraPins.get(widget);
 if(pin&&c.Cartesian3.distance(pin.position,camera.positionWC)<.001&&c.Cartesian3.distance(pin.direction,camera.directionWC)<1e-8&&c.Cartesian3.distance(pin.up,camera.upWC)<1e-8)return {...pin.view};
 const target=camera.pickEllipsoid(center)??c.Cartesian3.fromRadians(camera.positionCartographic.longitude,camera.positionCartographic.latitude);
 const ll=c.Cartographic.fromCartesian(target),latitude=clamp(c.Math.toDegrees(ll.latitude),-85,85);
 const offset=c.Matrix4.multiplyByPoint(c.Matrix4.inverseTransformation(c.Transforms.eastNorthUpToFixedFrame(target),new c.Matrix4()),camera.positionWC,new c.Cartesian3()),range=c.Cartesian3.magnitude(offset);
 const heading=Math.hypot(offset.x,offset.y)>.00001?Math.atan2(-offset.x,-offset.y):camera.heading;
 const pitch=range>0?Math.asin(clamp(-offset.z/range,-1,1)):camera.pitch;
 return {longitude:c.Math.toDegrees(ll.longitude),latitude,zoom:clamp(Math.log2(40075016.6856*Math.max(.08,Math.cos(ll.latitude))/Math.max(5,range)),0,22),bearing:c.Math.toDegrees(c.Math.negativePiToPi(heading)),pitch:clamp(90+c.Math.toDegrees(pitch),0,75)};
}
function addNavigation(c:C,widget:Cesium.CesiumWidget,surface:HTMLElement,initial:GeographicCamera){
 const tools=document.createElement("div");tools.className="ovg-cesium-navigation";
 const button=(label:string,content:string,action:()=>void)=>{const b=document.createElement("button");b.type="button";b.title=label;b.setAttribute("aria-label",label);b.textContent=content;b.onclick=()=>{action();widget.scene.requestRender()};tools.append(b)};
 const zoom=(factor:number)=>{const v=getCamera(c,widget);setCamera(c,widget,{...v,zoom:clamp(v.zoom+factor,0,22)})};
 button("Phóng to 3D","+",()=>zoom(1));button("Thu nhỏ 3D","−",()=>zoom(-1));
 button("Hướng Bắc","N",()=>setCamera(c,widget,{...getCamera(c,widget),bearing:0}));button("Về góc nhìn ban đầu","⌂",()=>setCamera(c,widget,initial));surface.append(tools);
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
 const surface=document.createElement("div");surface.className="ovg-cesium-3d";surface.dataset.ovg3dEngine="cesium";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5;overflow:hidden";
 host.append(surface);
 let widget:Cesium.CesiumWidget|undefined,disposed=false,active=false;
 const modelLayers=new Map<string,{primitive:Cesium.Primitive;groups:MeshGroup[]}>();
 const footprintEntities=new Map<string,Cesium.Entity[]>();
 let terrain:Cesium.Primitive|undefined,footprintsVisible=true,selectedModel="";
 const render=()=>widget?.scene.requestRender();
 const dispose=()=>{if(disposed)return;disposed=true;active=false;if(widget&&!widget.isDestroyed())widget.destroy();surface.remove()};
 const color=c.Color.fromCssColorString("#326dba"),highlight=c.Color.fromCssColorString("#f59e0b");
 const select=(id:string)=>{for(const [key,entities] of footprintEntities)for(const entity of entities)if(entity.polygon)entity.polygon.material=new c.ColorMaterialProperty(key===id?highlight:color);render()};
 const applyModelHighlight=()=>{for(const [id,{primitive,groups}] of modelLayers){if(!primitive.ready)continue;for(const group of groups){const a=primitive.getGeometryInstanceAttributes(group.id);if(a)a.color=c.ColorGeometryInstanceAttribute.toValue(id===selectedModel?highlight:group.color,a.color)}}};
 const selectModel=(id:string)=>{selectedModel=id;applyModelHighlight();render()};
 const setLayerVisible=(id:string,visible:boolean)=>{if(id==="footprints"){footprintsVisible=visible;for(const items of footprintEntities.values())for(const item of items)item.show=visible}else if(id==="terrain"&&terrain)terrain.show=visible;else{const m=modelLayers.get(id);if(m)m.primitive.show=visible}render()};
 try{
  widget=new c.CesiumWidget(surface,{baseLayer:false,terrainProvider:new c.EllipsoidTerrainProvider(),skyBox:false,skyAtmosphere:false,scene3DOnly:true,requestRenderMode:true,maximumRenderTimeChange:Infinity,showRenderLoopErrors:false,shouldAnimate:false,targetFrameRate:30});
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
  if(extras?.terrain){terrain=meshPrimitive(c,terrainGroups(c,extras.terrain));scene.primitives.add(terrain)}
  for(const m of models){const groups=modelGroups(c,m.id,m.placement),primitive=meshPrimitive(c,groups);scene.primitives.add(primitive);modelLayers.set(m.id,{primitive,groups})}
  setCamera(c,widget,camera);
  addNavigation(c,widget,surface,camera);
  let waitingForModels=true;scene.postRender.addEventListener(()=>{if(waitingForModels&&[...modelLayers.values()].every(v=>v.primitive.ready)){waitingForModels=false;applyModelHighlight();render()}});
  widget.screenSpaceEventHandler.setInputAction((e:{position:Cesium.Cartesian2})=>{
   if(!active||disposed||!widget)return;
   const hit=scene.pick(e.position),id=hit?.id;
   if(id instanceof c.Entity){const key=id.properties?.sourceId?.getValue(widget.clock.currentTime);if(footprintsVisible&&typeof key==="string"&&footprintEntities.has(key)){selectModel("");select(key);onPick(key)}}
   else if(id?.kind==="model"&&modelLayers.has(id.id)){select("");selectModel(id.id);extras?.onModelPick?.(id.id)}
  },c.ScreenSpaceEventType.LEFT_CLICK);
  // Render once before the mode transaction can commit.
  widget.resize();widget.render();
  if(signal.aborted)throw abortError();
  return {select,selectModel,setLayerVisible,getCamera(){if(!widget||disposed)throw Error("Disposed Cesium");return getCamera(c,widget)},activate(){if(disposed||!widget)throw Error("Disposed Cesium");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";widget.resize();render()},dispose};
 }catch(e){dispose();throw e}
}

/** Native Cesium tile streaming. Generated triangle-provenance tiles remain on the
 * legacy adapter until native feature-id semantics are equivalent. */
export async function prepareCesiumTiles3d(host:HTMLElement,root:Tile,manifestPath:string,signal:AbortSignal,status:(message:string)=>void):Promise<SelectableTileEngine>{
 const url=new URL(manifestPath,location.origin);
 if(url.origin!==location.origin||!/^\/api\/v1\/organizations\/[A-Za-z0-9_-]+\/projects\/[A-Za-z0-9_-]+\/3d\/tilesets\/[A-Za-z0-9_-]+\/tileset\.json$/.test(url.pathname)||url.search||url.hash)throw Error("Invalid project tileset path");
 const c=await loadCesium();if(signal.aborted)throw abortError();
 const surface=document.createElement("div");surface.className="ovg-cesium-3d";surface.dataset.ovg3dEngine="cesium-tiles";surface.style.cssText="position:absolute;inset:0;visibility:hidden;pointer-events:none;z-index:5;overflow:hidden";host.append(surface);
 let widget:Cesium.CesiumWidget|undefined,tiles:Cesium.Cesium3DTileset|undefined,disposed=false,active=false,failed=0,loaded=0;
 const dispose=()=>{if(disposed)return;disposed=true;active=false;if(widget&&!widget.isDestroyed())widget.destroy();if(tiles&&!tiles.isDestroyed())tiles.destroy();surface.remove()};
 try{
  widget=new c.CesiumWidget(surface,{baseLayer:false,terrainProvider:new c.EllipsoidTerrainProvider(),skyBox:false,skyAtmosphere:false,scene3DOnly:true,requestRenderMode:true,maximumRenderTimeChange:Infinity,showRenderLoopErrors:false,shouldAnimate:false,targetFrameRate:30});
  widget.scene.globe.baseColor=c.Color.fromCssColorString("#dce6e8");
  const base=await c.TileMapServiceImageryProvider.fromUrl((globalThis as typeof globalThis&{CESIUM_BASE_URL:string}).CESIUM_BASE_URL+"Assets/Textures/NaturalEarthII");
  if(signal.aborted)throw abortError();widget.imageryLayers.addImageryProvider(base);
  tiles=await c.Cesium3DTileset.fromUrl(url.href,{maximumScreenSpaceError:24,cacheBytes:64*1024*1024,maximumCacheOverflowBytes:64*1024*1024});
  if(signal.aborted)throw abortError();widget.scene.primitives.add(tiles);
  const r=root.region,longitude=c.Math.toDegrees((r[0]+r[2])/2),latitude=c.Math.toDegrees((r[1]+r[3])/2),span=Math.max(.0001,c.Math.toDegrees(r[2]-r[0]),c.Math.toDegrees(r[3]-r[1])/Math.max(.08,Math.cos(latitude*Math.PI/180)));
  const initial={longitude,latitude,zoom:clamp(Math.log2(180/span),0,19),bearing:0,pitch:45};setCamera(c,widget,initial);addNavigation(c,widget,surface,initial);
  tiles.tileLoad.addEventListener(()=>{loaded++;if(active&&!disposed)status("CesiumJS · đã tải nội dung tile"+(failed?" · "+failed+" lỗi":""))});
  tiles.tileFailed.addEventListener((e:{message?:string})=>{failed++;console.warn("Cesium tile failed:",e.message?.slice(0,300)??"Unknown error");if(active&&!disposed)status("CesiumJS · "+failed+" tile lỗi hoặc không có quyền truy cập")});
  tiles.allTilesLoaded.addEventListener(()=>{if(active&&!disposed&&!failed)status("CesiumJS · 3D Tiles sẵn sàng")});
  widget.resize();widget.render();if(signal.aborted)throw abortError();
  return {select(){},selectModel(){},clearFeature(){},highlightFeature(){return false},async focusFeature(){return false},getCamera(){if(!widget||disposed)throw Error("Disposed Cesium");return getCamera(c,widget)},setLayerVisible(id,value){if(id==="tileset"&&tiles&&!disposed){tiles.show=value;widget?.scene.requestRender()}},activate(){if(disposed||!widget)throw Error("Disposed Cesium");active=true;surface.style.visibility="visible";surface.style.pointerEvents="auto";widget.resize();status(loaded?"CesiumJS · đã tải nội dung tile":failed?"CesiumJS · "+failed+" tile lỗi hoặc không có quyền truy cập":"CesiumJS · đang tải 3D Tiles…");widget.scene.requestRender()},dispose};
 }catch(e){dispose();throw e}
}
