import { inflateSync } from "node:zlib";

// Google Play listing rules checked by scripts/store-assets.mjs before it
// renders and by tests/store-listing.test.mjs against the committed assets.

export const SCREENSHOT_SIZE = { width: 1242, height: 2208 };
export const FEATURE_GRAPHIC_SIZE = { width: 1024, height: 500 };
export const ICON_SIZE = { width: 512, height: 512 };
export const CAPTION_MAX_SHARE = 0.2;
// The store icon's square background: Play masks the corners itself.
export const ICON_BACKGROUND = [0xc9, 0xf2, 0x4e, 255];
export const ICON_MAX_BYTES = 1024 * 1024;
export const SCREENSHOT_MAX_BYTES = 8 * 1024 * 1024;

// Play's metadata policy rejects these in store graphics and promotional text.
const BANNED = [
  "best", "#1", "top", "new", "free", "discount", "sale", "million downloads",
  "download now", "install now", "try now",
];

export function bannedWords(text) {
  const lower = String(text).toLowerCase();
  return BANNED.filter((word) => {
    const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`(^|[^a-z0-9#])${escaped}(?![a-z0-9])`, "i").test(lower);
  });
}

// Captions and the tagline also stay off pricing: no Pro upsell, no prices.
export function pricingWords(text) {
  const hits = [];
  if (/(^|[^a-z0-9])pro(?![a-z0-9])/i.test(String(text))) hits.push("Pro");
  if (/[$€£¥]\s?\d|\d\s?(usd|eur|gbp)\b|\bprice\b/i.test(String(text))) hits.push("a price");
  return hits;
}

// Returns a list of problems; empty means the listing passes.
export function validateListing(listing, { knownScreens } = {}) {
  const errors = [];
  const len = (s) => [...String(s ?? "")].length;
  const banned = (field, text, caption = false) => {
    const hits = [...bannedWords(text ?? ""), ...(caption ? pricingWords(text ?? "") : [])];
    if (hits.length) errors.push(`${field} uses words Play rejects in listing graphics: ${hits.join(", ")}`);
  };

  if (!listing.title || len(listing.title) > 30) errors.push(`title must be 1-30 characters (is ${len(listing.title)})`);
  if (!listing.shortDescription || len(listing.shortDescription) > 80) {
    errors.push(`shortDescription must be 1-80 characters (is ${len(listing.shortDescription)})`);
  }
  if (listing.shortDescription && !/^[A-Za-z0-9 ,.'&-]+$/.test(listing.shortDescription)) {
    errors.push("shortDescription may only use letters, digits, spaces and , . ' & -");
  }
  if (!listing.fullDescription || len(listing.fullDescription) > 4000) {
    errors.push(`fullDescription must be 1-4000 characters (is ${len(listing.fullDescription)})`);
  }
  banned("title", listing.title);
  banned("shortDescription", listing.shortDescription);

  const tagline = listing.featureGraphic?.tagline;
  if (!tagline) errors.push("featureGraphic.tagline is required");
  banned("featureGraphic.tagline", tagline, true);

  const shots = listing.screenshots ?? [];
  if (shots.length < 2 || shots.length > 8) errors.push(`2-8 screenshots required (has ${shots.length})`);
  const files = new Set();
  shots.forEach((shot, i) => {
    const at = `screenshots[${i}]`;
    if (!/^screenshots\/[\w.-]+\.png$/.test(shot.file ?? "")) errors.push(`${at}.file must be screenshots/<name>.png`);
    if (files.has(shot.file)) errors.push(`${at}.file ${shot.file} is used twice`);
    files.add(shot.file);
    if (!shot.screen) errors.push(`${at}.screen is required`);
    else if (knownScreens && !knownScreens.includes(shot.screen)) errors.push(`${at}.screen "${shot.screen}" is not a known screen`);
    if (!shot.headline) errors.push(`${at}.headline is required`);
    if (len(shot.headline) > 34) errors.push(`${at}.headline is over 34 characters`);
    if (shot.subline && len(shot.subline) > 48) errors.push(`${at}.subline is over 48 characters`);
    if (!shot.alt || len(shot.alt) > 140) errors.push(`${at}.alt must be 1-140 characters (is ${len(shot.alt)})`);
    if (shot.accent && !String(shot.headline).includes(shot.accent)) {
      errors.push(`${at}.accent "${shot.accent}" does not appear in its headline`);
    }
    banned(`${at}.headline`, shot.headline, true);
    banned(`${at}.subline`, shot.subline, true);
  });
  return errors;
}

// Reads a PNG's header: dimensions and whether it carries an alpha channel.
export function pngInfo(buffer) {
  const signature = "89504e470d0a1a0a";
  if (buffer.subarray(0, 8).toString("hex") !== signature || buffer.toString("latin1", 12, 16) !== "IHDR") {
    throw new Error("not a PNG");
  }
  const colorType = buffer[25];
  let hasTransparency = colorType === 4 || colorType === 6;
  for (let at = 8; at < buffer.length;) {
    const size = buffer.readUInt32BE(at);
    const type = buffer.toString("latin1", at + 4, at + 8);
    if (type === "tRNS") hasTransparency = true;
    if (type === "IEND") break;
    at += 12 + size;
  }
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
    bitDepth: buffer[24],
    colorType,
    hasAlpha: hasTransparency,
    bytes: buffer.length,
  };
}

