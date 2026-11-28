import {test,expect} from '@playwright/test';
import {parseCoordinate,parseProperty,rampColors,validSnap,layerLeaves,visibilityIntent,validSymbol} from '../src/gis/model';
test('coordinate parsing, property bounds, ramp endpoints and invalid inputs',()=>{
 const ctx={crsId:'EPSG:4326',unit:'degree',axisOrder:'xy',geographic:true} as const;
 expect(parseCoordinate('106.5','11',ctx,'absolute')?.x).toBe(106.5);
 for(const s of ['','Infinity','NaN','0x10','1,2','181'])expect(parseCoordinate(s,'11',ctx,'absolute')).toBeNull();
 expect(parseCoordinate('90','181',{...ctx,axisOrder:'yx'},'absolute')).toBeNull();
 expect(parseCoordinate('181','100',ctx,'relative')).not.toBeNull();
 expect(parseProperty({id:'x',label:'x',kind:'number',value:1,editable:true,min:0,max:10},'11')).toEqual({ok:false});
 expect(rampColors('#000000','#ffffff',3)).toEqual(['#000000','#808080','#ffffff']);
 expect(rampColors('url(x)','#ffffff',3)).toEqual([]);
 expect(validSnap({enabled:true,kinds:[],tolerance:10,unit:'px',scope:'active'})).toBe(false);
 expect(validSymbol({color:'#000000',stroke:'#ffffff',opacity:2,size:4})).toBe(false);
 expect(()=>layerLeaves([{kind:'group',id:'x',name:'x',children:[{kind:'group',id:'x',name:'duplicate',children:[]}]}])).toThrow();
});
test('layer visibility, permission flags, filtering, selection and legend',async({page})=>{
 await page.goto('/?gis');const group=page.getByRole('checkbox',{name:'Show group Survey'});
 await expect(group).toHaveJSProperty('indeterminate',true);await group.click();
 await expect(page.getByRole('checkbox',{name:'Show Parcels'})).toBeChecked();
 await group.click();await expect(page.getByRole('checkbox',{name:'Show Roads'})).not.toBeChecked();
 await expect(page.getByRole('checkbox',{name:'Show Protected'})).toBeChecked();
 await expect(page.getByRole('button',{name:'Move Protected up'})).toBeDisabled();
 await page.getByRole('button',{name:'Roads',exact:true}).click();await expect(page.getByRole('button',{name:'Roads',exact:true})).toHaveAttribute('aria-pressed','true');
 await page.getByLabel('Filter layers').fill('Parcels');await expect(page.getByRole('button',{name:'Roads',exact:true})).toHaveCount(0);
 await expect(page.getByRole('button',{name:'Move Parcels up'})).toBeDisabled();
 await expect(page.getByText('Legend uses loaded sample')).toBeVisible();
});
test('CRS selection, CAD coordinate validation and snap capability',async({page})=>{
 await page.goto('/?gis');await page.getByLabel('X / first axis').fill('181');await page.getByLabel('Y / second axis').fill('11');
 await expect(page.getByRole('button',{name:'Submit coordinate'})).toBeDisabled();
 await page.getByLabel('X / first axis').fill('106.5');await page.getByRole('button',{name:'Submit coordinate'}).click();
 await expect(page.getByTestId('gis-event')).toContainText('EPSG:4326');
 await page.getByLabel('Search CRS').fill('VN-2000');await page.getByLabel('CRS',{exact:true}).selectOption('EPSG:3405');
 await expect(page.getByLabel('X / first axis')).toHaveValue('');
 await page.getByLabel('X / first axis').fill('500000');await page.getByLabel('Y / second axis').fill('1200000');await page.getByRole('button',{name:'Submit coordinate'}).click();
 await expect(page.getByTestId('gis-event')).toContainText('EPSG:3405');
 await expect(page.getByRole('button',{name:'intersection',exact:true})).toBeDisabled();
 await page.getByRole('button',{name:'edge',exact:true}).click();await expect(page.getByTestId('gis-event')).toContainText('"edge"');
});
test('symbol and ramp apply, typed property revision and read-only',async({page})=>{
 await page.goto('/?gis');await page.getByLabel('Fill color').fill('#ff0000');await page.getByRole('button',{name:'Apply symbol'}).click();await expect(page.getByTestId('gis-event')).toContainText('#ff0000');
 await page.getByRole('button',{name:'Apply ramp'}).click();await expect(page.getByTestId('gis-event')).toHaveText('["#000000","#808080","#ffffff"]');
 await expect(page.getByLabel('Handle')).toHaveAttribute('readonly','');
 await page.getByLabel('Length',{exact:true}).fill('-1');await expect(page.getByRole('button',{name:'Apply properties'})).toBeDisabled();
 await page.getByLabel('Length',{exact:true}).fill('20');await page.getByRole('button',{name:'Apply properties'}).click();await expect(page.getByTestId('gis-event')).toHaveText('{"entityId":"feature-1","revision":"r1","changes":{"length":20}}');
 await page.getByLabel('Feature name').fill('Unsaved');await page.getByRole('button',{name:'Change revision'}).click();await expect(page.getByLabel('Feature name')).toHaveValue('Road A');await expect(page.getByRole('button',{name:'Apply properties'})).toBeDisabled();
});
test('GIS controls at 360px and dark theme',async({page})=>{
 await page.setViewportSize({width:360,height:740});await page.goto('/?gis');await expect(page.getByRole('region',{name:'Layers'})).toBeVisible();
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.evaluate(()=>document.documentElement.dataset.ovgTheme='dark');await expect(page.getByRole('region',{name:'Layers'})).toHaveCSS('color','rgb(226, 234, 240)');
});