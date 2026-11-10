import { describe, expect, it } from "vitest";
import type { StatementDocument } from "@/lib/types/statements";
import { logicalOrder, parseReviewReason } from "./statement-review";

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
