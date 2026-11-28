import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {contrast,validate,generate,run} from '../generate.mjs';
const data=JSON.parse(readFileSync(new URL('../tokens.json',import.meta.url),'utf8'));
test('valid token set and contrast gates',()=>{assert.equal(validate(data),data);assert.equal(contrast('#000000','#ffffff'),21)});
test('reject missing, invalid, low contrast, too-small touch, nonzero reduced motion',()=>{
 for(const edit of [
  t=>delete t.color.dark.ink,
  t=>t.color.light.primary.$value.hex='#ffffff',
  t=>t.color.light.ink=structuredClone(t.color.light.surface),
  t=>t.density.touch['touch-target'].$value.value=32,
  t=>t.motion.reduced.$value.value=10,
  t=>t.typography.body.$value.unit='em',
 ]){const t=structuredClone(data);edit(t);assert.throws(()=>validate(t))}
});
test('deterministic generation, exact checked artifacts',()=>{assert.deepEqual(generate(data),generate(structuredClone(data)));run(true)});
test('SPEC072 alias defaults, scope and Tailwind no reset',()=>{
 const o=generate(data),css=o['generated/tokens.css'],tw=o['generated/tailwind.css'];
 for(const pair of ['surface: #ffffff','panel: #f2f5f6','ink: #263c49','muted: #526b79','line: #ccd8df','primary: #1565a9','gap: 8px','inset: 16px','control-height: 32px','radius: var(--ovg-ui-radius-ui)','radius-ui: 4px'])assert.ok(css.includes('--ovg-ui-'+pair),pair);
 for(const selector of ['data-ovg-theme=dark','data-ovg-theme=system','data-ovg-density=touch','prefers-reduced-motion'])assert.ok(css.includes(selector));
 assert.ok(tw.includes('@theme inline'));assert.ok(tw.includes('--color-ui-primary: var(--ovg-ui-primary)'));assert.ok(!/preflight/i.test(tw.replace(/\/\*[\s\S]*?\*\//g,'')));
 assert.ok(!css.includes('color-scheme: light;'));
});
test('Flutter colors, densities and real ThemeData generation',()=>{const d=generate(data)['flutter/lib/openvietgis_design_tokens.dart'];assert.ok(d.includes('Color(0xff1565a9)'));assert.ok(d.includes('Color(0xff62a9e5)'));assert.ok(d.includes('static ThemeData theme'));assert.ok(d.includes("'touch-target': 48"));});
