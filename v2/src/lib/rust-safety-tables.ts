/// The protected and caution tables in `crates/core/src/engine/safety.rs`,
/// read from its source so the demo layer gives a package the same verdict the
/// app does. A hand-kept copy drifted: the README screenshots showed Google
/// Feedback as safe to remove while the app calls it Caution.
export function rustSafetyTable(source: string, name: "NEVER_DISABLE" | "CAUTION"): Record<string, string> {
  const start = source.indexOf(`const ${name}: &[(&str, &str)] = &[`);
  if (start < 0) throw new Error(`safety.rs has no ${name} table`);
  const end = source.indexOf("\n];", start);
  const table: Record<string, string> = {};
  for (const m of source.slice(start, end).matchAll(/\(\s*"([^"\\]+)",\s*"((?:[^"\\]|\\.)*)",?\s*\)/g)) {
    table[m[1]] = m[2].replace(/\\(.)/g, "$1");
  }
  return table;
}
