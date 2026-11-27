import type { SpatialSource } from "@openvietgis/client-sdk";
export type QueryField = SpatialSource["schema"]["fields"][number];
export type QueryOperator = "eq" | "neq" | "gt" | "gte" | "lt" | "lte" | "contains" | "is_null" | "is_not_null";
export interface FilterRow {
  field: string;
  op: QueryOperator;
  value: string;
  negate: boolean;
}
export interface QueryDefinition {
  attribute_filter?: Record<string, unknown>;
  spatial_filter?: Record<string, unknown>;
}

export function availableOperators(field: QueryField | undefined): QueryOperator[] {
  if (!field) return ["eq"];
  const comparison: QueryOperator[] = ["eq", "neq"];
  if (field.type !== "boolean") comparison.push("gt", "gte", "lt", "lte");
  if (field.type === "text") comparison.push("contains");
  if (field.nullable) comparison.push("is_null", "is_not_null");
  return comparison;
}
function typedValue(field: QueryField, raw: string): string | number | boolean {
  switch (field.type) {
    case "text": return raw;
    case "boolean": {
      if (raw !== "true" && raw !== "false") throw new Error("Giá trị logic phải là true hoặc false.");
      return raw === "true";
    }
    case "integer": {
      const n = Number(raw);
      if (!/^-?\d+$/.test(raw) || !Number.isSafeInteger(n) || n < -2147483648 || n > 2147483647)
        throw new Error("Giá trị số nguyên 32 bit không hợp lệ.");
      return n;
    }
    case "decimal": {
      const value = raw.trim();
      if (!/^-?\d+(?:\.\d+)?$/.test(value)) throw new Error("Nhập số thập phân chính xác bằng dấu chấm.");
      const negative = value.startsWith("-");
      const [integerPart, fractionPart = ""] = value.replace(/^-/, "").split(".");
      const integer = integerPart!.replace(/^0+/, "") || "0";
      const fraction = fractionPart.replace(/0+$/, "");
      if (integer.replace(/^0+/, "").length + fraction.length > 38 || fraction.length > 18)
        throw new Error("Số thập phân vượt độ chính xác 38 chữ số hoặc 18 số sau dấu chấm.");
      return (negative && (integer !== "0" || fraction) ? "-" : "") + integer + (fraction ? "." + fraction : "");
    }
    case "timestamp": {
      if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(raw) || !Number.isFinite(Date.parse(raw)))
        throw new Error("Nhập thời điểm RFC3339 có múi giờ.");
      return raw;
    }
  }
}

/** Compose only persisted query definition fields, never pagination or revision. */
export function buildQueryDefinition(rows: FilterRow[], fields: QueryField[], group: "and" | "or",
  predicate: "intersects" | "within" | "contains", referenceLayer?: string): QueryDefinition {
  if (rows.length > 8) throw new Error("Tối đa 8 điều kiện thuộc tính.");
  const active = rows.length === 1 && !rows[0]?.field ? [] : rows;
  const nodes = active.map(row => {
    const field = fields.find(f => f.field_id === row.field);
    if (!field || !availableOperators(field).includes(row.op)) throw new Error("Trường hoặc toán tử không phù hợp schema.");
    const node = row.op === "is_null" || row.op === "is_not_null"
      ? { field: field.field_id, op: "is_null", value: row.op === "is_null" }
      : { field: field.field_id, op: row.op, value: typedValue(field, row.value) };
    return row.negate ? { op: "not", condition: node } : node;
  });
  return {
    ...(nodes.length ? { attribute_filter: nodes.length === 1 ? nodes[0]! : { op: group, conditions: nodes } } : {}),
    ...(referenceLayer ? { spatial_filter: { predicate, reference_layer_id: referenceLayer } } : {})
  };
}
