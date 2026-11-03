// No Access-Control-Allow-Origin anywhere: both apps call from native code, and
// leaving CORS closed means a web page cannot make a visitor's browser post here.

export function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", ...headers },
  });
}

export function error(status: number, code: string, extra: Record<string, unknown> = {}, headers: Record<string, string> = {}): Response {
  return json({ error: code, ...extra }, status, { "cache-control": "no-store", ...headers });
}

// Reads at most `limit` bytes. Returns null when the body is larger, without
// buffering the rest of it.
export async function readBodyLimited(request: Request, limit: number): Promise<Uint8Array | null> {
  const declared = request.headers.get("content-length");
  if (declared !== null && Number(declared) > limit) return null;
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > limit) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.byteLength;
  }
  return out;
}
