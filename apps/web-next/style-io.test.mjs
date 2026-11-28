import assert from "node:assert/strict";
import test from "node:test";
import { parseStyle, toQML, toSLD } from "./src/style-io.ts";

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

test("SLD uses non-overlapping lower boundaries after first class",()=>{
 const s=toSLD("L",{...base,renderer:"graduated",field:"x",classes:[{min:0,max:10,color:"#000000"},{min:10,max:20,color:"#ffffff"}]});
 assert.match(s,/<ogc:PropertyIsGreaterThan><ogc:PropertyName>x<\/ogc:PropertyName><ogc:Literal>10<\/ogc:Literal>/);
});


function domParser() {
  const decode = (value) => value.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&apos;/g, "'").replace(/&amp;/g, "&");
  const descendants = (node) => node.children.flatMap(child => [child, ...descendants(child)]);
  return {
    parseFromString(xml) {
      const doc = {
        documentElement: null,
        getElementsByTagName() {
          return this.documentElement ? [this.documentElement, ...descendants(this.documentElement)] : [];
        }
      };
      const stack = [];
      const tokens = xml.match(/<!--[\s\S]*?-->|<![^>]*>|<\/?[^>]+>|[^<]+/g) ?? [];
      for (const token of tokens) {
        if (token.startsWith("<!--") || token.startsWith("<!") || token.startsWith("<?")) continue;
        if (token.startsWith("</")) { stack.pop(); continue; }
        if (token.startsWith("<")) {
          const tag = token.match(/^<([^\s/>]+)/)?.[1];
          if (!tag) continue;
          const attrs = Object.fromEntries([...token.matchAll(/([A-Za-z_:][\w:.-]*)\s*=\s*"([^"]*)"/g)].map(m => [m[1], decode(m[2])]));
          const node = {
            localName: tag.split(":").at(-1),
            attrs,
            children: [],
            content: [],
            getAttribute(name) { return Object.hasOwn(this.attrs, name) ? this.attrs[name] : null; },
            getElementsByTagName() { return descendants(this); },
            get textContent() { return this.content.map(part => typeof part === "string" ? part : part.textContent).join(""); }
          };
          if (stack.length) {
            stack.at(-1).children.push(node);
            stack.at(-1).content.push(node);
          } else doc.documentElement = node;
          if (!/\/\s*>$/.test(token)) stack.push(node);
          continue;
        }
        if (stack.length) stack.at(-1).content.push(decode(token));
      }
      return doc;
    }
  };
}

test("QML round-trip imports point colour, opacity, width and radius", () => {
  const spec = { ...base, family: "Point", renderer: "single", color: "#3366cc", opacity: 0.4, width: 2, radius: 5, classes: [] };
  const xml = toQML(spec);
  assert.match(xml, /name="outline_width" value="0\.52"/);
  const parsed = parseStyle(xml, domParser());
  assert.equal(parsed.renderer, "single");
  assert.equal(parsed.color, "#3366cc");
  assert.ok(Math.abs(parsed.opacity - spec.opacity) < 1 / 255);
  assert.equal(parsed.width, spec.width);
  assert.equal(parsed.radius, spec.radius);
});

test("SLD round-trip imports line opacity and graduated class rules", () => {
  const spec = {
    ...base, family: "LineString", renderer: "graduated", field: "value", opacity: 0.4, width: 2,
    classes: [{ min: 0, max: 10, color: "#111111" }, { min: 10, max: 20, color: "#222222" }]
  };
  const xml = toSLD("Lines", spec);
  assert.match(xml, /name="stroke-opacity">0\.4/);
  const parsed = parseStyle(xml, domParser());
  assert.equal(parsed.renderer, "graduated");
  assert.equal(parsed.field, "value");
  assert.equal(parsed.classes.length, 2);
  assert.equal(parsed.classes[0].min, 0);
  assert.equal(parsed.width, spec.width);
  assert.equal(parsed.opacity, spec.opacity);
});

test("QML round-trip imports categorized values and class colours", () => {
  const spec = {
    ...base, family: "Polygon", renderer: "categorized", field: "kind",
    classes: [{ value: "residential", color: "#ff0000" }, { value: "green", color: "#00ff00" }]
  };
  const parsed = parseStyle(toQML(spec), domParser());
  assert.equal(parsed.renderer, "categorized");
  assert.equal(parsed.field, spec.field);
  assert.deepEqual(parsed.classes.map(({ value, color }) => ({ value, color })), spec.classes);
});

test("SLD round-trip imports categorized values and class colours", () => {
  const spec = {
    ...base, family: "Polygon", renderer: "categorized", field: "kind",
    classes: [{ value: "residential", color: "#ff0000" }, { value: "green", color: "#00ff00" }]
  };
  const parsed = parseStyle(toSLD("Areas", spec), domParser());
  assert.equal(parsed.renderer, "categorized");
  assert.equal(parsed.field, spec.field);
  assert.deepEqual(parsed.classes.map(({ value, color }) => ({ value, color })), spec.classes);
});

test("two-layer line casing round-trips through SLD and QML",()=>{
 const spec={...base,family:"LineString",renderer:"single",classes:[],casing:{color:"#101010",extraWidth:4}};
 for(const xml of [toSLD("Road",spec),toQML(spec)]){
  const p=parseStyle(xml,domParser());
  assert.equal(p.family,"LineString");assert.equal(p.color,spec.color);assert.equal(p.width,spec.width);
  assert.equal(p.casing.color,spec.casing.color);assert.ok(Math.abs(p.casing.extraWidth-4)<1e-9);
 }
 assert.throws(()=>parseStyle('<!DOCTYPE qgis [<!ENTITY x SYSTEM "file:///etc/passwd">]><qgis/>',domParser()));
});
