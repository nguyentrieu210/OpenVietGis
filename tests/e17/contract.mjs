import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { test } from 'node:test';
const source = resolve(process.env.E17_ROOT ?? '.', 'apps/web-next/src');
const load = name => import(pathToFileURL(resolve(source, name)).href);
const {readWorkspaceRoute} = await load('route-compat.ts');
const {findHotkey} = await load('hotkeys.ts');
const {revisionChain, renderDiff} = await load('SpatialSyncCore.ts');
const route = (org='',project='',tool=null,account=false) => ({org,project,tool,account});
test('E17-G01 canonical, legacy and dev prefix preserve explicit organization and project IDs', () => {
  for (const prefix of ['/app', '', '/devui/app']) {
    assert.deepEqual(readWorkspaceRoute(`${prefix}/organizations/org%20A/projects/p%2B1/queries`), route('org A','p+1','queries'));
    assert.deepEqual(readWorkspaceRoute(`${prefix}/organizations/o/projects/p/platform-hub`), route('o','p','ops'));
    assert.deepEqual(readWorkspaceRoute(`${prefix}/organizations/o`), route('o'));
  }
  assert.deepEqual(readWorkspaceRoute('/identity'),route('','',null,true));
  assert.deepEqual(readWorkspaceRoute('/app/account'),route('','',null,true));
});
test('E17-N01 malformed URL encoding and prototype command aliases do not select a workbench', () => {
  assert.deepEqual(readWorkspaceRoute('/app/organizations/%E0/projects/p/queries'),route());
  for(const name of ['constructor','__proto__','toString','missing']) assert.deepEqual(readWorkspaceRoute(`/app/organizations/o/projects/p/${name}`),route('o','p'));
});
test('E17-G02 keyboard command contract including macOS modifier and unknown shortcut', () => {
  const event = (key, extra={}) => ({key,ctrlKey:false,metaKey:false,shiftKey:false,altKey:false,...extra});
  assert.equal(findHotkey(event('s',{metaKey:true})).action,'SAVE');
  assert.equal(findHotkey(event('a')).action,'@PAN_TOGGLE');
  assert.equal(findHotkey(event('F6')).action,'ATTR');
  assert.equal(findHotkey(event('x')),undefined);
});
test('E17-N02 missing or reverted revision cannot be applied incrementally', () => {
  const c=(id,base,result,status='committed')=>({id,base_content_revision:base,resulting_content_revision:result,operation_count:1,status});
  assert.deepEqual(revisionChain([c('b','r1','r2'),c('a','r0','r1')],'r0','r2').map(x=>x.id),['a','b']);
  assert.equal(revisionChain([c('b','r1','r2')],'r0','r2'),null);
  assert.equal(revisionChain([c('a','r0','r1','reverted')],'r0','r1'),null);
});
test('E17-G03 render diff preserves layer identity; duplicate IDs fail closed', () => {
  const rows=[{id:'p1',geometry:{type:'Point',coordinates:[106,11]},properties:{layerId:'layer1',selected:false}}];
  const changed=[{...rows[0],properties:{...rows[0].properties,selected:true}}];
  assert.deepEqual(renderDiff(rows,changed),{remove:[],add:[],update:[{id:'p1',removeProperties:[],addOrUpdateProperties:[{key:'layerId',value:'layer1'},{key:'selected',value:true}]}]});
  assert.equal(renderDiff(rows,[...changed,...changed]),null);
});
