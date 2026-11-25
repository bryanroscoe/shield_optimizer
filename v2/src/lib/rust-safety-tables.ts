/// The protected and caution tables in `crates/core/src/engine/safety.rs`,
/// read from its source so the demo layer gives a package the same verdict the
/// app does. A hand-kept copy drifted: the README screenshots showed Google
/// Feedback as safe to remove while the app calls it Caution.
export function rustSafetyTable(source: string, name: "NEVER_DISABLE" | "CAUTION"): Record<string, string> {
  const start = source.indexOf(`const ${name}: &[(&str, &str)] = &[`);
  if (start < 0) throw new Error(`safety.rs has no ${name} table`);
  const end = source.indexOf("\n];", start);
  const table: Record<string, string> = {};
  for (const m of source.slice(start, end).matchAll(/\(\s*"([^"\\]+)",\s*"((?:[^"\\]|\\[\s\S])*)",?\s*\)/g)) {
    table[m[1]] = unescapeRust(m[2]);
  }
  return table;
}

const SIMPLE_ESCAPES: Record<string, string> = { n: "\n", r: "\r", t: "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'" };

function unescapeRust(text: string): string {
  return text.replace(/\\(u\{([0-9a-fA-F]+)\}|x([0-9a-fA-F]{2})|\n\s*|[\s\S])/g, (_, escape: string, unicode?: string, hex?: string) => {
    if (unicode) return String.fromCodePoint(parseInt(unicode, 16));
    if (hex) return String.fromCharCode(parseInt(hex, 16));
    if (escape.startsWith("\n")) return "";
    if (escape in SIMPLE_ESCAPES) return SIMPLE_ESCAPES[escape];
    throw new Error(`safety.rs uses an escape the demo can't read: \\${escape}`);
  });
}