// Decodes an 8-bit, non-interlaced RGB or RGBA PNG into raw pixels.
export function decodePng(buffer) {
  const info = pngInfo(buffer);
  const channels = { 2: 3, 6: 4 }[info.colorType];
  if (!channels || info.bitDepth !== 8 || buffer[28] !== 0) throw new Error("only 8-bit non-interlaced RGB/RGBA PNGs are supported");
  const chunks = [];
  for (let at = 8; at < buffer.length;) {
    const size = buffer.readUInt32BE(at);
    if (buffer.toString("latin1", at + 4, at + 8) === "IDAT") chunks.push(buffer.subarray(at + 8, at + 8 + size));
    at += 12 + size;
  }
  const raw = inflateSync(Buffer.concat(chunks));
  const stride = info.width * channels;
  const pixels = Buffer.alloc(stride * info.height);
  for (let y = 0; y < info.height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const value = raw[y * (stride + 1) + 1 + x];
      const a = x >= channels ? pixels[y * stride + x - channels] : 0;
      const b = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const c = x >= channels && y > 0 ? pixels[(y - 1) * stride + x - channels] : 0;
      const p = a + b - c;
      const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
      const predictor = [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][filter];
      pixels[y * stride + x] = (value + predictor) & 0xff;
    }
  }
  const pixel = (x, y) => {
    const at = (y * info.width + x) * channels;
    return [pixels[at], pixels[at + 1], pixels[at + 2], channels === 4 ? pixels[at + 3] : 255];
  };
  return { ...info, channels, pixel };
}

// `kind` is "screenshot", "featureGraphic" or "icon".
export function validateImage(name, buffer, kind) {
  const errors = [];
  const info = pngInfo(buffer);
  const want = { screenshot: SCREENSHOT_SIZE, featureGraphic: FEATURE_GRAPHIC_SIZE, icon: ICON_SIZE }[kind];
  if (info.width !== want.width || info.height !== want.height) {
    errors.push(`${name} is ${info.width}x${info.height}, expected ${want.width}x${want.height}`);
  }
  if (kind === "icon") {
    if (info.bytes > ICON_MAX_BYTES) errors.push(`${name} is ${info.bytes} bytes, over 1024 KB`);
    if (info.colorType !== 6 || info.bitDepth !== 8) errors.push(`${name} must be a 32-bit RGBA PNG`);
    else {
      const png = decodePng(buffer);
      const corners = [[0, 0], [png.width - 1, 0], [0, png.height - 1], [png.width - 1, png.height - 1]];
      for (const [x, y] of corners) {
        const px = png.pixel(x, y);
        if (px.join() !== ICON_BACKGROUND.join()) {
          errors.push(`${name} corner (${x},${y}) is rgba(${px.join(",")}), expected the opaque full-bleed background`);
        }
      }
    }
    return errors;
  }
  if (info.hasAlpha || info.colorType !== 2 || info.bitDepth !== 8) {
    errors.push(`${name} must be a 24-bit PNG without alpha (color type ${info.colorType}, depth ${info.bitDepth})`);
  }
  if (kind === "screenshot") {
    const long = Math.max(info.width, info.height);
    const short = Math.min(info.width, info.height);
    if (short < 320 || long > 3840) errors.push(`${name} sides must be 320-3840 px`);
    if (long > short * 2.3) errors.push(`${name} long side is over 2.3x the short side`);
    if (info.bytes > SCREENSHOT_MAX_BYTES) errors.push(`${name} is ${info.bytes} bytes, over 8 MB`);
  }
  return errors;
}
