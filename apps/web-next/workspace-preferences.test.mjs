import assert from "node:assert/strict";
import test from "node:test";
import {preferenceKey,readCommandIds,recordRecent,fitFloatingPanel} from "./src/workspace-preferences.ts";
test("preferences are isolated by account, organization, project and section",()=>{
 const key=preferenceKey("a","o","p","dock");
 for(const args of [["b","o","p","dock"],["a","o2","p","dock"],["a","o","p2","dock"],["a","o","p","tools"]])assert.notEqual(preferenceKey(...args),key);
 assert.notEqual(preferenceKey("a:b","c","p","dock"),preferenceKey("a","b:c","p","dock"));
});
test("stored command ids reject malformed values and stay bounded",()=>{
 assert.deepEqual(readCommandIds('["SAVE","SAVE",1,"<x>","DIST"]'),["SAVE","DIST"]);
 assert.deepEqual(readCommandIds("bad"),[]);
 assert.deepEqual(recordRecent(["SAVE","DIST"],"DIST"),["DIST","SAVE"]);
});
test("floating panel fits completely within viewport",()=>{
 assert.deepEqual(fitFloatingPanel(320,120,720,420,390,400),{x:0,y:0,width:390,height:400});
 assert.deepEqual(fitFloatingPanel(350,380,100,100,390,400),{x:290,y:300,width:100,height:100});
});
