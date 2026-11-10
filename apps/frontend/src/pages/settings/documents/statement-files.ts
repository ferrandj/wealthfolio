import type { AnalyzedStatement, StatementOverride } from "@/lib/types/statements";

/** The bytes of a file as base64, without the data-URL prefix. */
export function fileToBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Could not read the file"));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.readAsDataURL(file);
  });
}

export function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** "auto" lets the service match or create the account; "new" creates one; anything else is an account id. */
export const ACCOUNT_AUTO = "auto";
export const ACCOUNT_NEW = "new";

/** What to do with the stored statements this one overlaps: replace them, or keep both. */
export type OverlapDecision = "replace" | "keep";

export interface StatementChoice {
  account: string;
  newAccountName: string;
  skip: boolean;
  overlap?: OverlapDecision;
}

/** The overlap only matters while the statement goes to the account it was matched with. */
export function overlapsApply(statement: AnalyzedStatement, choice: StatementChoice): boolean {
  if (choice.skip || statement.overlaps.length === 0) return false;
  return (
    choice.account === ACCOUNT_AUTO ||
    (!!statement.matchedAccount && choice.account === statement.matchedAccount.id)
  );
}

/** A statement that overlaps another one is not imported until the person has said what to do. */
export function needsOverlapDecision(
  statement: AnalyzedStatement,
  choice: StatementChoice | undefined,
): boolean {
  const c = choice ?? { account: ACCOUNT_AUTO, newAccountName: "", skip: false };
  return overlapsApply(statement, c) && !c.overlap;
}

/** The stored statements to replace, for every statement of the file that chose so. */
export function replacedDocumentIds(
  statements: AnalyzedStatement[],
  choices: Record<number, StatementChoice>,
): string[] {
  const ids = new Set<string>();
  for (const s of statements) {
    const choice = choices[s.preview.index];
    if (choice?.overlap === "replace" && overlapsApply(s, choice)) {
      for (const o of s.overlaps) ids.add(o.documentId);
    }
  }
  return [...ids];
}

export function overrideFor(index: number, choice: StatementChoice): StatementOverride | null {
  if (choice.skip) return { index, skip: true };
  if (choice.account === ACCOUNT_NEW) {
    return { index, newAccountName: choice.newAccountName.trim() || null };
  }
  if (choice.account !== ACCOUNT_AUTO) return { index, accountId: choice.account };
  return null;
}

export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}
