import { describe, expect, it } from "vitest";
import { searchSurvey } from "./SurveyNavigator";

const row = (key: string, attributes: Record<string, unknown>) => ({ key, feature: { attributes } });
const rows = [
  row("1901", { so_nha: "1901", ten_duong: "Đường Văn Tiến Dũng", ap: "Ấp 2" }),
  row("190-28", { so_nha: "190", ten_duong: "Đường Liên Ấp 3B", ap: "Ấp 28" }),
  row("190-a17", { so_nha: "190", ten_duong: "Đường A17", ap: "Ấp 26" }),
  row("1190", { so_nha: "1190", ten_duong: "Đường A17", ap: "Ấp 26" }),
  row("155-22", { so_nha: "155/22", ten_duong: "Đường Phạm Hùng", ap: "Ấp 27", ma_phieu: "27/PH/155/22" }),
  row("ap2", { so_nha: "44/23", ten_duong: "Đường A35-Ap27", ap: "Ấp 2" })
];
const keys = (query: string) => searchSurvey(rows, query).map((r) => r.key);

describe("survey house-number search", () => {
  it("puts the exact house number first and never matches inside another number", () => {
    expect(keys("190").slice(0, 2).sort()).toEqual(["190-28", "190-a17"]);
    expect(keys("190")).not.toContain("1190");
  });
  it("ignores accents, case and separators", () => {
    expect(keys("duong a17")).toEqual(["190-a17", "1190"]);
    expect(keys("190/ ĐườngA17- Ấp 26".replace("ĐườngA17", "Đường A17"))[0]).toBe("190-a17");
    expect(keys("155/22")[0]).toBe("155-22");
    expect(keys("155 22")[0]).toBe("155-22");
  });
  it("ranks a whole word above a prefix", () => {
    expect(keys("ấp 2")[0]).toMatch(/^(1901|ap2)$/);
    expect(keys("ap 2").indexOf("190-28")).toBeGreaterThan(keys("ap 2").indexOf("1901"));
  });
  it("returns nothing for an empty query", () => {
    expect(keys("  / ")).toEqual([]);
  });
});
