import type { AlternativeAssetHolding } from "@/lib/types";
import type { AnalyzedStatement } from "@/lib/types/statements";
import { describe, expect, it } from "vitest";
import {
  ACCOUNT_AUTO,
  ACCOUNT_NEW,
  base64ToBytes,
  fileToBase64,
  formatBytes,
  isLoanPaper,
  LIABILITY_NEW,
  loanChoiceFor,
  needsOverlapDecision,
  overrideFor,
  replacedDocumentIds,
} from "./statement-files";

describe("statement files", () => {
  it("round-trips the bytes of a file through base64", async () => {
    const bytes = new Uint8Array([0, 1, 2, 250, 255]);
    const base64 = await fileToBase64(new Blob([bytes]));
    expect(Array.from(base64ToBytes(base64))).toEqual(Array.from(bytes));
  });

  it("sends no override when the account is left automatic", () => {
    expect(overrideFor(0, { account: ACCOUNT_AUTO, newAccountName: "", skip: false })).toBeNull();
  });

  it("sends the chosen account, a new name, or a skip", () => {
    expect(overrideFor(1, { account: "acc-1", newAccountName: "", skip: false })).toEqual({
      index: 1,
      accountId: "acc-1",
    });
    expect(
      overrideFor(0, { account: ACCOUNT_NEW, newAccountName: " Joint ", skip: false }),
    ).toEqual({ index: 0, newAccountName: "Joint" });
    expect(overrideFor(2, { account: "acc-1", newAccountName: "", skip: true })).toEqual({
      index: 2,
      skip: true,
    });
  });

  it("formats sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
  });
});

describe("overlapping statements", () => {
  const statement = (overlaps: { documentId: string }[], matched = "acc1") =>
    ({
      preview: { index: 0 },
      matchedAccount: { id: matched },
      overlaps,
    }) as unknown as AnalyzedStatement;
  const auto = { account: "auto", newAccountName: "", skip: false };

  it("asks for a decision until one is made", () => {
    const s = statement([{ documentId: "d1" }]);
    expect(needsOverlapDecision(s, undefined)).toBe(true);
    expect(needsOverlapDecision(s, { ...auto, overlap: "keep" })).toBe(false);
    expect(needsOverlapDecision(s, { ...auto, skip: true })).toBe(false);
    expect(needsOverlapDecision(statement([]), undefined)).toBe(false);
  });

  it("does not matter when the statement goes to another account", () => {
    const s = statement([{ documentId: "d1" }]);
    expect(needsOverlapDecision(s, { ...auto, account: "other" })).toBe(false);
    expect(needsOverlapDecision(s, { ...auto, account: "acc1" })).toBe(true);
  });

  it("lists the stored statements to replace", () => {
    const s = statement([{ documentId: "d1" }, { documentId: "d2" }]);
    expect(replacedDocumentIds([s], { 0: { ...auto, overlap: "replace" } })).toEqual(["d1", "d2"]);
    expect(replacedDocumentIds([s], { 0: { ...auto, overlap: "keep" } })).toEqual([]);
    expect(replacedDocumentIds([s], { 0: { ...auto, overlap: "replace", skip: true } })).toEqual(
      [],
    );
  });
});

describe("loan papers", () => {
  const paper = (reference?: string) =>
    ({
      preview: {
        index: 0,
        documentKind: "loan_offer",
        accountLabel: "Prêt personnel …5678",
        extra: reference ? { loan: { reference } } : {},
      },
      suggestedAccountName: "Banque - Crédit",
    }) as unknown as AnalyzedStatement;
  const liability = (id: string, key?: string) =>
    ({
      id,
      kind: "liability",
      name: id,
      metadata: key ? { statement_loan_key: key } : {},
    }) as unknown as AlternativeAssetHolding;

  it("goes to the liability of its loan number, else to a new one named after the loan", () => {
    const liabilities = [liability("other"), liability("mine", "12345-00012345678")];
    expect(isLoanPaper(paper())).toBe(true);
    expect(loanChoiceFor(paper("12345-00012345678"), undefined, liabilities)).toEqual({
      liabilityId: "mine",
    });
    expect(loanChoiceFor(paper("99999-00000000001"), undefined, liabilities)).toEqual({
      newAccount: { name: "Prêt personnel …5678", key: "99999-00000000001" },
    });
  });

  it("goes where the person chose", () => {
    const liabilities = [liability("other"), liability("mine", "12345-00012345678")];
    const choice = { account: "auto", newAccountName: "", skip: false, liability: "other" };
    expect(loanChoiceFor(paper("12345-00012345678"), choice, liabilities)).toEqual({
      liabilityId: "other",
    });
    const named = { ...choice, liability: LIABILITY_NEW, newAccountName: "Crédit voiture" };
    expect(loanChoiceFor(paper(), named, liabilities)).toEqual({
      newAccount: { name: "Crédit voiture", key: null },
    });
  });
});
