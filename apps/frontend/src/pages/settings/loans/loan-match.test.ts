import { describe, expect, it } from "vitest";
import {
  debitCandidates,
  detachedAnswers,
  loanLineOf,
  matchedAnswers,
  matchWindow,
} from "./loan-match";

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
});
