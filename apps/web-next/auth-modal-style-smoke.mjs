/** Optional Keycloak browser smoke; tests styling only, never submits credentials. */
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {chromium} from '@playwright/test';

const origin = (process.env.OVG_AUTH_TEST_ORIGIN || 'http://127.0.0.1:18080').replace(/\/$/, '');
const css = await readFile(new URL('../../infra/dev/keycloak/themes/openvietgis/login/resources/css/openvietgis-modal-v3.css', import.meta.url), 'utf8');
const authPath = origin.includes('openvietgis.cloud')
  ? '/api/v1/auth/login/keycloak?prompt=login'
  : '/realms/openvietgis/protocol/openid-connect/auth?client_id=openvietgis-web&redirect_uri=http%3A%2F%2F127.0.0.1%3A8000%2Fapi%2Fv1%2Fauth%2Fcallback%2Fkeycloak&response_type=code&scope=openid&state=style-only&code_challenge=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&code_challenge_method=S256&prompt=login';
const browser = await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage']});
try {
  for (const missingPatternFly of [false, true]) {
    for (const width of [480, 375, 320]) {
      const context = await browser.newContext({viewport:{width,height:900}});
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror',e=>errors.push(e.message));
      if (!process.env.OVG_AUTH_VERIFY_DEPLOYED) await page.route('**/openvietgis-modal-v3.css',route=>route.fulfill({contentType:'text/css',body:css}));
      if (missingPatternFly) await page.route('**/vendor/patternfly-v5/*.css',route=>route.abort());
      await page.route(origin+'/embedded-visual-test',route=>route.fulfill({contentType:'text/html',body:`<!doctype html><html><body style="margin:0"><iframe title="Keycloak" style="display:block;border:0;width:100%;height:875px" src="${authPath.replaceAll('&','&amp;')}"></iframe></body></html>`}));
      await page.goto(origin+'/embedded-visual-test',{waitUntil:'domcontentloaded'});
      const frame = page.frameLocator('iframe[title=Keycloak]');
      await frame.locator('.ovg-social-first').waitFor({timeout:15000});
      const positions = await frame.locator('body').evaluate(body=>{
        const get = selector=>{
          const rect=body.querySelector(selector)?.getBoundingClientRect();
          return rect && {left:rect.left,right:rect.right,top:rect.top,bottom:rect.bottom,width:rect.width,height:rect.height};
        };
        return {google:get('#social-google img'),button:get('#social-google'),username:get('#username'),password:get('#password'),submit:get('#kc-login'),main:get('.pf-v5-c-login__main'),scrollWidth:document.documentElement.scrollWidth,viewWidth:innerWidth};
      });
      const {google,button,username,password,submit,main,scrollWidth,viewWidth}=positions;
      for(const [label,r] of Object.entries({google,button,username,password,submit,main})) {
        assert.ok(r && r.left>=-1 && r.right<=viewWidth+1,`${label} overflows at ${width}, missing PF ${missingPatternFly}: ${JSON.stringify(positions)}`);
      }
      assert.ok(scrollWidth<=viewWidth+1,`horizontal scroll at ${width}: ${JSON.stringify(positions)}`);
      assert.ok(google.width<=24 && google.height<=24,'Google icon oversized');
      assert.ok(button.height<=70 && button.height>=40,'Google button height invalid');
      assert.ok(username.width>=width-90 && password.width>=width-150,'Input fields too narrow');
      assert.ok(submit.width>=width-90 && submit.bottom<720,'Submit inaccessible without excessive scrolling');
      assert.deepEqual(errors,[]);
      console.log(`PASS ${width}px; PatternFly ${missingPatternFly?'unavailable':'available'}; Google ${Math.round(google.width)}px; form ${Math.round(main.height)}px`);
      await context.close();
    }
  }
} finally {await browser.close()}
