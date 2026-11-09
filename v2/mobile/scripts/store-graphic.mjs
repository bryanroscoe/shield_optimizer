import { copyFile, mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const root = new URL("../", import.meta.url);
const output = new URL("store-listing/", root);
await mkdir(output, { recursive: true });
await copyFile(new URL("src-tauri/icons/icon.png", root), new URL("icon.png", output));
const icon = await readFile(new URL("src-tauri/icons/icon.svg", root), "utf8");
const font = (await readFile(new URL("src/fonts/Geist-Variable.woff2", root))).toString("base64");
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1024, height: 500 }, deviceScaleFactor: 1 });
  await page.setContent(`<!doctype html><html lang="en"><meta charset="utf-8"><title>ATV Optimizer</title>
    <style>
      @font-face{font-family:Geist;src:url(data:font/woff2;base64,${font}) format('woff2');font-weight:100 900}
      *{box-sizing:border-box}body{margin:0;background:#244b43;color:#f8faf2;font-family:Geist,sans-serif}
      main{height:500px;padding:72px 80px;display:flex;align-items:center;gap:48px}
      .icon{flex:none;width:160px;height:160px}.icon svg{width:100%;height:100%}
      h1{font-size:52px;font-weight:650;letter-spacing:-2px;line-height:1.1;margin:0 0 20px}
      p{font-size:26px;line-height:1.4;margin:0;color:#e0eddb;max-width:570px}
    </style><main><div class="icon">${icon}</div><div><h1>ATV Optimizer</h1>
    <p>Wireless diagnostics and app management for your TV.</p></div></main></html>`);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: fileURLToPath(new URL("feature-graphic.png", output)) });
} finally {
  await browser.close();
}
console.log(`Store icon and feature graphic → ${fileURLToPath(output)}`);
