import {describe,it,expect} from "vitest";
import {resolveResumeMode,resumeModeKey} from "./workspace-resume";
const scope={accountId:"a",organizationId:"o",projectId:"p"};
describe("scoped workspace resume",()=>{
 it("reads only the current actor/project preference",()=>{const read=(k:string)=>k==="ovg:resume-mode:v1:a:o:p"?"cad":null;expect(resolveResumeMode(scope,false,read)).toBe("cad");expect(resolveResumeMode({...scope,accountId:"b"},false,read)).toBe("gis");expect(resolveResumeMode({...scope,projectId:"q"},false,read)).toBe("gis")});
 it("prioritizes explicit GIS workbench and printed QR",()=>expect(resolveResumeMode(scope,true,()=>"cad")).toBe("gis"));
 it("preserves 3D and handles corrupt/unavailable storage",()=>{expect(resolveResumeMode(scope,false,()=>"3d")).toBe("3d");expect(resolveResumeMode(scope,false,()=>"unknown")).toBe("gis");expect(resolveResumeMode(scope,false,()=>{throw Error()})).toBe("gis");expect(resumeModeKey(scope)).toBe("ovg:resume-mode:v1:a:o:p")});
});
