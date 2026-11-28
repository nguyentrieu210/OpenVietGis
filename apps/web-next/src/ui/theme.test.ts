import {expect,it} from "vitest";
import {normalizeTheme,resolveTheme} from "./theme";
it("validates stored theme and resolves system appearance",()=>{
 expect(normalizeTheme("bad")).toBe("light");expect(normalizeTheme(null)).toBe("light");expect(normalizeTheme("dark")).toBe("dark");expect(resolveTheme("system",true)).toBe("dark");expect(resolveTheme("light",true)).toBe("light");
});
import {readUISettings} from "./theme";
import {preferenceKey} from "@openvietgis/ui-react/preferences/model";
it("migrates legacy theme and isolates account settings",()=>{
 const data=new Map<string,string>([["ovg:ui-theme:v1:a","dark"],[preferenceKey("b"),JSON.stringify({theme:"system",density:"touch",contrast:"high",motion:"reduce",locale:"en"})]]);
 const storage={getItem:(k:string)=>data.get(k)??null};
 expect(readUISettings(storage,"a")).toMatchObject({theme:"dark",density:"compact"});
 expect(readUISettings(storage,"b")).toMatchObject({theme:"system",density:"touch",contrast:"high",motion:"reduce",locale:"vi"});
 expect(readUISettings(storage,"c").theme).toBe("light");
 data.set(preferenceKey("a"),"{invalid");
 expect(readUISettings(storage,"a").theme).toBe("dark");
});