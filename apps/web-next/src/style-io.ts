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
  const stroke = (c: string, opacity = 1) => `<Stroke><CssParameter name="stroke">${c}</CssParameter><CssParameter name="stroke-width">${spec.width}</CssParameter><CssParameter name="stroke-opacity">${opacity}</CssParameter></Stroke>`;
  if (spec.family === "Point")
    return `<PointSymbolizer><Graphic><Mark><WellKnownName>circle</WellKnownName><Fill>${fill}</Fill>${stroke("#ffffff")}</Mark><Size>${spec.radius * 2}</Size></Graphic></PointSymbolizer>`;
  if (spec.family === "LineString") return `<LineSymbolizer>${stroke(color, spec.opacity)}</LineSymbolizer>`;
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
    ? `<layer class="SimpleMarker"><Option type="Map"><Option name="color" value="${rgba(color)}" type="QString"/><Option name="name" value="circle" type="QString"/><Option name="size" value="${spec.radius * 0.6}" type="QString"/><Option name="outline_color" value="255,255,255,255" type="QString"/><Option name="outline_width" value="${spec.width * 0.26}" type="QString"/></Option></layer>`
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
const qgisColor = (v: string | null | undefined) => { const p = (v ?? "").split(",").map(Number); return p.length >= 3 && p.slice(0, 3).every(n => Number.isInteger(n) && n >= 0 && n <= 255) ? "#" + p.slice(0, 3).map(n => n.toString(16).padStart(2, "0")).join("") : undefined; };
const qgisOpacity = (v: string | null | undefined) => { const a = Number((v ?? "").split(",")[3]); return Number.isFinite(a) && a >= 0 && a <= 255 ? a / 255 : undefined; };

