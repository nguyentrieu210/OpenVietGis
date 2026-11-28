import {describe,it,expect} from "vitest";
import {parseCadResume,cadResumeDocument,makeCadResume,validGlobeCamera,readGlobeCamera} from "./spatial-resume";
const doc={id:"d",version:2,sha256:"a".repeat(64),filename:"plan.dxf"};
describe("scoped spatial resume",()=>{
 it("only restores an exact current CAD document pin",()=>{
  const pin=parseCadResume(JSON.stringify(makeCadResume(doc)));expect(cadResumeDocument(pin,[doc])).toBe(doc);
  expect(cadResumeDocument(pin,[{...doc,version:3}])).toBeNull();expect(cadResumeDocument(pin,[{...doc,sha256:"b".repeat(64)}])).toBeNull();
  expect(cadResumeDocument(pin,[doc,doc])).toBeNull();expect(cadResumeDocument({kind:"vault"},[doc])).toBeNull();
  expect(makeCadResume({...doc,version:0})).toBeNull();expect(parseCadResume('{"kind":"document","id":"../x","version":2}')).toBeNull();
 });
 it("validates globe camera independently of scene data",()=>{
  expect(validGlobeCamera({longitude:106,latitude:11,zoom:12,bearing:350,pitch:30})).toEqual({longitude:106,latitude:11,zoom:12,bearing:-10,pitch:30});
  expect(validGlobeCamera({longitude:106,latitude:90,zoom:12,bearing:0})).toBeNull();
  expect(readGlobeCamera("bad")).toBeNull();
 });
});
