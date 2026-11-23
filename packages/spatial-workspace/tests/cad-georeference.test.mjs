import {test} from "node:test";import assert from "node:assert/strict";
import {fitCadGeoreference,projectCadPoint} from "../src/cad-georeference.ts";
const controls=[{drawing:[0,0],projected:[500000,1300000]},{drawing:[10000,0],projected:[500000,1300010]},{drawing:[0,5000],projected:[499995,1300000]},{drawing:[10000,5000],projected:[499995,1300010]}];
const input={crsCode:5899,projectedMetres:true,units:"millimeter",controls,toleranceMetres:.01,maxScaleDeviation:.005};
test("millimetre CAD rotation and translation with verified residual",()=>{const fit=fitCadGeoreference(input);assert.ok(fit.maxResidualMetres<1e-8);const p=projectCadPoint(fit,[2000,3000]);assert.ok(Math.hypot(p[0]-499997,p[1]-1300002)<1e-8);assert.ok(Math.abs(fit.scale-1)<1e-8)});
test("missing/degree CRS rejected",()=>{assert.throws(()=>fitCadGeoreference({...input,crsCode:4326}));assert.throws(()=>fitCadGeoreference({...input,projectedMetres:false}))});
test("wrong unit and collinear points rejected",()=>{assert.throws(()=>fitCadGeoreference({...input,units:"meter"}));assert.throws(()=>fitCadGeoreference({...input,controls:[{drawing:[0,0],projected:[0,0]},{drawing:[1,0],projected:[1,0]},{drawing:[2,0],projected:[2,0]}]}))});
test("inconsistent control points and insufficient count rejected",()=>{assert.throws(()=>fitCadGeoreference({...input,controls:[...controls.slice(0,3),{drawing:[10000,5000],projected:[499995,1300011]}]}));assert.throws(()=>fitCadGeoreference({...input,controls:controls.slice(0,2)}))});
