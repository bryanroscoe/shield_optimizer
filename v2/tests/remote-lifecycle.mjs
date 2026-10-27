import assert from 'node:assert/strict';
import { mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.REMOTE_LIFECYCLE_EVIDENCE;
const warning = 'Remote input outcome unknown. It was not resent. Check the TV before retrying. SIMULATION: channel write acknowledgment lost.';
assert.ok((await readFile(resolve(root, 'crates/core/src/commands/input.rs'), 'utf8')).includes('Remote input outcome unknown. It was not resent. Check the TV before retrying.'));
const previous = { demo: process.env.VITE_DEMO, host: process.env.TAURI_DEV_HOST };
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser;
const pause = ms => evidence ? new Promise(resolve => setTimeout(resolve, ms)) : Promise.resolve();
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false, fs: { allow: [root, await realpath(resolve(root, 'node_modules'))] } } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1280, height: 950 }, ...(evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width: 1280, height: 950 } } } : {}) });
  const page = await context.newPage();
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
  await page.getByRole('tab', { name: 'Remote', exact: true }).click();
  await page.getByRole('textbox', { name: /Live typing capture/ }).waitFor();
  await page.evaluate(warning => {
    const original = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    window.calls=[]; window.counter=0; window.hold=''; window.release=null; window.fail=false;
    window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
      if(['remote_open','remote_close','send_key','send_text','open_settings'].includes(command)){
        window.calls.push({command,...args});
        if(window.hold===command){window.hold='';await new Promise(r=>window.release=r);}
        if(command==='remote_open')return{generation:String(++window.counter)};
        if(command==='remote_close')return null;
        if(window.fail){window.fail=false;throw warning;}
        return{ok:true,message:'',transport:'channel'};
      }
      const result=await original(command,args);
      if(command==='device_profile')return {...result,serial:args.serial};
      return result;
    };
    const label=document.createElement('div');label.textContent='SIMULATION — injected Remote lifecycle; no ADB or TV';
    label.style.cssText='position:fixed;bottom:0;right:0;background:#172337;color:white;padding:9px;z-index:99999;font-size:13px';document.body.append(label);
  },warning);
  const calls=()=>page.evaluate(()=>window.calls);
  const inputs=async()=>(await calls()).filter(c=>['send_key','send_text','open_settings'].includes(c.command));
  const clear=()=>page.evaluate(()=>window.calls=[]);
  const remote=()=>page.getByRole('tab',{name:'Remote',exact:true}).click();
  const hide=()=>page.getByRole('tab',{name:'Overview',exact:true}).click();
  const key=name=>page.getByRole('button',{name,exact:true}).click();
  const hold=command=>page.evaluate(c=>window.hold=c,command);
  const release=()=>page.evaluate(()=>{window.release();window.release=null;});
  const held=()=>page.waitForFunction(()=>window.release!==null);
  assert.deepEqual(await calls(),[]);
  await hold('send_key');await key('Home');await held();await key('Back');await hide();await release();await page.waitForTimeout(150);
  assert.equal((await inputs()).length,1,'hide discards unsent Back');
  assert.equal((await calls()).filter(c=>c.command==='remote_close').length,1);
  await remote();await key('Back');await page.waitForFunction(()=>window.calls.filter(c=>c.command==='send_key').length===2);
  assert.equal((await inputs())[1].generation,'2');
  await pause(1500);if(evidence)await page.screenshot({path:`${evidence}/remote-reopened-simulated.png`});
  await hide();await remote();await clear();await hold('remote_open');await key('Home');await held();await key('Back');await hide();await remote();await key('Back');
  await page.waitForFunction(()=>window.calls.some(c=>c.command==='send_key'));await release();await page.waitForTimeout(150);
  assert.equal((await inputs()).length,1,'late old open cannot dispatch queued input');
  assert.notEqual((await calls()).find(c=>c.command==='remote_close').generation,(await inputs())[0].generation);
  await clear();await hold('send_key');await page.evaluate(()=>window.fail=true);await key('Home');await held();await key('Back');await release();
  await page.getByText(warning,{exact:true}).waitFor();await page.waitForTimeout(200);assert.equal((await inputs()).length,1);
  if(evidence)await page.screenshot({path:`${evidence}/remote-queue-stopped-simulated.png`});await pause(4000);
  await key('Back');await page.waitForFunction(()=>window.calls.filter(c=>c.command==='send_key').length===2);
  await clear();const capture=page.getByRole('textbox',{name:/Live typing capture/});
  await page.getByRole('button',{name:'D-pad up',exact:true}).focus();await page.keyboard.press('Enter');await page.waitForFunction(()=>window.calls.some(c=>c.key==='up'));
  await capture.evaluate(el=>el.dispatchEvent(new KeyboardEvent('keydown',{key:'😀',bubbles:true,cancelable:true})));await page.waitForFunction(()=>window.calls.some(c=>c.text==='😀'));
  await capture.evaluate(el=>{el.dispatchEvent(new KeyboardEvent('keydown',{key:'Process',isComposing:true,bubbles:true}));el.dispatchEvent(new CompositionEvent('compositionend',{data:'漢字',bubbles:true}));});
  await page.waitForFunction(()=>window.calls.some(c=>c.text==='漢字'));assert.deepEqual((await inputs()).map(c=>c.key??c.text),['up','😀','漢字']);
  await clear();const up=page.getByRole('button',{name:'D-pad up',exact:true});await up.dispatchEvent('pointerdown');await page.waitForTimeout(720);await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
  const count=(await inputs()).length;await page.waitForTimeout(300);assert.equal((await inputs()).length,count);
  await up.dispatchEvent('pointerdown');await hide();const hiddenCount=(await inputs()).length;await page.waitForTimeout(700);assert.equal((await inputs()).length,hiddenCount);
  await remote();await clear();await hold('send_key');await key('Home');await held();await key('Back');await page.getByRole('checkbox',{name:'Force compatible mode'}).check();await release();await page.waitForTimeout(150);
  assert.equal((await inputs()).length,1);await key('Back');await page.waitForFunction(()=>window.calls.filter(c=>c.command==='send_key').length===2);assert.equal((await inputs())[1].forceShell,true);
  await clear();await hold('send_key');await key('Home');await held();await key('Back');
  await page.evaluate(()=>{const a=document.createElement('a');a.href='/devices/192.0.2.99%3A5555';a.id='change-target';a.textContent='SIMULATED target change';document.body.append(a);});
  await page.locator('#change-target').click();await page.waitForURL('**/devices/192.0.2.99%3A5555');await page.waitForTimeout(100);await release();await page.waitForTimeout(150);
  assert.equal((await inputs()).length,1,'serial change discards old queued work');
  await remote();await key('Back');await page.waitForFunction(()=>window.calls.filter(c=>c.command==='send_key').length===2);
  assert.equal((await inputs())[1].serial,'192.0.2.99:5555','new gesture uses selected target only');
  await clear();await hold('send_key');await key('Home');await held();await key('Back');await page.getByRole('button',{name:'Back to devices',exact:true}).click();await page.getByRole('heading',{name:'Devices',exact:true}).waitFor();await release();await page.waitForTimeout(150);
  assert.equal((await inputs()).length,1);assert.equal((await calls()).filter(c=>c.command==='remote_close').length,1);
  if(evidence)await writeFile(`${evidence}/remote-lifecycle-calls.json`,JSON.stringify(await calls(),null,2));await pause(1500);
  const video = page.video();
  await context.close();
  if (evidence) await video.saveAs(`${evidence}/remote-lifecycle-simulated.webm`);
  console.log('Remote lifecycle passed: lazy generations, hide/unmount cancellation, stale open close, no replay, keyboard, Unicode/IME, hold cleanup and compatibility invalidation. Injected simulation only.');
} finally {
  await browser?.close();
  await server?.close();
  if (previous.demo === undefined) delete process.env.VITE_DEMO; else process.env.VITE_DEMO = previous.demo;
  if (previous.host === undefined) delete process.env.TAURI_DEV_HOST; else process.env.TAURI_DEV_HOST = previous.host;
}
