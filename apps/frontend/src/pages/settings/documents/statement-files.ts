import type { AlternativeAssetHolding } from "@/lib/types";
import type {
  AnalyzedStatement,
  LoanAccountChoice,
  LoanView,
  StatementOverride,
} from "@/lib/types/statements";

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
  /** A loan paper: the liability it goes to (an id, or `LIABILITY_NEW`); unset, the one of its loan. */
  liability?: string;
}

/** The kinds of the papers of a loan: they go to a liability, not to an account. */
export const LOAN_PAPER_KINDS = ["loan_offer", "loan_schedule", "loan_event", "loan_statement"];
/** A new liability, made empty for the loan. */
export const LIABILITY_NEW = "__new_liability__";
const LOAN_KEY = "statement_loan_key";

export const isLoanPaper = (statement: AnalyzedStatement): boolean =>
  LOAN_PAPER_KINDS.includes(statement.preview.documentKind ?? "");

/** The loan number a paper gives ("80372-00060536765"), if any. */
export function loanReference(statement: AnalyzedStatement): string | null {
  const loan = statement.preview.extra?.loan as { reference?: unknown } | undefined;
  return typeof loan?.reference === "string" && loan.reference ? loan.reference : null;
}

/** The liabilities a loan paper can go to. */
export const liabilitiesOf = (holdings: AlternativeAssetHolding[]) =>
  holdings.filter((h) => h.kind.toLowerCase() === "liability");

/**
 * The liability of a loan paper when the person did not pick one: the one its loan is already attached to, else the
 * one made for the same loan number, else a new one.
 */
export function defaultLiability(
  statement: AnalyzedStatement,
  liabilities: AlternativeAssetHolding[],
  loan?: LoanView,
): string {
  if (loan?.liabilityId) return loan.liabilityId;
  const reference = loanReference(statement);
  const known = reference
    ? liabilities.find((l) => l.metadata?.[LOAN_KEY] === reference)
    : undefined;
  return known?.id ?? LIABILITY_NEW;
}

/** What to tell the service about the liability of an imported loan paper. */
export function loanChoiceFor(
  statement: AnalyzedStatement,
  choice: StatementChoice | undefined,
  liabilities: AlternativeAssetHolding[],
  loan?: LoanView,
): LoanAccountChoice {
  const picked = choice?.liability ?? defaultLiability(statement, liabilities, loan);
  if (picked !== LIABILITY_NEW) return { liabilityId: picked };
  const name =
    choice?.newAccountName?.trim() ||
    statement.preview.accountLabel ||
    statement.suggestedAccountName ||
    "";
  return { newAccount: { name, key: loanReference(statement) } };
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
