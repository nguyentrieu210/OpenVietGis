import { describe, expect, it } from "vitest";
import { needsWork, signoffFor, surveyProgress } from "./SurveyNavigator";

const point = (attributes: Record<string, unknown>) => ({ key: String(Math.random()), feature: { attributes } });

describe("spec 051 reconciliation progress (Bình Hưng rule)", () => {
  it("counts a point as pending only when it is not khop and nobody handled it", () => {
    expect(needsWork(point({ trang_thai: "thieu_so", xu_ly: "chua_xu_ly" }))).toBe(true);
    expect(needsWork(point({ trang_thai: "sai_so" }))).toBe(true);
    expect(needsWork(point({ trang_thai: "khop", xu_ly: "chua_xu_ly" }))).toBe(false);
    for (const xu_ly of ["giu", "da_sua", "bo"]) expect(needsWork(point({ trang_thai: "lech_vi_tri", xu_ly }))).toBe(false);
  });

  it("reports done / pending / floor percent", () => {
    const rows = [
      point({ trang_thai: "khop" }),
      point({ trang_thai: "thieu_so", xu_ly: "giu" }),
      point({ trang_thai: "thieu_so", xu_ly: "chua_xu_ly" })
    ];
    expect(surveyProgress(rows)).toEqual({ total: 3, pending: 1, done: 2, percent: 66 });
    expect(surveyProgress([])).toEqual({ total: 0, pending: 0, done: 0, percent: 0 });
  });

  it("a hamlet sign-off covers every road; a road sign-off covers only that road", () => {
    const hamlet = { id: "1", ap: "Ấp 1", ten_duong: null, signed_at: "2026-11-25T00:00:00Z", point_count: 5 };
    const road = { id: "2", ap: "Ấp 2", ten_duong: "Đường A", signed_at: "2026-11-25T00:00:00Z", point_count: 2 };
    expect(signoffFor([hamlet, road], "Ấp 1", "Đường B")).toBe(hamlet);
    expect(signoffFor([hamlet, road], "Ấp 2", "Đường A")).toBe(road);
    expect(signoffFor([hamlet, road], "Ấp 2", "Đường B")).toBeUndefined();
    expect(signoffFor([hamlet, road], "Ấp 2", "")).toBeUndefined();
  });
});
