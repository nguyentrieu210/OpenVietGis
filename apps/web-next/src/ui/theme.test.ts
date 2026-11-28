import {expect,it} from "vitest";
import {normalizeTheme,resolveTheme} from "./theme";
it("validates stored theme and resolves system appearance",()=>{
 expect(normalizeTheme("bad")).toBe("light");expect(normalizeTheme(null)).toBe("light");expect(normalizeTheme("dark")).toBe("dark");expect(resolveTheme("system",true)).toBe("dark");expect(resolveTheme("light",true)).toBe("light");
});
