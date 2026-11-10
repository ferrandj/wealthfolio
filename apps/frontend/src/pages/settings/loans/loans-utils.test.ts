import { describe, expect, it } from "vitest";
import type { LoanView } from "@/lib/types/statements";
import { day, needsFirstDue, problemCount } from "./loans-utils";

describe("loans utils", () => {
  it("writes a date the French way without a time zone shift", () => {
    expect(day("2026-06-15")).toBe("15/06/2026");
    expect(day(null)).toBe("—");
  });

  it("counts the checks that failed", () => {
    expect(
      problemCount([
        { status: "ok", text: "a" },
        { status: "warn", text: "b" },
        { status: "info", text: "c" },
      ]),
    ).toBe(1);
  });

  it("asks for the first due date only while some instalments have no date", () => {
    const row = {
      kind: "installment",
      date: null,
      number: 1,
      capital: 1,
      interest: 1,
      insurance: 0,
      total: 2,
      remaining: 0,
      source: "contrat",
      status: "future",
      note: "",
    } as const;
    const loan = { plan: { rows: [row], checks: [], summary: null } } as unknown as LoanView;
    expect(needsFirstDue(loan)).toBe(true);
    const dated = {
      plan: { rows: [{ ...row, date: "2026-01-15" }], checks: [], summary: null },
    } as unknown as LoanView;
    expect(needsFirstDue(dated)).toBe(false);
  });
});
