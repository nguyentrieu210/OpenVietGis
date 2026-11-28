import test from 'node:test';
import assert from 'node:assert/strict';
import {summarize,parseTree,differences} from '../audit_frontend_077.mjs';
test('parses NUL tree and preserves paths',()=>{
 const entries=parseTree('100644 blob abc\ta file.css\0'+'100644 blob def\tui.tsx\0'+'160000 commit xyz\tvendor\0');
 assert.deepEqual(summarize(entries),{blobs:2,tsx:1,css:1,cssPaths:['a file.css']});
});
test('detects duplicates, new CSS, missing CSS and count drift',()=>{
 const observed={blobs:2,tsx:1,css:1,cssPaths:['new.css']};
 const errors=differences({blobs:3,tsx:1,css:2,cssGroups:{a:['old.css'],b:['old.css']}},observed);
 for(const expected of ['blobs:','css:','Duplicate CSS','Missing CSS: old.css','Unclassified CSS: new.css'])assert.ok(errors.some(e=>e.startsWith(expected)),expected);
});
test('matching baseline passes, malformed tree fails',()=>{
 assert.deepEqual(differences({blobs:1,tsx:0,css:1,cssGroups:{foundation:['x.css']}},{blobs:1,tsx:0,css:1,cssPaths:['x.css']}),[]);
 assert.throws(()=>parseTree('invalid'),/Malformed/);
});