/** Parse an SLD 1.0/1.1 or QGIS QML document into colour classes. Throws a Vietnamese message on unsupported input. */
export function parseStyle(xml: string, parser: { parseFromString(s: string, t: string): Document } = new DOMParser()): Parsed {
  if (xml.length > 2_000_000) throw Error("Tệp kiểu quá lớn (tối đa 2 MB).");
  const doc = parser.parseFromString(xml, "application/xml");
  if (byLocal(doc, "parsererror").length) throw Error("Tệp kiểu không phải XML hợp lệ.");
  const root = doc.documentElement;
  if (root.localName === "qgis") {
    const r = first(doc, "renderer-v2");
    if (!r) throw Error("QML không có phần ký hiệu (renderer-v2).");
    const symbols = byLocal(r, "symbol");
    const symbolByName = (name: string | null) => symbols.find(s => s.getAttribute("name") === name);
    const symbolProperties = (symbol: Element | undefined) => {
      if (!symbol) return { color: undefined, opacity: undefined, width: undefined, radius: undefined };
      const options = byLocal(symbol, "Option"), props = byLocal(symbol, "prop");
      const value = (names: readonly string[]) => {
        const option = options.find(o => names.includes(o.getAttribute("name") ?? ""));
        if (option) return option.getAttribute("value");
        const prop = props.find(o => names.includes(o.getAttribute("k") ?? ""));
        return prop?.getAttribute("v") ?? null;
      };
      const type = symbol.getAttribute("type") ?? "";
      const colorValue = value(["color", "line_color"]);
      const alpha = Math.max(0, Math.min(1, num(symbol.getAttribute("alpha"), 1)));
      const opacity = type === "fill" ? qgisOpacity(colorValue) ?? alpha : alpha;
      const widthRaw = value(type === "line" ? ["line_width", "width"] : ["outline_width", "line_width"]);
      const sizeRaw = value(["size"]);
      const width = widthRaw == null ? undefined : num(widthRaw, NaN) / 0.26;
      const radius = sizeRaw == null ? undefined : num(sizeRaw, NaN) / 0.6;
      return {
        color: qgisColor(colorValue),
        opacity,
        width: width !== undefined && Number.isFinite(width) && width >= 0 ? width : undefined,
        radius: radius !== undefined && Number.isFinite(radius) && radius >= 0 ? radius : undefined
      };
    };
    const symbolColor = (name: string | null) => symbolProperties(symbolByName(name)).color;
    const base = symbolProperties(symbols[0]);
    const type = r.getAttribute("type");
    if (type === "categorizedSymbol")
      return { renderer: "categorized", field: r.getAttribute("attr") ?? undefined, ...base, classes: byLocal(r, "category").flatMap(c => { const color = symbolColor(c.getAttribute("symbol")); return color ? [{ value: c.getAttribute("value") ?? "", label: c.getAttribute("label") ?? undefined, color }] : []; }) };
    if (type === "graduatedSymbol")
      return { renderer: "graduated", field: r.getAttribute("attr") ?? undefined, ...base, classes: byLocal(r, "range").flatMap(c => { const color = symbolColor(c.getAttribute("symbol")); return color ? [{ min: num(c.getAttribute("lower"), 0), max: num(c.getAttribute("upper"), 0), label: c.getAttribute("label") ?? undefined, color }] : []; }).sort((a, b) => a.min! - b.min!) };
    if (type !== "singleSymbol") throw Error("Kiểu renderer QML chưa được hỗ trợ: " + type);
    return { renderer: "single", ...base, classes: [] };
  }
  if (root.localName !== "StyledLayerDescriptor") throw Error("Chỉ nhận tệp SLD hoặc QML của QGIS.");
  const param = (el: Element | undefined, n: string) => el && byLocal(el, "CssParameter").concat(byLocal(el, "SvgParameter")).find(p => p.getAttribute("name") === n)?.textContent?.trim();
  const ruleColor = (rule: Element) => { const fill = first(rule, "Fill"), stroke = first(rule, "Stroke"); return hex(param(fill, "fill") ?? param(stroke, "stroke"), ""); };
  const ruleBasic = (rule: Element) => {
    const fill = first(rule, "Fill"), stroke = first(rule, "Stroke"), size = first(rule, "Size")?.textContent?.trim();
    const widthRaw = param(stroke, "stroke-width"), radiusRaw = size;
    const width = widthRaw == null ? undefined : num(widthRaw, NaN);
    const radius = radiusRaw == null ? undefined : num(radiusRaw, NaN) / 2;
    return {
      opacity: num(param(fill, "fill-opacity") ?? param(stroke, "stroke-opacity"), 1),
      width: width !== undefined && Number.isFinite(width) && width >= 0 ? width : undefined,
      radius: radius !== undefined && Number.isFinite(radius) && radius >= 0 ? radius : undefined
    };
  };
  const rules = byLocal(doc, "Rule");
  const literal = (el: Element | undefined) => el && first(el, "Literal")?.textContent?.trim();
  const fieldOf = (el: Element | undefined) => el && first(el, "PropertyName")?.textContent?.trim();
  const eq = rules.map(r => ({ r, f: first(r, "PropertyIsEqualTo") }));
  if (eq.length && eq.every(x => x.f&&fieldOf(x.f)===fieldOf(eq[0]!.f)))
    return { renderer: "categorized", field: fieldOf(eq[0]!.f), ...ruleBasic(rules[0]!), classes: eq.flatMap(({ r, f }) => { const color = ruleColor(r); return color ? [{ value: literal(f) ?? "", color }] : []; }) };
  const ranged = rules.map(r => ({ r, lo: first(r, "PropertyIsGreaterThanOrEqualTo") ?? first(r, "PropertyIsGreaterThan"), hi: first(r, "PropertyIsLessThanOrEqualTo") ?? first(r, "PropertyIsLessThan") }));
  if (ranged.length && ranged.every(x => x.lo && x.hi&&fieldOf(x.lo)===fieldOf(ranged[0]!.lo)&&fieldOf(x.hi)===fieldOf(x.lo)))
    return { renderer: "graduated", field: fieldOf(ranged[0]!.lo), ...ruleBasic(rules[0]!), classes: ranged.flatMap(({ r, lo, hi }) => { const color = ruleColor(r); return color ? [{ min: num(literal(lo), 0), max: num(literal(hi), 0), color }] : []; }).sort((a, b) => a.min! - b.min!) };
  if(rules.length!==1||rules.some(r=>!!first(r,"Filter")||!!first(r,"ElseFilter")))throw Error("SLD rule-based hoặc nhiều trường chưa được hỗ trợ.");
  const rule = rules[0];
  if (!rule) throw Error("SLD không có quy tắc (Rule) nào.");
  return { renderer: "single", color: ruleColor(rule) || undefined, ...ruleBasic(rule), classes: [] };
}
