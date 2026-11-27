import assert from "node:assert/strict";
import test from "node:test";
import {searchCommands,resolveCommand,normalizeCommand} from "./src/command-search.ts";
const commands=[{alias:"SAVE",label:"Lưu bản đồ",group:"Dự án"},{alias:"ERASE",label:"Xóa đối tượng",group:"Biên tập",disabled:true},{alias:"DIST",label:"Đo khoảng cách",group:"Hiển thị",hint:"distance"}];
const aliases={DI:"DIST"};
test("Vietnamese accents including capital Đ, English aliases and multiword discovery",()=>{
 assert.equal(normalizeCommand("Đo"),"DO");
 assert.deepEqual(searchCommands(commands,"do khoang cach",aliases).map(c=>c.alias),["DIST"]);
 assert.equal(searchCommands(commands,"distance",aliases)[0].alias,"DIST");
 assert.equal(searchCommands(commands,"di",aliases)[0].alias,"DIST");
});
test("fuzzy discovery cannot silently resolve a destructive command",()=>{
 assert.equal(searchCommands(commands,"xoa doi",aliases)[0].alias,"ERASE");
 assert.equal(resolveCommand(commands,"xoa",aliases),undefined);
 assert.equal(resolveCommand(commands,"ERASE",aliases).disabled,true);
 assert.equal(resolveCommand(commands,"di",aliases).alias,"DIST");
 assert.equal(resolveCommand(commands,"constructor",aliases),undefined);
});
