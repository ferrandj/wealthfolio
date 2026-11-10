import { describe, expect, it } from "vitest";
import type { StatementDocument } from "@/lib/types/statements";
import type { ReviewedOperation, StatementReview } from "@/lib/types/statements";
import {
  confirmDoubts,
  doubtfulOperations,
  logicalOrder,
  ownTransferChoice,
  parseReviewReason,
  suggestedChoice,
} from "./statement-review";

const doc = (id: string, accountId: string | null, periodStart: string | null) =>
  ({
    id,
    accountId,
    periodStart,
    periodEnd: null,
    createdAt: "2026-01-01",
  }) as StatementDocument;

describe("statement review", () => {
  it("looks at documents account by account, oldest first, orphans last", () => {
    const docs = [
      doc("dec", "a", "2020-12-01"),
      doc("orphan", null, "2020-01-01"),
      doc("nov", "a", "2020-11-10"),
      doc("other", "b", "2020-01-01"),
    ];
    expect(docs.sort(logicalOrder).map((d) => d.id)).toEqual(["nov", "dec", "other", "orphan"]);
  });

  it("reads why a document waits, and tolerates garbage", () => {
    expect(parseReviewReason('{"doubts":2,"balanceGap":"100.58","warnings":0}')).toEqual({
      doubts: 2,
      balanceGap: "100.58",
      warnings: 0,
    });
    expect(parseReviewReason("not json")).toBeNull();
    expect(parseReviewReason(null)).toBeNull();
  });
});

const op = (patch: Partial<ReviewedOperation>): ReviewedOperation => ({
  row: 0,
  activityId: "a1",
  date: "2020-11-10",
  label: "MR FERRAND JEREMIE",
  amount: "3000.00",
  currency: "EUR",
  confidence: "low",
  needsReview: true,
  suggestedTaxonomyId: "income_sources",
  suggestedCategoryId: "cat_income_transfers_own",
  ...patch,
});

const review = (operations: ReviewedOperation[]): StatementReview =>
  ({ index: 0, currency: "EUR", operations }) as StatementReview;

describe("resolving the doubts of a review", () => {
  it("offers account to account in the right direction", () => {
    expect(ownTransferChoice("-10.00")).toEqual({
      taxonomyId: "spending_categories",
      categoryId: "cat_transfers_own",
    });
    expect(ownTransferChoice("2200.00")).toEqual({
      taxonomyId: "income_sources",
      categoryId: "cat_income_transfers_own",
    });
  });

  it("prefers the reader's suggestion to the stored category, and falls back to the stored one", () => {
    expect(suggestedChoice(op({}))).toEqual({
      taxonomyId: "income_sources",
      categoryId: "cat_income_transfers_own",
    });
    // an old catch-all on the activity must not win over the precise suggestion
    expect(
      suggestedChoice(op({ taxonomyId: "income_sources", categoryId: "cat_income_other" })),
    ).toEqual({ taxonomyId: "income_sources", categoryId: "cat_income_transfers_own" });
    expect(
      suggestedChoice(
        op({
          suggestedTaxonomyId: null,
          suggestedCategoryId: null,
          taxonomyId: "income_sources",
          categoryId: "cat_income_prime",
        }),
      ),
    ).toEqual({ taxonomyId: "income_sources", categoryId: "cat_income_prime" });
    expect(
      suggestedChoice(op({ suggestedTaxonomyId: null, suggestedCategoryId: null })),
    ).toBeNull();
  });

  it("confirms every undecided doubt as suggested and leaves the rest alone", () => {
    const statements = [
      review([
        op({ activityId: "doubt-1" }),
        op({ activityId: "doubt-2", suggestedCategoryId: "cat_income_prime" }),
        op({ activityId: "sure", needsReview: false }),
        op({ activityId: "decided" }),
        op({ activityId: null }),
      ]),
    ];
    const decided = { decided: { taxonomyId: "income_sources", categoryId: "cat_income_gifts" } };
    expect(doubtfulOperations(statements, decided).map((o) => o.activityId)).toEqual([
      "doubt-1",
      "doubt-2",
    ]);
    const next = confirmDoubts(statements, decided);
    expect(Object.keys(next).sort()).toEqual(["decided", "doubt-1", "doubt-2"]);
    expect(next.decided.categoryId).toBe("cat_income_gifts");
    expect(next["doubt-2"].categoryId).toBe("cat_income_prime");
  });
});
