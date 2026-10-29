import assert from 'node:assert/strict';
import {readFile,mkdir,realpath} from 'node:fs/promises';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'vite';
import {chromium} from 'playwright';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const entries=JSON.parse(await readFile(resolve(root,'tests/fixtures/reported-package-names.json'),'utf8'));
const names=JSON.parse(await readFile(resolve(root,'crates/core/data/app-lists/known-names.json'),'utf8'));
assert.equal(new Set(entries.map(e=>e.package)).size,15);
for(const e of entries){assert.equal(names[e.package].name,e.name);assert.equal(new URL(e.source_url).searchParams.get('id'),e.package);for(const file of ['common','shield','googletv'])assert.ok(!JSON.parse(await readFile(resolve(root,`crates/core/data/app-lists/${file}.json`),'utf8')).some(row=>row.package===e.package));}
const evidence=process.env.REPORTED_NAMES_EVIDENCE;if(evidence)await mkdir(evidence,{recursive:true});
process.env.VITE_DEMO='1';delete process.env.TAURI_DEV_HOST;
let server,browser;
try{
server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false,fs:{allow:[root,await realpath(resolve(root,'node_modules'))]}}});await server.listen();browser=await chromium.launch();
const context=await browser.newContext({viewport:{width:1360,height:1050},...(evidence?{recordVideo:{dir:evidence+'/raw'}}:{})});
await context.addInitScript(({entries,names})=>{let bridge;window.catalogNamesEnabled=localStorage.getItem('reportedNamesEnabled')==='1';Object.defineProperty(window,'__TAURI_INTERNALS__',{configurable:true,get:()=>bridge,set(value){const original=value.invoke.bind(value);value.invoke=async(command,args={})=>{if(command==='list_other_packages')return entries.map(e=>({package:e.package,name:window.catalogNamesEnabled?names[e.package].name:null,description:window.catalogNamesEnabled?names[e.package].description:null,system:false,enabled:true}));if(command==='safety_info'&&args.package==='com.klevico.monet')return{kind:'caution',source:'launcher_list',reason:'This is a Home screen. Keep an enabled replacement before disabling your current launcher.'};return original(command,args);};bridge=value;}});},{entries,names});
const page=await context.newPage();const url=`http://127.0.0.1:${server.httpServer.address().port}`;
async function openApps(){await page.goto(url,{waitUntil:'networkidle'});await page.getByText('NVIDIA SHIELD',{exact:false}).first().click();await page.getByRole('tab',{name:'App List',exact:true}).click();await page.locator('.other-apps tbody tr').first().waitFor({timeout:10000});await page.evaluate(()=>{const tag=document.createElement('div');tag.textContent='SIMULATION — package fixtures, no ADB or TV';tag.style='position:fixed;bottom:0;right:0;padding:10px;background:#182334;color:white;z-index:99999';document.body.append(tag);});}
await openApps();await page.locator('.other-apps').evaluate(el=>el.scrollIntoView({block:'start'}));await page.waitForTimeout(1000);if(evidence)await page.locator('.other-apps').screenshot({path:evidence+'/before-simulated.png'});
await page.evaluate(()=>localStorage.setItem('reportedNamesEnabled','1'));await openApps();
for(const e of entries){const row=page.locator('.other-apps tbody tr').filter({has:page.getByText(e.package,{exact:true})});assert.equal(await row.count(),1);assert.ok((await row.innerText()).includes(e.name));assert.ok((await row.innerText()).includes(names[e.package].description));await row.getByText(e.package==='com.klevico.monet'?'Caution':'Unknown',{exact:true}).waitFor({timeout:5000});}
await page.locator('.other-apps').evaluate(el=>el.scrollIntoView({block:'start'}));await page.waitForTimeout(1200);if(evidence)await page.locator('.other-apps').screenshot({path:evidence+'/after-simulated.png'});
const video=page.video();await context.close();if(evidence)await video.saveAs(evidence+'/reported-names-simulated.webm');console.log('PASS 15 identities, display names and unchanged Unknown/Caution fixtures; no curated/default additions. Browser simulation only.');
}finally{await browser?.close();await server?.close();}
