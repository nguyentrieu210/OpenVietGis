import {test,expect} from '@playwright/test';
const layoutKey=(s:{account:string;project:string;workspace:string})=>'ovg:workbench:v1:'+JSON.stringify([s.account,s.project,s.workspace]);
test('tab split float and restore retain canvas state while moving panels',async({page})=>{
 await page.goto('/?workbench');const map=page.getByTestId('map-panel');await expect(map).toBeVisible();
 const mount=await map.getAttribute('data-mount');await map.getByRole('button',{name:'Edit 0'}).click();
 await page.getByLabel('Panel',{exact:true}).selectOption('layers');
 await page.getByRole('button',{name:'Dock tab',exact:true}).click();
 await page.getByLabel('Panel',{exact:true}).selectOption('map');await page.getByRole('button',{name:'Open panel'}).click();
 await expect(map.getByRole('button',{name:'Edit 1'})).toBeVisible();await expect(map).toHaveAttribute('data-mount',mount!);
 await page.getByLabel('Panel',{exact:true}).selectOption('layers');await page.getByRole('button',{name:'Dock right'}).click();
 await page.getByRole('button',{name:'Float panel'}).click();await expect(page.getByTestId('layers-panel')).toBeVisible();
 await page.getByRole('button',{name:'Dock left'}).click();await expect(map).toHaveAttribute('data-mount',mount!);
 await expect(page.getByTestId('saved')).toHaveText('saved');
 await page.waitForTimeout(250);
 const key=layoutKey({account:'a',project:'p1',workspace:'gis'});
 const before=await page.evaluate(k=>localStorage.getItem(k),key);expect(before).toBeTruthy();
 await page.reload();await expect(page.getByTestId('layers-panel')).toBeVisible();await expect(page.getByTestId('saved')).toHaveText('saved');
});
test('close and reopen, account scope and corrupt layout fallback',async({page})=>{
 await page.goto('/?workbench');await expect(page.getByTestId('map-panel')).toBeVisible();
 await page.getByRole('button',{name:'Close Inspector'}).click();await expect(page.getByTestId('inspector-panel')).toHaveCount(0);
 await page.getByLabel('Panel',{exact:true}).selectOption('attributes');await page.getByRole('button',{name:'Open panel'}).click();
 await expect(page.getByTestId('attributes-panel')).toBeVisible();await page.waitForTimeout(250);
 await page.getByRole('button',{name:'Switch account'}).click();await expect(page.getByTestId('inspector-panel')).toBeVisible();await expect(page.getByTestId('attributes-panel')).toHaveCount(0);
 await page.getByRole('button',{name:'Switch account'}).click();await expect(page.getByTestId('attributes-panel')).toBeVisible();
 await page.evaluate(k=>localStorage.setItem(k,'{"version":999}'),layoutKey({account:'a',project:'p1',workspace:'gis'}));await page.reload();await expect(page.getByTestId('inspector-panel')).toBeVisible();
});
test('floating roundtrip, invalid component and popout rejected',async({page})=>{
 await page.goto('/?workbench');await expect(page.getByTestId('map-panel')).toBeVisible();
 const k=layoutKey({account:'a',project:'p1',workspace:'gis'});
 await page.getByLabel('Panel',{exact:true}).selectOption('layers');await page.getByRole('button',{name:'Float panel'}).click();
 await expect.poll(async()=>page.evaluate(key=>JSON.parse(localStorage.getItem(key)||'{}').layout?.floatingGroups?.length||0,k)).toBe(1);
 await page.reload();await expect(page.getByTestId('layers-panel')).toBeVisible();
 await expect.poll(async()=>page.evaluate(key=>JSON.parse(localStorage.getItem(key)||'{}').layout?.floatingGroups?.length||0,k)).toBe(1);
 await page.evaluate(key=>{const x=JSON.parse(localStorage.getItem(key)!);x.layout.panels.layers.contentComponent='untrusted';localStorage.setItem(key,JSON.stringify(x))},k);
 await page.reload();await expect(page.getByTestId('layers-panel')).toBeVisible();
 await expect.poll(async()=>page.evaluate(key=>JSON.parse(localStorage.getItem(key)||'{}').layout?.panels?.layers?.contentComponent,k)).toBe('layers');
 await page.evaluate(key=>{const x=JSON.parse(localStorage.getItem(key)!);x.layout.popoutGroups=[{url:'https://example.invalid'}];localStorage.setItem(key,JSON.stringify(x))},k);
 await page.reload();await expect(page.getByTestId('map-panel')).toBeVisible();
 await expect.poll(async()=>page.evaluate(key=>JSON.parse(localStorage.getItem(key)||'{}').layout?.popoutGroups?.length||0,k)).toBe(0);
});
test('splitter resize preserves canvas and reset recovers defaults',async({page})=>{
 await page.goto('/?workbench');const map=page.getByTestId('map-panel');await expect(map).toBeVisible();const mount=await map.getAttribute('data-mount');
 const sash=page.locator('.dv-sash').first();await expect(sash).toBeVisible();const b=await sash.boundingBox();const before=(await map.boundingBox())!.width;
 await page.mouse.move(b!.x+b!.width/2,b!.y+b!.height/2);await page.mouse.down();await page.mouse.move(b!.x+b!.width/2+80,b!.y+b!.height/2,{steps:8});await page.mouse.up();
 await expect.poll(async()=>Math.abs((await map.boundingBox())!.width-before)).toBeGreaterThan(10);
 await expect(map).toHaveAttribute('data-mount',mount!);
 await page.getByRole('button',{name:'Close Inspector'}).click();await page.getByRole('button',{name:'Reset layout'}).click();await expect(page.getByTestId('inspector-panel')).toBeVisible();
});