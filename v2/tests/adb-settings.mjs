import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { createServer } from 'vite';
import { chromium } from 'playwright';
process.env.VITE_DEMO = '1';
const evidence = process.env.SETTINGS_EVIDENCE;
if (evidence) await mkdir(evidence, {recursive:true});
let server, browser;
try {
 server = await createServer({root:resolve(import.meta.dirname,'..'),server:{host:'127.0.0.1',port:0,hmr:false}});
 await server.listen(); browser=await chromium.launch();
 const context=await browser.newContext({viewport:{width:1360,height:1000},...(evidence?{recordVideo:{dir:evidence}}:{})});
 const page=await context.newPage();
 await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`,{waitUntil:'networkidle'});
 await page.evaluate(()=>{const original=window.__TAURI_INTERNALS__.invoke;window.settingsCalls=[];window.__TAURI_INTERNALS__.invoke=async(command,args)=>{window.settingsCalls.push(command);return original(command,args);};});
 await page.getByRole('button',{name:'Settings',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'Settings',exact:true});
 await dialog.getByText(/37.0.1/).waitFor();
 assert.equal(await dialog.getByRole('checkbox',{name:'Stop ADB when quitting'}).isChecked(),false);
 assert.equal(await dialog.getByRole('checkbox',{name:'Auto-update'}).count(),1);
 assert.equal(await dialog.getByRole('group',{name:'Theme'}).count(),1);
 page.once('dialog',d=>d.dismiss());
 await dialog.getByRole('button',{name:'Re-download latest ADB',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.settingsCalls.includes('install_adb')),false);
 page.once('dialog',d=>d.accept());
 await dialog.getByRole('button',{name:'Re-download latest ADB',exact:true}).click();
 await dialog.getByText('Simulated re-download completed; no software was downloaded.').waitFor();
 assert.equal(await page.evaluate(()=>window.settingsCalls.filter(c=>c==='install_adb').length),1);
 assert.equal(await page.evaluate(()=>window.settingsCalls.includes('restart_adb')),false,'download must not restart shared server automatically');
 if(evidence){await page.screenshot({path:`${evidence}/settings-simulated.png`});await page.waitForTimeout(1200);}
 await dialog.getByRole('button',{name:'Close settings'}).click();
 await dialog.waitFor({state:'hidden'});
 await page.getByRole('button',{name:'Settings',exact:true}).click();
 await dialog.waitFor();await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
 assert.equal(await page.evaluate(()=>window.settingsCalls.filter(c=>c==='install_adb').length),1,'reopening settings must not reinstall');
 await context.close();console.log('PASS Settings navigation, default-off quit, version, confirmation, explicit restart and reopen safety (simulation)');
}finally{await browser?.close();await server?.close();}
