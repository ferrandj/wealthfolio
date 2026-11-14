import { describe, expect, it } from "vitest";
import {
  costAnswers,
  debitCandidates,
  detachedAnswers,
  loanCostOf,
  loanLineOf,
  matchedAnswers,
  matchWindow,
} from "./loan-match";
import { loanLinkValue } from "./loan-payment-picker";

const loan = {
  key: "L",
  answers: {
    confirmedPayments: [
      { date: "2025-04-15", activityId: "a1" },
      { date: "2025-05-15", activityId: "a2" },
    ],
    detachedActivities: ["a3"],
  },
  payments: { lines: [{ date: "2025-04-15", activityId: "a1", state: "found" }] },
} as never;

describe("loan matching", () => {
  it("searches the debits around the due date", () => {
    expect(matchWindow("2025-04-15")).toEqual({ dateFrom: "2025-03-31", dateTo: "2025-05-05" });
  });

  it("puts the same amount first and says when a debit pays another instalment", () => {
    const acts = [
      { id: "x", activityType: "WITHDRAWAL", amount: "-700", date: "2025-05-15" },
      { id: "a1", activityType: "WITHDRAWAL", amount: "-725.56", date: "2025-04-16" },
      { id: "dep", activityType: "DEPOSIT", amount: "725.56", date: "2025-05-15" },
    ] as never;
    const c = debitCandidates(acts, { date: "2025-05-15", expected: 725.56, activityId: null }, [
      { date: "2025-04-15", activityId: "a1" },
    ]);
    expect(c.map((x) => x.activity.id)).toEqual(["a1", "x"]);
    expect(c[0].sameAmount).toBe(true);
    expect(c[0].usedFor).toBe("2025-04-15");
  });

  it("pins a debit to one instalment only and detaches it from the loan", () => {
    const m = matchedAnswers(loan, "2025-05-15", "a3");
    expect(m.confirmedPayments).toEqual([
      { date: "2025-04-15", activityId: "a1" },
      { date: "2025-05-15", activityId: "a3" },
    ]);
    expect(m.detachedActivities).toEqual([]);
    const d = detachedAnswers(loan, "a1");
    expect(d.confirmedPayments).toEqual([{ date: "2025-05-15", activityId: "a2" }]);
    expect(d.detachedActivities).toEqual(["a3", "a1"]);
  });

  it("finds the instalment a debit pays", () => {
    expect(loanLineOf([loan], "a1")?.line.date).toBe("2025-04-15");
    expect(loanLineOf([loan], "zz")).toBeNull();
  });

  it("keeps a fee of the loan apart from the instalments", () => {
    const c = costAnswers(loan, "a1");
    expect(c.otherActivities).toEqual(["a1"]);
    expect(c.confirmedPayments).toEqual([{ date: "2025-05-15", activityId: "a2" }]);
    expect(loanCostOf([{ ...(loan as object), answers: c } as never], "a1")).not.toBeNull();
    expect(
      matchedAnswers({ ...(loan as object), answers: c } as never, "2025-04-15", "a1")
        .otherActivities,
    ).toEqual([]);
    expect(loanLinkValue("L", "cost")).toBe("cost:L");
    expect(loanLinkValue("L", "to_attach")).toBe("");
    expect(loanLinkValue("L", "matched")).toBe("L");
  });
});
