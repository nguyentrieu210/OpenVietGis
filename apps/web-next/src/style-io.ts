// Spec 056 đợt 3: export / import layer styles as OGC SLD 1.0 or QGIS QML (single, categorized, graduated).
// Only the colour classes and basic stroke/size are exchanged; QGIS-only effects are ignored on import.
export type StyleClass = { color: string; value?: string; min?: number; max?: number; label?: string };
export type StyleSpec = {
  family: "Point" | "LineString" | "Polygon" | string;
  renderer: "single" | "categorized" | "graduated";
  field?: string;
  color: string;
  opacity: number;
  width: number;
  radius: number;
  classes: StyleClass[];
};

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const hex = (x: string | null | undefined, fallback: string) => (x && /^#[0-9a-f]{6}$/i.test(x.trim()) ? x.trim().toLowerCase() : fallback);
const num = (x: string | null | undefined, fallback: number) => { const n = x==null||!x.trim()?NaN:Number(x); return Number.isFinite(n) ? n : fallback; };

function sldSymbolizer(spec: StyleSpec, color: string) {
  const fill = `<CssParameter name="fill">${color}</CssParameter><CssParameter name="fill-opacity">${spec.opacity}</CssParameter>`;
  const stroke = (c: string) => `<Stroke><CssParameter name="stroke">${c}</CssParameter><CssParameter name="stroke-width">${spec.width}</CssParameter></Stroke>`;
  if (spec.family === "Point")
    return `<PointSymbolizer><Graphic><Mark><WellKnownName>circle</WellKnownName><Fill>${fill}</Fill>${stroke("#ffffff")}</Mark><Size>${spec.radius * 2}</Size></Graphic></PointSymbolizer>`;
  if (spec.family === "LineString") return `<LineSymbolizer>${stroke(color)}</LineSymbolizer>`;
  return `<PolygonSymbolizer><Fill>${fill}</Fill>${stroke(color)}</PolygonSymbolizer>`;
}

export function toSLD(name: string, spec: StyleSpec): string {
  const prop = (f: string) => `<ogc:PropertyName>${esc(f)}</ogc:PropertyName>`;
  const rules = spec.renderer === "single" || !spec.field
    ? [`<Rule><Name>${esc(name)}</Name>${sldSymbolizer(spec, spec.color)}</Rule>`]
    : spec.classes.map((c,index) => {
        const filter = spec.renderer === "categorized"
          ? `<ogc:PropertyIsEqualTo>${prop(spec.field!)}<ogc:Literal>${esc(c.value ?? "")}</ogc:Literal></ogc:PropertyIsEqualTo>`
          : `<ogc:And><ogc:${index===0?"PropertyIsGreaterThanOrEqualTo":"PropertyIsGreaterThan"}>${prop(spec.field!)}<ogc:Literal>${c.min}</ogc:Literal></ogc:${index===0?"PropertyIsGreaterThanOrEqualTo":"PropertyIsGreaterThan"}><ogc:PropertyIsLessThanOrEqualTo>${prop(spec.field!)}<ogc:Literal>${c.max}</ogc:Literal></ogc:PropertyIsLessThanOrEqualTo></ogc:And>`;
        return `<Rule><Name>${esc(c.label ?? c.value ?? `${c.min} - ${c.max}`)}</Name><ogc:Filter>${filter}</ogc:Filter>${sldSymbolizer(spec, c.color)}</Rule>`;
      });
  return `<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0" xmlns="http://www.opengis.net/sld" xmlns:ogc="http://www.opengis.net/ogc" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.opengis.net/sld http://schemas.opengis.net/sld/1.0.0/StyledLayerDescriptor.xsd"><NamedLayer><Name>${esc(name)}</Name><UserStyle><Title>${esc(name)}</Title><FeatureTypeStyle>${rules.join("")}</FeatureTypeStyle></UserStyle></NamedLayer></StyledLayerDescriptor>
`;
}

const rgba = (c: string, a = 1) => `${parseInt(c.slice(1, 3), 16)},${parseInt(c.slice(3, 5), 16)},${parseInt(c.slice(5, 7), 16)},${Math.round(a * 255)}`;
function qmlSymbol(spec: StyleSpec, color: string, i: number) {
  const type = spec.family === "Point" ? "marker" : spec.family === "LineString" ? "line" : "fill";
  const layer = spec.family === "Point"
    ? `<layer class="SimpleMarker"><Option type="Map"><Option name="color" value="${rgba(color)}" type="QString"/><Option name="name" value="circle" type="QString"/><Option name="size" value="${spec.radius * 0.6}" type="QString"/><Option name="outline_color" value="255,255,255,255" type="QString"/></Option></layer>`
    : spec.family === "LineString"
      ? `<layer class="SimpleLine"><Option type="Map"><Option name="line_color" value="${rgba(color)}" type="QString"/><Option name="line_width" value="${spec.width * 0.26}" type="QString"/></Option></layer>`
      : `<layer class="SimpleFill"><Option type="Map"><Option name="color" value="${rgba(color, spec.opacity)}" type="QString"/><Option name="outline_color" value="${rgba(color)}" type="QString"/><Option name="outline_width" value="${spec.width * 0.26}" type="QString"/></Option></layer>`;
  return `<symbol name="${i}" type="${type}" alpha="${spec.family === "Polygon" ? 1 : spec.opacity}">${layer}</symbol>`;
}

export function toQML(spec: StyleSpec): string {
  const symbols = (spec.renderer === "single" || !spec.field ? [spec.color] : spec.classes.map(c => c.color)).map((c, i) => qmlSymbol(spec, c, i)).join("");
  const body = spec.renderer === "single" || !spec.field
    ? `<renderer-v2 type="singleSymbol"><symbols>${symbols}</symbols></renderer-v2>`
    : spec.renderer === "categorized"
      ? `<renderer-v2 type="categorizedSymbol" attr="${esc(spec.field)}"><categories>${spec.classes.map((c, i) => `<category value="${esc(c.value ?? "")}" symbol="${i}" label="${esc(c.label ?? c.value ?? "")}" render="true"/>`).join("")}</categories><symbols>${symbols}</symbols></renderer-v2>`
      : `<renderer-v2 type="graduatedSymbol" attr="${esc(spec.field)}"><ranges>${spec.classes.map((c, i) => `<range lower="${c.min}" upper="${c.max}" symbol="${i}" label="${esc(c.label ?? `${c.min} - ${c.max}`)}" render="true"/>`).join("")}</ranges><symbols>${symbols}</symbols></renderer-v2>`;
  return `<!DOCTYPE qgis PUBLIC 'http://mrcc.com/qgis.dtd' 'SYSTEM'>
<qgis version="3.34" styleCategories="Symbology">${body}</qgis>
`;
}

type Parsed = { renderer: StyleSpec["renderer"]; field?: string; color?: string; opacity?: number; width?: number; radius?: number; classes: StyleClass[] };
const byLocal = (root: Element | Document, name: string) => Array.from(root.getElementsByTagName("*")).filter(e => e.localName === name);
const first = (root: Element | Document, name: string) => byLocal(root, name)[0];
const qgisColor = (v: string | null | undefined) => { const p = (v ?? "").split(",").map(Number); return p.length >= 3 && p.slice(0, 3).every(n => n >= 0 && n <= 255) ? "#" + p.slice(0, 3).map(n => n.toString(16).padStart(2, "0")).join("") : undefined; };

/** Parse an SLD 1.0/1.1 or QGIS QML document into colour classes. Throws a Vietnamese message on unsupported input. */
export function parseStyle(xml: string, parser: { parseFromString(s: string, t: string): Document } = new DOMParser()): Parsed {
  if (xml.length > 2_000_000) throw Error("Tệp kiểu quá lớn (tối đa 2 MB).");
  const doc = parser.parseFromString(xml, "application/xml");
  if (byLocal(doc, "parsererror").length) throw Error("Tệp kiểu không phải XML hợp lệ.");
  const root = doc.documentElement;
  if (root.localName === "qgis") {
    const r = first(doc, "renderer-v2");
    if (!r) throw Error("QML không có phần ký hiệu (renderer-v2).");
    const symbolColor = (name: string | null) => {
      const sym = byLocal(r, "symbol").find(s => s.getAttribute("name") === name);
      const opt = sym && byLocal(sym, "Option").find(o => ["color", "line_color"].includes(o.getAttribute("name") ?? ""));
      const prop = sym && byLocal(sym, "prop").find(o => ["color", "line_color"].includes(o.getAttribute("k") ?? ""));
      return qgisColor(opt?.getAttribute("value") ?? prop?.getAttribute("v"));
    };
    const type = r.getAttribute("type");
    if (type === "categorizedSymbol")
      return { renderer: "categorized", field: r.getAttribute("attr") ?? undefined, classes: byLocal(r, "category").flatMap(c => { const color = symbolColor(c.getAttribute("symbol")); return color ? [{ value: c.getAttribute("value") ?? "", label: c.getAttribute("label") ?? undefined, color }] : []; }) };
    if (type === "graduatedSymbol")
      return { renderer: "graduated", field: r.getAttribute("attr") ?? undefined, classes: byLocal(r, "range").flatMap(c => { const color = symbolColor(c.getAttribute("symbol")); return color ? [{ min: num(c.getAttribute("lower"), 0), max: num(c.getAttribute("upper"), 0), color }] : []; }).sort((a, b) => a.min! - b.min!) };
    if(type!=="singleSymbol")throw Error("Kiểu renderer QML chưa được hỗ trợ: "+type);
    const sym = first(r, "symbol");
    return { renderer: "single", color: symbolColor(sym?.getAttribute("name") ?? "0"), opacity: num(sym?.getAttribute("alpha"), 1), classes: [] };
  }
  if (root.localName !== "StyledLayerDescriptor") throw Error("Chỉ nhận tệp SLD hoặc QML của QGIS.");
  const param = (el: Element | undefined, n: string) => el && byLocal(el, "CssParameter").concat(byLocal(el, "SvgParameter")).find(p => p.getAttribute("name") === n)?.textContent?.trim();
  const ruleColor = (rule: Element) => { const fill = first(rule, "Fill"), stroke = first(rule, "Stroke"); return hex(param(fill, "fill") ?? param(stroke, "stroke"), ""); };
  const rules = byLocal(doc, "Rule");
  const literal = (el: Element | undefined) => el && first(el, "Literal")?.textContent?.trim();
  const fieldOf = (el: Element | undefined) => el && first(el, "PropertyName")?.textContent?.trim();
  const eq = rules.map(r => ({ r, f: first(r, "PropertyIsEqualTo") }));
  if (eq.length && eq.every(x => x.f&&fieldOf(x.f)===fieldOf(eq[0]!.f)))
    return { renderer: "categorized", field: fieldOf(eq[0]!.f), classes: eq.flatMap(({ r, f }) => { const color = ruleColor(r); return color ? [{ value: literal(f) ?? "", color }] : []; }) };
  const ranged = rules.map(r => ({ r, lo: first(r, "PropertyIsGreaterThanOrEqualTo") ?? first(r, "PropertyIsGreaterThan"), hi: first(r, "PropertyIsLessThanOrEqualTo") ?? first(r, "PropertyIsLessThan") }));
  if (ranged.length && ranged.every(x => x.lo && x.hi&&fieldOf(x.lo)===fieldOf(ranged[0]!.lo)&&fieldOf(x.hi)===fieldOf(x.lo)))
    return { renderer: "graduated", field: fieldOf(ranged[0]!.lo), classes: ranged.flatMap(({ r, lo, hi }) => { const color = ruleColor(r); return color ? [{ min: num(literal(lo), 0), max: num(literal(hi), 0), color }] : []; }).sort((a, b) => a.min! - b.min!) };
  if(rules.length!==1||rules.some(r=>!!first(r,"Filter")||!!first(r,"ElseFilter")))throw Error("SLD rule-based hoặc nhiều trường chưa được hỗ trợ.");
  const rule = rules[0];
  if (!rule) throw Error("SLD không có quy tắc (Rule) nào.");
  const fill = first(rule, "Fill"), stroke = first(rule, "Stroke"), size = first(rule, "Size")?.textContent;
  return { renderer: "single", color: ruleColor(rule) || undefined, opacity: num(param(fill, "fill-opacity"), 1), width: num(param(stroke, "stroke-width"), NaN), radius: size ? num(size, 10) / 2 : undefined, classes: [] };
}
