import assert from "node:assert/strict";
import test from "node:test";
import { evalLabel } from "./src/label-expr.ts";

test("label expression functions", () => {
  const a = { so_nha: "15", hem: "", ten: "đường số 3", dt: "1234.567", trong: null };
  assert.equal(evalLabel("{so_nha} {hem|prefix:Hẻm }", a), "15 ");
  assert.equal(evalLabel("{ten|upper}", a), "ĐƯỜNG SỐ 3");
  assert.equal(evalLabel("{dt|round:1}", a), "1234.6");
  assert.equal(evalLabel("{dt|number}", a), "1.234,57");
  assert.equal(evalLabel("{trong|default:—}", a), "—");
  assert.equal(evalLabel("{so_nha|if:có số}", a), "có số");
  assert.equal(evalLabel("{dt|round:0|suffix: m²}", a), "1235 m²");
  assert.equal(evalLabel("{so_nha|unknown}", a), "15");
});

import {validateLabel} from "./src/label-expr.ts";
test("validate typed template syntax and prevent inherited attribute lookup",()=>{
 assert.equal(validateLabel("{name|upper} {area|round:2}",["name","area"]),null);
 for(const x of ["{unknown}","{name|script}","{name|round:7}","{name|upper:a}","{name|prefix}","{name","{{name}}"])assert.notEqual(validateLabel(x,["name"]),null);
 assert.equal(evalLabel("{constructor}",{}),"");
});

import {parseExpression,evaluateExpression} from "./src/label-expr.ts";
test("bounded AST expressions retain legacy labels and implement typed null logic",()=>{
 const attrs={name:"abc",area:12.345,empty:null,n:"2"};
 const fields=Object.keys(attrs);
 const run=s=>evaluateExpression(parseExpression(s,fields),attrs);
 assert.equal(run('upper("name") || \' · \' || round("area", 2)'),"ABC · 12.35");
 assert.equal(run('"empty" IS NULL AND "area" > 10'),true);
 assert.equal(run('NULL AND FALSE'),false);
 assert.equal(run('NULL OR TRUE'),true);
 assert.equal(run('NULL OR FALSE'),null);
 assert.equal(run('if(TRUE, 5, 1/0)'),5);
 assert.equal(run('coalesce("empty", to_real("n") * 4)'),8);
 assert.equal(run("concat('a', NULL, 'b')"),"ab");
 assert.equal(evalLabel('qgis: upper("name")',attrs),"ABC");
 assert.equal(validateLabel('qgis: "name"',[ "name" ]),null);
 assert.throws(()=>run('1/0'));
 assert.throws(()=>run('"n"+2'));
 for(const x of ['qgis: system(1)','qgis: "missing"','qgis: 1; SELECT','qgis: constructor()','qgis: 1 +','qgis: '+ '('.repeat(50)+'1'+')'.repeat(50)])assert.notEqual(validateLabel(x,fields),null);
});
