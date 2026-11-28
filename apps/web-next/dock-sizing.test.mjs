import assert from "node:assert/strict";
import test from "node:test";
import {clampSize} from "./src/dock-sizing.ts";
test("dock respects narrow viewport even if configured minimum is larger",()=>{
 assert.equal(clampSize(400,220,156),156);
 assert.equal(clampSize(120,220,720),220);
 assert.equal(clampSize(500,220,720),500);
 assert.equal(clampSize(1000,220,720),720);
 assert.equal(clampSize(NaN,220,720),220);
});
