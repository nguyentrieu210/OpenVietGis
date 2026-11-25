import { describe, expect, it } from "vitest";
import { surveyTree } from "./AreaTree";

const p = (attributes: Record<string, unknown>) => ({ key: String(Math.random()), feature: { attributes } });

describe("spec 051 FR-063 parent → child tree", () => {
  it("groups points Ấp → Đường → Hẻm with counts and pending", () => {
    const tree = surveyTree([
      p({ ap: "Ấp 1", ten_duong: "Đường A", hem: "5", trang_thai: "thieu_so" }),
      p({ ap: "Ấp 1", ten_duong: "Đường A", hem: "5", trang_thai: "khop" }),
      p({ ap: "Ấp 1", ten_duong: "Đường A", hem: "", trang_thai: "sai_so", xu_ly: "giu" }),
      p({ ap: "Ấp 1 ", ten_duong: "", trang_thai: "thieu_so" }),
      p({ ap: "", ten_duong: "Đường B" })
    ]);
    expect([...tree.keys()]).toEqual(["Ấp 1"]);
    const ap = tree.get("Ấp 1")!;
    expect([ap.n, ap.pending]).toEqual([4, 2]);
    const road = ap.roads.get("Đường A")!;
    expect([road.n, road.pending]).toEqual([3, 1]);
    expect(road.alleys.get("5")).toEqual({ n: 2, pending: 1 });
    expect(ap.roads.get("(Chưa có tên đường)")?.n).toBe(1);
  });
});
