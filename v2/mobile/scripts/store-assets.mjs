// Google Play listing assets for the phone app: `npm run store-assets`.
//
// store-listing/listing.json drives everything. For each of its screenshots
// this captures the real app screen (VITE_DEMO=1 fixtures, see
// screenshots/screens.mjs) and composes it under its caption on a
// 1242x2208 canvas. It also renders the 1024x500 feature graphic, the
// full-bleed 512x512 store icon, and store-listing/preview.png, a gitignored
// review sheet laid out roughly like the Play listing. Raw captures go to a
// temp dir and are removed; outputs are written only after every check passes.

import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync } from "node:zlib";
import { chromium } from "playwright";
import { SCREENS, captureScreen, screenByName, startDemoServer } from "../screenshots/screens.mjs";
import {
  CAPTION_MAX_SHARE, FEATURE_GRAPHIC_SIZE, ICON_SIZE, SCREENSHOT_SIZE, decodePng, validateImage, validateListing,
} from "./store-listing-checks.mjs";

const ROOT = fileURLToPath(new URL("../", import.meta.url));
const OUT = join(ROOT, "store-listing");
const SHOTS = join(OUT, "screenshots");

// Composite geometry, in canvas px. The card starts right under the caption
// block and runs past the bottom edge so only its top corners show.
const W = SCREENSHOT_SIZE.width;
const H = SCREENSHOT_SIZE.height;
const CAPTION_H = 440;
const CARD_W = 1050;
const CARD_X = (W - CARD_W) / 2;
const CARD_TOP = CAPTION_H;
// The raw screen: 412 CSS px wide at 3x (1236 px), drawn at CARD_W, tall
// enough that the drawn screen reaches past the canvas bottom.
const CAPTURE_SCALE = 3;
const CAPTURE_W = 412;
const CAPTURE_H = Math.ceil(((H - CARD_TOP + 16) * CAPTURE_W) / CARD_W);

// Per-screen tweaks so the card shows the part of the screen that tells the
// story rather than an empty lower half.
const PREPARE = {
  // The caption names CEC, frame rate, animations and DNS; they start at
  // Picture & sound, below the remote-button settings that open the screen.
  tweaks: (page) => scrollToTop(page.locator(".section-label", { hasText: "Picture & sound" })),
};

async function scrollToTop(locator) {
  await locator.evaluate((el) => {
    el.style.scrollMarginTop = "6px";
    el.scrollIntoView({ block: "start" });
  });
}

const TOKENS = {
  brand: "#244b43",
  deep: "#0d1614",
  accent: "#c9f24e",
  ink: "#f8faf2",
  soft: "#e0eddb",
};

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const dataUrl = (buf, type) => `data:${type};base64,${buf.toString("base64")}`;

function headlineHtml(headline, accent) {
  if (!accent) return esc(headline);
  const at = headline.indexOf(accent);
  return esc(headline.slice(0, at)) + `<span class="accent">${esc(accent)}</span>` + esc(headline.slice(at + accent.length));
}

