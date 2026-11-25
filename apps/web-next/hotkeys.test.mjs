import assert from "node:assert/strict";
import test from "node:test";
import { comboOf, findHotkey } from "./src/hotkeys.ts";

const ev = (key, mods = {}) => ({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });
test("combos map to QGIS-style shortcuts", () => {
  assert.equal(findHotkey(ev("a")).action, "@PAN_TOGGLE");
  assert.equal(findHotkey(ev("s", { ctrlKey: true })).action, "SAVE");
  assert.equal(findHotkey(ev("F", { ctrlKey: true, shiftKey: true })).action, "ZE");
  assert.equal(findHotkey(ev("+", { ctrlKey: true, shiftKey: true })).action, "ZI");
  assert.equal(findHotkey(ev("F6")).action, "ATTR");
  assert.equal(findHotkey(ev("?", { shiftKey: true })).action, "@HELP");
  assert.equal(findHotkey(ev("x")), undefined);
  assert.equal(comboOf(ev("a", { ctrlKey: true, altKey: true })), "Ctrl+Alt+A");
});
