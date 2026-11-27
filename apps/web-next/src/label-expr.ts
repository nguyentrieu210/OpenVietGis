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
    const key=(field??"").trim();
    let value = String(Object.hasOwn(attrs,key)?attrs[key]??"":"").trim();
    for (const p of pipes) {
      const k = p.indexOf(":");
      value = apply(value, (k < 0 ? p : p.slice(0, k)).trim(), k < 0 ? undefined : p.slice(k + 1));
    }
    return value;
  });
}

/** Validate the limited template language before saving; existing templates remain readable. */
export function validateLabel(expr:string,fields:readonly string[]):string|null{
 if(!expr.trim())return "Nhập biểu thức nhãn.";
 if(expr.length>MAX)return "Biểu thức tối đa 500 ký tự.";
 const known=new Set(fields);
 const pieces=expr.matchAll(/\{([^{}]*)\}/g);
 for(const match of pieces){
  const [raw,...pipes]=match[1]!.split("|"),field=raw!.trim();
  if(!known.has(field))return "Trường không tồn tại: "+field;
  for(const pipe of pipes){
   const i=pipe.indexOf(":"),fn=(i<0?pipe:pipe.slice(0,i)).trim(),arg=i<0?undefined:pipe.slice(i+1);
   if(!["upper","lower","round","number","default","prefix","suffix","if"].includes(fn))return "Hàm không hỗ trợ: "+fn;
   if(fn==="round"&&arg!==undefined&&!/^[0-6]$/.test(arg.trim()))return "round cần số nguyên 0–6.";
   if(["upper","lower","number"].includes(fn)&&arg!==undefined)return fn+" không nhận tham số.";
   if(["default","prefix","suffix","if"].includes(fn)&&arg===undefined)return fn+" cần tham số.";
  }
 }
 if(/[{}]/.test(expr.replace(/\{[^{}]*\}/g,"")))return "Ngoặc nhọn chưa cân bằng.";
 return null;
}
