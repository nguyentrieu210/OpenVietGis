import test from "node:test";import assert from "node:assert/strict";
import {parseTileset,selectTiles,ecefToGeographic,tileVertex} from "../src/tileset.ts";
const transform=[0,1,0,0,0,0,1,0,1,0,0,0,6378137,0,0,1],region=[-.00003,-.00003,.00003,.00003,-1,50];
const leaf=(uri,r=region)=>({boundingVolume:{region:r},geometricError:0,content:{uri}});
const root={boundingVolume:{region},geometricError:10,refine:"REPLACE",transform,content:{uri:"coarse.glb"},children:[leaf("detail.glb")]};
const doc={asset:{version:"1.1"},geometricError:20,root};
const view={west:-1,south:-1,east:1,north:1,longitude:0,latitude:0,zoom:10};
test("region bounds, REPLACE refinement, capped deterministic selection",()=>{
 const parsed=parseTileset(doc);
 assert.equal(selectTiles(parsed,view).tiles[0].uri,"coarse.glb");
 assert.deepEqual(selectTiles(parsed,{...view,zoom:19}).tiles.map(t=>t.uri),["detail.glb"]);
 assert.equal(selectTiles(parsed,{...view,west:10,east:20}).tiles.length,0);
 const crowded=parseTileset({...doc,root:{...root,children:Array.from({length:12},(_,i)=>leaf("tile"+i+".glb"))}});
 assert.equal(selectTiles(crowded,{...view,zoom:19}).tiles.length,8);
 assert.equal(selectTiles(crowded,{...view,zoom:19}).omitted,4);
});
test("explicit ECEF transform, glTF y-up and WGS84 ellipsoid control points",()=>{
 const t=parseTileset(doc).children[0];assert.deepEqual(tileVertex(t,0,0,0),[0,0,0]);
 const [lon,lat,h]=tileVertex(t,0,25,0);assert.equal(lon,0);assert.equal(lat,0);assert.ok(Math.abs(h-25)<1e-6);
 assert.ok(tileVertex(t,10,0,0)[0]>0);assert.ok(tileVertex(t,0,0,10)[1]<0);
 const phi=10*Math.PI/180,lambda=106*Math.PI/180,a=6378137,e2=.0066943799901413165,N=a/Math.sqrt(1-e2*Math.sin(phi)**2);
 const p=ecefToGeographic((N+100)*Math.cos(phi)*Math.cos(lambda),(N+100)*Math.cos(phi)*Math.sin(lambda),(N*(1-e2)+100)*Math.sin(phi));
 assert.ok(Math.abs(p[0]-106)<1e-8&&Math.abs(p[1]-10)<1e-8&&Math.abs(p[2]-100)<1e-5);
 assert.throws(()=>tileVertex(t,100000,0,0));
});
test("reject unsupported URI, refinement and nested bounds",()=>{
 for(const bad of [{...root,content:{uri:"../escape.glb"}},{...root,refine:"ADD"},{...root,children:[leaf("x.glb",[-1,-1,1,1,0,1])]}])assert.throws(()=>parseTileset({...doc,root:bad}));
});
