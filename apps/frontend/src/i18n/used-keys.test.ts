import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(__dirname, "..");
const LOCALES = join(__dirname, "locales");

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "node_modules" ? [] : sources(path);
    return /\.(tsx?|jsx?)$/.test(name) && !name.includes(".test.") ? [path] : [];
  });
}

const parsed = new Map<string, unknown>();
function load(locale: string, ns: string): unknown {
  const id = `${locale}/${ns}`;
  if (!parsed.has(id)) {
    try {
      parsed.set(id, JSON.parse(readFileSync(join(LOCALES, locale, `${ns}.json`), "utf8")));
    } catch {
      parsed.set(id, null);
    }
  }
  return parsed.get(id);
}

function has(locale: string, key: string): boolean {
  const [ns, path] = key.split(":");
  let node = load(locale, ns);
  if (node === null) return false;
  const parts = path.split(".");
  for (const [i, part] of parts.entries()) {
    if (typeof node !== "object" || node === null) return false;
    const map = node as Record<string, unknown>;
    if (part in map) node = map[part];
    else if (i === parts.length - 1 && (`${part}_other` in map || `${part}_one` in map))
      return true;
    else return false;
  }
  return true;
}

describe("translations", () => {
  // a label shown as its key ("contacts.holders_title") is a key used in code that no locale defines
  it("defines in French and in English every namespaced key the code uses", () => {
    const missing = new Set<string>();
    let seen = 0;
    for (const file of sources(SRC)) {
      const text = readFileSync(file, "utf8");
      for (const m of text.matchAll(/\bt\(\s*["'`]([a-zA-Z]+:[a-zA-Z0-9_.]+)["'`]/g)) {
        const key = m[1];
        if (key.endsWith(".")) continue;
        seen += 1;
        for (const locale of ["fr", "en"]) {
          if (!has(locale, key)) missing.add(`${locale} ${key} (${file.slice(SRC.length + 1)})`);
        }
      }
    }
    expect(seen).toBeGreaterThan(500); // the scan does read the code
    expect([...missing].sort()).toEqual([]);
  }, 60_000);
});
