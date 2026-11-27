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