// Chromium writes an opaque page as 24-bit RGB; Play asks for a 32-bit icon.
function toRgbaPng(buffer) {
  const png = decodePng(buffer);
  const rows = [];
  for (let y = 0; y < png.height; y++) {
    const row = Buffer.alloc(1 + png.width * 4);
    for (let x = 0; x < png.width; x++) Buffer.from(png.pixel(x, y)).copy(row, 1 + x * 4);
    rows.push(row);
  }
  const chunk = (type, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(data.length);
    head.write(type, 4, "latin1");
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])));
    return Buffer.concat([head, data, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(png.width, 0);
  ihdr.writeUInt32BE(png.height, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  return Buffer.concat([
    buffer.subarray(0, 8), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(Buffer.concat(rows), { level: 9 })), chunk("IEND", Buffer.alloc(0)),
  ]);
}

function fail(errors) {
  if (!errors.length) return;
  for (const e of errors) console.error(`  ✗ ${e}`);
  throw new Error(`store assets: ${errors.length} check(s) failed`);
}

const fontCss = (font) => `@font-face{font-family:Geist;src:url(${dataUrl(font, "font/woff2")}) format('woff2');font-weight:100 900}`;

// Shared background language: brand green into near-black, one soft lime
// glow, and a faint grid that fades out so the field isn't flat.
const backgroundCss = ({ glowX, glowY, glowW, glowH, from, to, angle, gridFade }) => `
  .bg{position:absolute;inset:0;background:linear-gradient(${angle},${from} 0%,${to} 100%)}
  .glow{position:absolute;left:${glowX - glowW / 2}px;top:${glowY - glowH / 2}px;width:${glowW}px;height:${glowH}px;
    background:radial-gradient(closest-side,rgba(201,242,78,.30),rgba(201,242,78,.10) 45%,rgba(201,242,78,0) 100%)}
  .grid{position:absolute;inset:0;
    background-image:linear-gradient(rgba(224,237,219,.045) 1px,transparent 1px),linear-gradient(90deg,rgba(224,237,219,.045) 1px,transparent 1px);
    background-size:46px 46px;background-position:-1px -1px;
    -webkit-mask-image:${gridFade};mask-image:${gridFade}}`;

function screenshotHtml(shot, frame, font) {
  return `<!doctype html><html lang="en"><meta charset="utf-8"><style>
    ${fontCss(font)}
    *{box-sizing:border-box}html,body{margin:0;width:${W}px;height:${H}px;overflow:hidden;background:${TOKENS.deep}}
    body{position:relative;font-family:Geist,sans-serif;-webkit-font-smoothing:antialiased}
    ${backgroundCss({
      glowX: W / 2, glowY: CARD_TOP + 60, glowW: 1500, glowH: 1000,
      from: TOKENS.brand, to: TOKENS.deep, angle: "180deg",
      gridFade: "linear-gradient(180deg,rgba(0,0,0,1) 0%,rgba(0,0,0,.35) 30%,rgba(0,0,0,0) 55%)",
    })}
    .caption{position:absolute;left:0;top:0;width:${W}px;height:${CAPTION_H}px;padding:20px 40px 0;
      display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center}
    h1{margin:0;font-size:96px;line-height:1.04;font-weight:650;letter-spacing:-0.03em;color:${TOKENS.ink};
      text-wrap:balance;max-width:1010px}
    h1 .accent{color:${TOKENS.accent}}
    p{margin:24px 0 0;font-size:48px;line-height:1.2;font-weight:500;letter-spacing:-0.015em;color:${TOKENS.soft};
      text-wrap:balance;max-width:1162px}
    .card{position:absolute;left:${CARD_X}px;top:${CARD_TOP}px;width:${CARD_W}px;border-radius:56px 56px 0 0;overflow:hidden;
      background:#0b0d10;box-shadow:0 -2px 0 rgba(224,237,219,.02),0 30px 80px rgba(0,0,0,.45),0 60px 160px rgba(0,0,0,.55)}
    .card img{display:block;width:100%}
    .card::after{content:"";position:absolute;inset:0;border-radius:inherit;border:1px solid rgba(224,237,219,.16);border-bottom:0;pointer-events:none}
  </style><body><div class="bg"></div><div class="glow"></div><div class="grid"></div>
    <div class="caption"><h1>${headlineHtml(shot.headline, shot.accent)}</h1>${shot.subline ? `<p>${esc(shot.subline)}</p>` : ""}</div>
    <div class="card"><img src="${dataUrl(frame, "image/png")}" alt=""></div></body></html>`;
}

function featureHtml(listing, frames, icon, font) {
  const { width: FW, height: FH } = FEATURE_GRAPHIC_SIZE;
  const card = (frame, cls) => `<div class="fcard ${cls}"><img src="${dataUrl(frame, "image/png")}" alt=""></div>`;
  return `<!doctype html><html lang="en"><meta charset="utf-8"><style>
    ${fontCss(font)}
    *{box-sizing:border-box}html,body{margin:0;width:${FW}px;height:${FH}px;overflow:hidden;background:${TOKENS.brand}}
    body{position:relative;font-family:Geist,sans-serif;-webkit-font-smoothing:antialiased}
    ${backgroundCss({
      glowX: 800, glowY: 250, glowW: 720, glowH: 600,
      from: "#2c5a4f", to: "#173731", angle: "120deg",
      gridFade: "radial-gradient(ellipse 70% 90% at 70% 40%,rgba(0,0,0,.9),rgba(0,0,0,0) 75%)",
    })}
    .text{position:absolute;left:154px;top:75px;height:350px;width:420px;display:flex;flex-direction:column;justify-content:center}
    .name{display:flex;align-items:center;gap:14px}
    .name svg{width:52px;height:52px;flex:none;border-radius:14px}
    h1{margin:0;font-size:56px;line-height:1;font-weight:650;letter-spacing:-0.03em;color:${TOKENS.ink};white-space:nowrap}
    p{margin:22px 0 0;font-size:30px;line-height:1.25;font-weight:450;letter-spacing:-0.01em;color:${TOKENS.soft};text-wrap:balance}
    .fcard{position:absolute;width:244px;border-radius:22px;overflow:hidden;background:#0b0d10;
      box-shadow:0 18px 40px rgba(0,0,0,.40),0 40px 90px rgba(0,0,0,.45)}
    .fcard img{display:block;width:100%}
    .fcard::after{content:"";position:absolute;inset:0;border-radius:inherit;border:1px solid rgba(224,237,219,.18);pointer-events:none}
    .back{left:656px;top:136px;transform:rotate(-8deg);opacity:.9}
    .front{left:792px;top:92px;transform:rotate(3deg);z-index:2}
  </style><body><div class="bg"></div><div class="glow"></div><div class="grid"></div>
    <div class="text"><div class="name">${icon}<h1>${esc(listing.title)}</h1></div><p>${esc(listing.featureGraphic.tagline)}</p></div>
    ${card(frames[1], "back")}${card(frames[0], "front")}</body></html>`;
}

function previewHtml(listing, feature, shots, iconPng, font) {
  const intro = listing.fullDescription.split("\n\n").slice(0, 2).map((para) => `<p>${esc(para).replace(/\n/g, "<br>")}</p>`).join("");
  return `<!doctype html><html lang="en"><meta charset="utf-8"><style>
    ${fontCss(font)}
    *{box-sizing:border-box}html,body{margin:0;background:#131416}
    body{width:2320px;padding:64px 80px 80px;font-family:Geist,sans-serif;color:#e8eaed;-webkit-font-smoothing:antialiased}
    .feature{display:block;width:1024px;height:500px;border-radius:20px;margin:0 auto 48px}
    header{display:flex;align-items:center;gap:28px;margin-bottom:40px}
    header img{width:112px;height:112px;border-radius:24px}
    h1{margin:0;font-size:52px;font-weight:600;letter-spacing:-0.02em}
    .short{margin:10px 0 0;font-size:26px;color:#bdc1c6}
    .row{display:flex;gap:20px;margin-bottom:48px}
    .row img{width:250px;height:${Math.round((250 * H) / W)}px;border-radius:16px;display:block}
    h2{margin:0 0 16px;font-size:30px;font-weight:600}
    .about{max-width:1400px;font-size:24px;line-height:1.5;color:#bdc1c6}
    .about p{margin:0 0 16px}
  </style><body>
    <img class="feature" src="${dataUrl(feature, "image/png")}" alt="">
    <header><img src="${dataUrl(iconPng, "image/png")}" alt=""><div><h1>${esc(listing.title)}</h1><p class="short">${esc(listing.shortDescription)}</p></div></header>
    <div class="row">${shots.map((s) => `<img src="${dataUrl(s, "image/png")}" alt="">`).join("")}</div>
    <h2>About this app</h2><div class="about">${intro}<p>…</p></div></body></html>`;
}

const listing = JSON.parse(await readFile(join(OUT, "listing.json"), "utf8"));
fail(validateListing(listing, { knownScreens: SCREENS.map((s) => s.name) }));

const font = await readFile(join(ROOT, "src/fonts/Geist-Variable.woff2"));
const iconSvg = await readFile(join(ROOT, "src-tauri/icons/icon.svg"), "utf8");

// Everything is rendered and checked before store-listing/ is touched, so a
// failed run leaves the committed files as they were.
const outputs = new Map();
let temp;
let server;
let browser;
try {
  temp = await mkdtemp(join(tmpdir(), "atvopt-store-"));
  server = await startDemoServer();
  browser = await chromium.launch();
  console.log(`Capturing ${listing.screenshots.length} screens at ${CAPTURE_W}x${CAPTURE_H}@${CAPTURE_SCALE}x`);
  const frames = {};
  for (const shot of listing.screenshots) {
    const path = join(temp, `${shot.screen}.png`);
    await captureScreen(browser, server.origin, screenByName(shot.screen), {
      viewport: { width: CAPTURE_W, height: CAPTURE_H }, scale: CAPTURE_SCALE, path, prepare: PREPARE[shot.screen],
      fixedTime: new Date("2026-10-21T19:27:00-07:00"), timezoneId: "America/Los_Angeles",
    });
    frames[shot.screen] = await readFile(path);
  }

  // Play applies its own corner mask, so the store icon is the app icon's
  // glyph on a full-bleed square rather than the rounded app icon file.
  const square = iconSvg.replace('<rect width="120" height="120" rx="28"', '<rect width="120" height="120"');
  if (square === iconSvg) throw new Error("icon.svg no longer starts with the rounded background rect; update the store icon render");
  const page = await browser.newPage({ viewport: ICON_SIZE, deviceScaleFactor: 1 });
  await page.setContent(`<!doctype html><style>html,body{margin:0;background:#c9f24e}svg{display:block;width:${ICON_SIZE.width}px;height:${ICON_SIZE.height}px}</style>${square}`);
  const iconPng = toRgbaPng(await page.screenshot());
  fail(validateImage("icon.png", iconPng, "icon"));
  outputs.set("icon.png", iconPng);
  console.log("  icon.png");

  await page.setViewportSize({ width: W, height: H });
  const composites = [];
  for (const shot of listing.screenshots) {
    await page.setContent(screenshotHtml(shot, frames[shot.screen], font), { waitUntil: "load" });
    await page.evaluate(() => document.fonts.ready);
    const m = await page.evaluate(() => {
      const box = (sel) => document.querySelector(sel)?.getBoundingClientRect();
      const h1 = box("h1");
      const lh = parseFloat(getComputedStyle(document.querySelector("h1")).lineHeight);
      const sub = box("p");
      return {
        caption: box(".caption").bottom,
        textBottom: (sub ?? h1).bottom,
        textTop: h1.top,
        lines: Math.round(h1.height / lh),
        subLines: sub ? Math.round(sub.height / parseFloat(getComputedStyle(document.querySelector("p")).lineHeight)) : 0,
        card: box(".card").top,
      };
    });
    const errors = [];
    if (m.caption > H * CAPTION_MAX_SHARE) errors.push(`${shot.file}: caption block is ${m.caption}px, over ${CAPTION_MAX_SHARE * 100}% of ${H}`);
    if (m.textBottom > m.caption || m.textTop < 0) errors.push(`${shot.file}: caption text overflows its block`);
    if (m.card < m.textBottom) errors.push(`${shot.file}: headline collides with the screen card`);
    if (m.lines > 2) errors.push(`${shot.file}: headline wraps to ${m.lines} lines (max 2)`);
    if (m.subLines > 2) errors.push(`${shot.file}: subline wraps to ${m.subLines} lines (max 2)`);
    fail(errors);
    const buf = await page.screenshot();
    fail(validateImage(shot.file, buf, "screenshot"));
    outputs.set(shot.file, buf);
    composites.push(buf);
    console.log(`  ${shot.file}  headline ${m.lines}, subline ${m.subLines} line(s), text ${Math.round(m.textTop)}-${Math.round(m.textBottom)}px of a ${m.caption}px caption (${((m.caption / H) * 100).toFixed(1)}%)`);
  }

  const featureFrames = ["dashboard", "optimize"].map((name) => frames[name]);
  if (featureFrames.some((f) => !f)) throw new Error("feature graphic needs the dashboard and optimize screenshots");
  await page.setViewportSize(FEATURE_GRAPHIC_SIZE);
  await page.setContent(featureHtml(listing, featureFrames, iconSvg, font), { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  const feature = await page.screenshot();
  fail(validateImage("feature-graphic.png", feature, "featureGraphic"));
  outputs.set("feature-graphic.png", feature);
  console.log("  feature-graphic.png");

  // Review sheet only: gitignored, never uploaded.
  await page.setViewportSize({ width: 2320, height: 1200 });
  await page.setContent(previewHtml(listing, feature, composites, iconPng, font), { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  outputs.set("preview.png", await page.screenshot({ fullPage: true }));
  console.log("  preview.png");
  await page.close();
} finally {
  await browser?.close();
  await server?.close();
  if (temp) await rm(temp, { recursive: true, force: true });
}

const keep = new Set(listing.screenshots.map((s) => s.file.replace(/^screenshots\//, "")));
for (const f of await readdir(SHOTS).catch(() => [])) {
  if (f.endsWith(".png") && !keep.has(f)) await rm(join(SHOTS, f));
}
await mkdir(SHOTS, { recursive: true });
for (const [file, buf] of outputs) await writeFile(join(OUT, file), buf);

const sizes = listing.screenshots.map((s) => outputs.get(s.file).length);
console.log(`Store assets → ${OUT} (largest screenshot ${(Math.max(...sizes) / 1024 / 1024).toFixed(2)} MB)`);
