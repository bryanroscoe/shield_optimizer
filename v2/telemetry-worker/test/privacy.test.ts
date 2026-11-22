import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import worker from "../src/index";

describe("Android privacy policy", () => {
  it("does not publish a placeholder contact", async () => {
    const res = await worker.fetch(new Request("https://telemetry.test/privacy"), { ...env, PRIVACY_CONTACT_EMAIL: undefined });
    expect(res.status).toBe(503);
    expect(res.headers.get("cache-control")).toBe("no-store");
  });

  it("publishes contact and actual retention without reading request metadata", async () => {
    const request = new Request("https://telemetry.test/privacy");
    Object.defineProperty(request, "cf", { get() { throw new Error("must not read location"); } });
    const res = await worker.fetch(request, { ...env, PRIVACY_CONTACT_EMAIL: "privacy@example.com", RAW_RETENTION_DAYS: "45" });
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/html");
    const text = await res.text();
    expect(text).toContain("mailto:privacy@example.com");
    expect(text).toContain("45 days");
    expect(text).toContain("Support ID");
    expect(text).toContain("com.atvoptimizer.mobile");
    expect(text).toContain("not all automatically anonymized");
  });

  it("describes what a bug report sends and how long it is kept", async () => {
    const res = await worker.fetch(new Request("https://telemetry.test/privacy"), {
      ...env,
      PRIVACY_CONTACT_EMAIL: "privacy@example.com",
      RAW_RETENTION_DAYS: "45",
      LOG_RETENTION_DAYS: "14",
    });
    const text = await res.text();
    expect(text).toContain("<h2>Bug reports you send</h2>");
    expect(text).toContain("<strong>Report a bug</strong> and press <strong>Send</strong>");
    expect(text).toContain("the description you typed");
    expect(text).toContain("deleted after 14 days");
    expect(text).toContain("does not store your request\nIP address or location");
    expect(text).not.toContain("does not upload its debug logs");
  });

  it("refuses markup in the configured contact and handles HEAD without a body", async () => {
    const bad = await worker.fetch(new Request("https://telemetry.test/privacy"), { ...env, PRIVACY_CONTACT_EMAIL: '<script>@example.com' });
    expect(bad.status).toBe(503);
    const head = await worker.fetch(new Request("https://telemetry.test/privacy", { method: "HEAD" }), { ...env, PRIVACY_CONTACT_EMAIL: "privacy@example.com" });
    expect(head.status).toBe(200);
    expect(await head.text()).toBe("");
    const post = await worker.fetch(new Request("https://telemetry.test/privacy", { method: "POST" }), env);
    expect(post.status).toBe(405);
  });
});
