import type { DocumentSource } from "@/lib/types/statements";

/** The rhythms offered for bringing a document back, in days (0: never asked). */
export const RHYTHMS = [7, 14, 30, 90, 365, 0] as const;

/** A day, `days` from `from`, as YYYY-MM-DD. */
export function dayAfter(days: number, from: Date = new Date()): string {
  const d = new Date(from.getTime() + days * 86_400_000);
  return d.toISOString().slice(0, 10);
}

/** The sources to ask for now. */
export function dueSources(sources: DocumentSource[]): DocumentSource[] {
  return sources.filter((s) => s.due);
}

/** The sources never described (asked once, after their first import). */
export function sourcesToDescribe(sources: DocumentSource[]): DocumentSource[] {
  return sources.filter((s) => s.needsSetup);
}
