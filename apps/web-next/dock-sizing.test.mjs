import assert from "node:assert/strict";
import test from "node:test";
import {clampSize,separatorKeySize} from "./src/dock-sizing.ts";
test("dock respects narrow viewport even if configured minimum is larger",()=>{
 assert.equal(clampSize(400,220,156),156);
 assert.equal(clampSize(120,220,720),220);
 assert.equal(clampSize(500,220,720),500);
 assert.equal(clampSize(1000,220,720),720);
 assert.equal(clampSize(NaN,220,720),220);
});

test("separator keyboard follows edge, axis and viewport limits",()=>{
 assert.equal(separatorKeySize("ArrowLeft","x","start",300,200,600),320);
 assert.equal(separatorKeySize("ArrowRight","x","start",300,200,600),280);
 assert.equal(separatorKeySize("ArrowRight","x","end",300,200,600),320);
 assert.equal(separatorKeySize("ArrowUp","y","start",300,200,600),320);
 assert.equal(separatorKeySize("ArrowDown","y","end",300,200,600,5),305);
 assert.equal(separatorKeySize("ArrowLeft","y","end",300,200,600),null);
 assert.equal(separatorKeySize("Home","x","end",300,200,150),150);
 assert.equal(separatorKeySize("End","x","end",300,200,600),600);
 assert.equal(separatorKeySize("ArrowRight","x","end",595,200,600),600);
});
