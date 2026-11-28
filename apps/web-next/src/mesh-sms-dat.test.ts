import {it,expect} from "vitest";
import {parse2dm} from "./local-mesh";
import {parseSmsDat} from "./mesh-sms-dat";
const mesh=parse2dm("MESH2D\nND 3 106 10 0\nND 1 106.1 10 0\nND 2 106 10.1 0\nE3T 1 3 1 2 1",4326);
const dat='DATASET\nOBJTYPE "mesh2d"\nBEGSCL\nND 3\nNC 1\nNAME "Depth"\nTIMEUNITS 2\nTS 0 0\n1\n2\n3\nTS 0 1\n4\n5\n6\nENDDS',origin="2026-01-01T00:00:00Z";
it("reads SMS scalar nodes in ID order and converts explicit UTC-relative hours",()=>{const d=parseSmsDat(dat,mesh,origin,"m");expect(d.frames).toEqual([[3,1,2],[6,4,5]]);expect(d.times[1]!-d.times[0]!).toBe(3600000);expect([d.min,d.max]).toEqual([1,6])});
it("rejects active face masks, count mismatch and implicit time origin",()=>{for(const s of [dat.replace("TS 0 1","TS 1 1"),dat.replace("NC 1","NC 2"),dat.replace("ENDDS","")])expect(()=>parseSmsDat(s,mesh,origin,"m")).toThrow();expect(()=>parseSmsDat(dat,mesh,"","m")).toThrow()});
