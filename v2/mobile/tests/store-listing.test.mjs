import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { SCREENS } from "../screenshots/screens.mjs";
import { bannedWords, pricingWords, validateImage, validateListing } from "../scripts/store-listing-checks.mjs";

const dir = new URL("../store-listing/", import.meta.url);
const listing = JSON.parse(await readFile(new URL("listing.json", dir), "utf8"));

test("listing.json passes Play's metadata limits", () => {
  assert.deepEqual(validateListing(listing, { knownScreens: SCREENS.map((s) => s.name) }), []);
});

test("committed store images have Play's sizes and no alpha", async () => {
  const errors = [];
  for (const shot of listing.screenshots) {
    errors.push(...validateImage(shot.file, await readFile(new URL(shot.file, dir)), "screenshot"));
  }
  errors.push(...validateImage("feature-graphic.png", await readFile(new URL("feature-graphic.png", dir)), "featureGraphic"));
  errors.push(...validateImage("icon.png", await readFile(new URL("icon.png", dir)), "icon"));
  assert.deepEqual(errors, []);
});

test("banned promotional words match whole words only", () => {
  assert.deepEqual(bannedWords("The best way, #1 pick, Install now"), ["best", "#1", "install now"]);
  assert.deepEqual(bannedWords("Stop the renewal of carefree topics"), []);
  assert.deepEqual(pricingWords("Go Pro for $4.99"), ["Pro", "a price"]);
  assert.deepEqual(pricingWords("Productive approach"), []);
});

test("listing checks reject what Play rejects", () => {
  const shot = { file: "screenshots/01-a.png", screen: "dashboard", headline: "Best TV app", alt: "x" };
  const bad = {
    title: "x".repeat(31),
    shortDescription: "Tune your TV!",
    fullDescription: "ok",
    featureGraphic: { tagline: "Unlock Pro" },
    screenshots: [shot],
  };
  const errors = validateListing(bad, { knownScreens: ["dashboard"] }).join("\n");
  assert.match(errors, /title must be 1-30/);
  assert.match(errors, /shortDescription may only use/);
  assert.match(errors, /2-8 screenshots/);
  assert.match(errors, /headline uses words.*best/);
  assert.match(errors, /tagline uses words.*Pro/);
});
