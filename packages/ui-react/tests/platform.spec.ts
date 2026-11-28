import {test,expect} from '@playwright/test';
import {normalizeShortcut,CommandRegistry} from '../src/commands';
import {cleanPreferences} from '../src/preferences';
import {validatePresentation,type UniversalSession} from '../src/plugin';
test('command conflicts, dynamic permissions, preferences and plugin schema guard',async()=>{
 const r=new CommandRegistry(),c={scope:'p',locale:'en' as const,capabilities:new Set<string>()};let count=0;
 const cmd={id:'map.zoom',labels:{en:'Zoom',vi:'Zoom'},group:'map',shortcut:'Ctrl+Shift+Z',capability:'map.read',run:()=>{count++}};
 r.register(cmd);expect(normalizeShortcut('shift+ctrl+z')).toBe('ctrl+shift+z');expect(()=>r.register({...cmd,id:'map.other'})).toThrow();await expect(r.execute(cmd.id,()=>c)).rejects.toThrow();c.capabilities.add('map.read');await r.execute(cmd.id,()=>c);expect(count).toBe(1);
 expect(cleanPreferences({theme:'invalid',locale:'en'}).theme).toBe('system');
 const s:UniversalSession={scopeId:'p',sessionId:'s1',nodes:[{id:'a',kind:'button',label:'x',binding:'bad',children:[]}],actions:new Set(),fields:new Set(),invoke:async()=>{},isCurrent:()=>true};expect(validatePresentation(s)).toBe(false);
});
test('100k virtual grid, cell edit, selection, filter',async({page})=>{
 await page.goto('/?platform');await expect(page.getByText('100000 rows · 0 selected')).toBeVisible();
 await expect.poll(()=>page.locator('[data-grid-row]').count()).toBeLessThan(60);
 await page.getByRole('checkbox',{name:'Select row-0',exact:true}).check();await expect(page.getByText('100000 rows · 1 selected')).toBeVisible();
 await page.getByRole('button',{name:'Feature 0',exact:true}).click();await page.getByLabel('Edit value').fill('Changed');await page.getByRole('button',{name:'Save cell'}).click();await expect(page.getByTestId('platform-status')).toContainText('"revision":"r1"');
 await page.locator('.ovg-grid-scroll').evaluate(el=>el.scrollTop=el.scrollHeight);await expect.poll(()=>page.locator('[data-grid-row]').count()).toBeLessThan(60);await expect(page.getByRole('button',{name:'Feature 99999',exact:true})).toBeVisible();
 await page.getByLabel('Filter attributes').fill('Feature 99999');await expect(page.getByText('1 rows · 1 selected')).toBeVisible();
});
test('plugin host callback, command palette and theme preferences',async({page})=>{
 await page.goto('/?platform');await page.getByLabel('Notice').fill('hello');await page.getByRole('button',{name:'Send notice'}).click();await expect(page.getByRole('region',{name:'Read-only plugin'})).toContainText('hello');
 await page.getByRole('button',{name:'Command palette'}).click();await page.getByRole('button',{name:/Zoom map/}).click();await expect(page.getByTestId('platform-status')).toHaveText('zoom');
 await page.getByLabel('Theme',{exact:true}).selectOption('dark');await expect(page.locator('html')).toHaveAttribute('data-ovg-theme','dark');await page.getByLabel('Contrast').selectOption('high');await expect(page.locator('html')).toHaveAttribute('data-ovg-contrast','high');
});