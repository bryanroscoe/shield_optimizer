import assert from "node:assert/strict";

export async function exerciseProfileCatalog(page, openedSources) {
  const catalog = page.getByRole("region", { name: "Device and package catalog" });
  await catalog.getByRole("heading", { name: "Device & package catalog", exact: true }).waitFor();
  assert.match(await catalog.innerText(), /427 device profiles.*1,971 package IDs/);
  assert.equal(await catalog.getByRole("checkbox").count(), 0);
  assert.equal(await catalog.getByRole("button", { name: /^(Disable|Uninstall|Apply|Optimize)$/ }).count(), 0);
  assert.equal(await catalog.getByText("Safe to remove", { exact: true }).count(), 0);

  const devices = catalog.getByRole("searchbox", { name: "Search device profiles" });
  const packages = catalog.getByRole("searchbox", { name: "Search package IDs" });
  await packages.fill("");
  await devices.fill("Acer");

  await catalog.locator('[data-profile="acer-ar3-r3"]').click();
  await catalog.locator('[data-package-source="com.google.android.feedback"]').waitFor();
  await catalog.locator('[data-profile="acer-ar32ap2841hdfl"]').click();
  await catalog.locator('[data-package-source="com.apps.function.gdpr"]').waitFor();
  const context = catalog.locator("[data-source-context]");
  await context.locator("summary").click();
  assert.match(await context.innerText(), /RTT7\.220308\.001/);
  assert.doesNotMatch(await context.innerText(), /UKR1\.251116/);

  await devices.fill("acer-ar3-r3");
  await packages.fill("com.realtek.leanbacklauncher.partnercustomizer");
  await catalog.locator('[data-profile="acer-ar3-r3"]').click();
  const observation = catalog.locator('[data-package-source="com.realtek.leanbacklauncher.partnercustomizer"]');
  await observation.waitFor();
  await observation.locator("summary").click();
  assert.match(await observation.innerText(), /Inputs button/);
  assert.match(await observation.innerText(), /Source note/);
  assert.doesNotMatch(await observation.innerText(), /Upstream disable entry/);
  await context.locator("summary").click();
  assert.match(await context.innerText(), /UKR1\.251116/);
  assert.doesNotMatch(await context.innerText(), /RTT7\.220308\.001/);
  await catalog.getByRole("button", { name: "Open pinned source", exact: true }).click();
  assert.match((await openedSources()).at(-1), /8672e68e8a24c1258e4662cd11daa4ed13795dad\/examples\/acer-ar3-r3\/debloat\.txt$/);

  await devices.fill("acer-ar32ap2841hdfl");
  await catalog.locator('[data-profile="acer-ar32ap2841hdfl"]').click();
  await observation.waitFor();
  assert.match(await observation.innerText(), /Upstream disable entry/);
  assert.doesNotMatch(await observation.innerText(), /Inputs button/);

  const license = catalog.locator(".catalog-license");
  await license.locator("summary").focus();
  await license.locator("summary").press("Enter");
  assert.match(await license.innerText(), /Copyright \(c\) 2026 Gathin/);
  assert.match(await license.innerText(), /Permission is hereby granted/);
  assert.match(await license.innerText(), /THE SOFTWARE IS PROVIDED "AS IS"/);

  await packages.fill("com.example.not_in_this_source_set");
  await catalog.getByText("No source profiles match these filters.", { exact: true }).waitFor();
  assert.equal(await catalog.locator("[data-package-source]").count(), 0);
  assert.equal(await catalog.locator("[data-source-context]").count(), 0);
  await packages.fill("com.realtek.leanbacklauncher.partnercustomizer");
  await devices.fill("acer-ar3-r3");
  await catalog.locator('[data-profile="acer-ar3-r3"]').click();
  await catalog.locator("[data-package-source]").first().waitFor();
  await catalog.locator("[data-package-source]").first().locator("summary").click();
  if (await license.evaluate((element) => element.open)) await license.locator("summary").click();
}

export async function labelSyntheticCapture(page) {
  await page.evaluate(() => {
    const label = document.createElement("div");
    label.textContent = "SIMULATED UI · bundled source data · no TV";
    label.style.cssText = "background:#17243a;color:white;padding:7px;font:12px system-ui;text-align:center;pointer-events:none";
    document.body.prepend(label);
  });
}
