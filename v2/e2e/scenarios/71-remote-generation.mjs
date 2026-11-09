import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

export const scenarios = [{
  name: "remote-generation-ipc",
  async run(ctx) {
    await ctx.reset(shieldScenario());
    const before = await ctx.log();
    const first = await ctx.invoke("remote_open", { serial: SHIELD.key });
    const second = await ctx.invoke("remote_open", { serial: SHIELD.key });
    ctx.assert.equal(first.ok, true);
    ctx.assert.equal(second.ok, true);
    ctx.assert.match(first.value.generation, /^[1-9]\d*$/);
    ctx.assert.notEqual(first.value.generation, second.value.generation);
    const stale = { serial: SHIELD.key, generation: first.value.generation };
    for (const [command, args] of [
      ["send_key", { ...stale, key: "home", forceShell: true }],
      ["send_text", { ...stale, text: "fixture", forceShell: true }],
      ["open_settings", stale],
    ]) {
      const result = await ctx.invoke(command, args);
      ctx.assert.equal(result.ok, false);
      ctx.assert.match(String(result.error), /generation/i);
    }
    ctx.assert.equal((await ctx.invoke("remote_close", stale)).ok, true);
    ctx.assert.deepEqual(await ctx.log(), before, "logical open and stale requests do no device IO");
    const current = { serial: SHIELD.key, generation: second.value.generation };
    const sent = await ctx.invoke("send_key", { ...current, key: "home", forceShell: true });
    ctx.assert.equal(sent.ok, true);
    ctx.assert.equal(sent.value.ok, true);
    const after = await ctx.log();
    ctx.assert.equal(after.length, before.length + 1, "one simulated input, no replay");
    ctx.assert.equal((await ctx.invoke("remote_close", current)).ok, true);
    const closed = await ctx.invoke("send_key", { ...current, key: "home", forceShell: true });
    ctx.assert.equal(closed.ok, false);
    ctx.assert.deepEqual(await ctx.log(), after);
    const legacy = await ctx.invoke("send_text", { serial: SHIELD.key, text: "", forceShell: true });
    ctx.assert.equal(legacy.ok, true, "omitted optional generation remains deserializable");
    ctx.assert.equal(legacy.value.ok, false);
    const invalid = await ctx.invoke("send_key", { serial: SHIELD.key, generation: "01", key: "home", forceShell: true });
    ctx.assert.equal(invalid.ok, false);
    ctx.assert.match(String(invalid.error), /Invalid Remote/);
    ctx.assert.deepEqual(await ctx.log(), after);
  },
}];
