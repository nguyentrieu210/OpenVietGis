import {test,expect} from '@playwright/test';
test('dialog focus, labelled fields, Escape, restore and fixed footer',async({page})=>{
 await page.goto('/');const trigger=page.getByRole('button',{name:'Open dialog'});await trigger.click();
 const dialog=page.getByRole('dialog',{name:'Layer properties'});await expect(dialog).toBeVisible();
 await expect(dialog.getByLabel('Layer name')).toHaveAttribute('aria-invalid','true');
 for(let i=0;i<9;i++){await page.keyboard.press('Tab');expect(await dialog.evaluate(el=>el.contains(document.activeElement))).toBe(true)}
 await page.keyboard.press('Escape');await expect(dialog).toBeHidden();await expect(trigger).toBeFocused();
 await trigger.click();await dialog.getByRole('button',{name:'Save'}).click();await expect(dialog).toBeHidden();
});
test('menu selection, disabled command, popover, tooltip, tabs and toggles',async({page})=>{
 await page.goto('/');await page.getByRole('button',{name:'Actions menu'}).click();
 await expect(page.getByRole('menuitem',{name:'Forbidden command'})).toHaveAttribute('aria-disabled','true');
 await page.getByRole('menuitem',{name:'Run command'}).click();await expect(page.getByLabel('Last action')).toHaveText('run');
 await page.getByRole('button',{name:'Options'}).click();await expect(page.getByText('Options panel')).toBeVisible();await page.keyboard.press('Escape');await expect(page.getByText('Options panel')).toBeHidden();
 await page.getByRole('button',{name:'Help'}).hover();await expect(page.getByRole('tooltip')).toHaveText('Map information');
 await page.getByRole('checkbox',{name:'Select layer'}).click();await expect(page.getByRole('checkbox',{name:'Select layer'})).toBeChecked();
 await page.getByRole('switch',{name:'Show layer'}).click();await expect(page.getByRole('switch',{name:'Show layer'})).toBeChecked();
 await page.getByRole('tab',{name:'Map'}).focus();await page.keyboard.press('ArrowRight');await page.keyboard.press('Enter');await expect(page.getByRole('tabpanel',{name:'Attributes'})).toBeVisible();
});
for(const width of [360,390,768,1024,1440,1920])test('dialog overflow '+width,async({page})=>{
 await page.setViewportSize({width,height:640});await page.goto('/');await page.getByRole('button',{name:'Open dialog'}).click();const d=page.getByRole('dialog');await expect(d).toBeVisible();const box=await d.boundingBox();expect(box!.x).toBeGreaterThanOrEqual(0);expect(box!.x+box!.width).toBeLessThanOrEqual(width+1);await expect(d.getByRole('button',{name:'Save'})).toBeInViewport();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 const table=d.locator('.ovg-v2-table-scroll');expect(await table.evaluate(el=>el.scrollWidth>=el.clientWidth)).toBe(true);
});
