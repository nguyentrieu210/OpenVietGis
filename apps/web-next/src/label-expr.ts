// Spec 056: QGIS-like label expressions without code execution.
// "{field|fn:arg|fn}" placeholders; functions: upper, lower, round:n, number, default:x, prefix:x, suffix:x, if:x.
const MAX = 500;
type Attrs = Record<string, unknown>;

function apply(value: string, fn: string, arg: string | undefined): string {
  switch (fn) {
    case "upper": return value.toLocaleUpperCase("vi");
    case "lower": return value.toLocaleLowerCase("vi");
    case "round": { const n = Number(value.replace(",", ".")); const d = Math.max(0, Math.min(6, Number(arg ?? 0) || 0)); return value !== "" && Number.isFinite(n) ? n.toFixed(d) : value; }
    case "number": { const n = Number(value.replace(",", ".")); return value !== "" && Number.isFinite(n) ? n.toLocaleString("vi-VN", { maximumFractionDigits: 2 }) : value; }
    case "default": return value === "" ? arg ?? "" : value;
    case "prefix": return value === "" ? "" : (arg ?? "") + value;
    case "suffix": return value === "" ? "" : value + (arg ?? "");
    case "if": return value === "" ? "" : arg ?? value;
    default: return value; // unknown functions are ignored, never evaluated
  }
}

export function evalLabel(expr: string, attrs: Attrs): string {
  return expr.slice(0, MAX).replace(/\{([^{}]+)\}/g, (_, body: string) => {
    const [field, ...pipes] = body.split("|");
    let value = String(attrs[(field ?? "").trim()] ?? "").trim();
    for (const p of pipes) {
      const k = p.indexOf(":");
      value = apply(value, (k < 0 ? p : p.slice(0, k)).trim(), k < 0 ? undefined : p.slice(k + 1));
    }
    return value;
  });
}
