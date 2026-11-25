import assert from "node:assert/strict";
import test from "node:test";
import { toQML, toSLD } from "./src/style-io.ts";

const base = { family: "Polygon", color: "#3366cc", opacity: 0.5, width: 2, radius: 5 };

test("SLD: single, categorized and graduated rules with escaped values", () => {
  const single = toSLD("Thửa <đất>", { ...base, renderer: "single", classes: [] });
  assert.match(single, /<StyledLayerDescriptor version="1.0.0"/);
  assert.match(single, /Thửa &lt;đất&gt;/);
  assert.match(single, /<PolygonSymbolizer><Fill><CssParameter name="fill">#3366cc/);
  const cat = toSLD("L", { ...base, renderer: "categorized", field: "loai", classes: [{ value: "ONT", color: "#ff0000" }, { value: "A&B", color: "#00ff00" }] });
  assert.equal((cat.match(/<Rule>/g) ?? []).length, 2);
  assert.match(cat, /<ogc:Literal>A&amp;B<\/ogc:Literal>/);
  const grad = toSLD("L", { ...base, family: "Point", renderer: "graduated", field: "dt", classes: [{ min: 0, max: 10, color: "#111111" }, { min: 10, max: 20, color: "#222222" }] });
  assert.match(grad, /PropertyIsGreaterThanOrEqualTo.*<ogc:Literal>10<\/ogc:Literal>/s);
  assert.match(grad, /<PointSymbolizer>/);
});

test("QML: renderer types and category / range lists", () => {
  assert.match(toQML({ ...base, renderer: "single", classes: [] }), /<renderer-v2 type="singleSymbol">/);
  const cat = toQML({ ...base, family: "LineString", renderer: "categorized", field: "loai", classes: [{ value: "a", color: "#ff0000" }] });
  assert.match(cat, /type="categorizedSymbol" attr="loai"/);
  assert.match(cat, /<category value="a" symbol="0"/);
  assert.match(cat, /name="line_color" value="255,0,0,255"/);
  const grad = toQML({ ...base, renderer: "graduated", field: "dt", classes: [{ min: 1, max: 2, color: "#000000" }] });
  assert.match(grad, /<range lower="1" upper="2" symbol="0"/);
});
