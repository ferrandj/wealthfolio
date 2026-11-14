import { describe, expect, it } from "vitest";
import { buildTodo } from "./todo-model";

describe("buildTodo", () => {
  it("is empty when nothing waits", () => {
    expect(buildTodo({}).total).toBe(0);
  });

  it("gathers what is missing in each section", () => {
    const todo = buildTodo({
      documents: [
        { id: "d1", status: "PENDING" },
        { id: "d2", status: "VALIDATED" },
      ] as never,
      operations: [{ why: ["to_categorize"] }, { why: ["doubt", "loan_to_attach"] }] as never,
      loans: [
        {
          key: "l1",
          gaps: [
            { code: "terms", blocking: false },
            { code: "no_account", blocking: true },
          ],
        },
        { key: "l2", gaps: [], liabilityPending: false },
        { key: "l3", gaps: [], liabilityPending: true },
      ] as never,
      accounts: [
        { id: "a1", isActive: true, isArchived: false },
        { id: "a2", isActive: true, isArchived: false },
        { id: "a3", isActive: false, isArchived: true },
      ] as never,
      contacts: [{ linkedAccountIds: ["a1"] }] as never,
      coverage: [
        {
          accountId: "a1",
          missingMonths: [{ month: "2026-03", normal: false }],
          balanceBreaks: [],
        },
        { accountId: "a2", missingMonths: [{ month: "2026-04", normal: true }], balanceBreaks: [] },
      ] as never,
      counterparties: [{}, {}] as never,
      pendingActivities: 3,
    });
    expect(todo.documents.map((d) => d.id)).toEqual(["d1"]);
    expect(todo.operations).toEqual({
      to_categorize: 1,
      doubt: 1,
      loan_to_attach: 1,
      loan_proposed: 0,
    });
    expect(todo.loans.map((l) => l.loan.key)).toEqual(["l1", "l3"]);
    expect(todo.loans[0].gaps[0].code).toBe("no_account");
    expect(todo.ownerlessAccounts.map((a) => a.id)).toEqual(["a2"]);
    expect(todo.statements).toEqual([
      { accountId: "a1", missingMonths: ["2026-03"], balanceBreaks: 0 },
    ]);
    expect(todo.total).toBe(1 + 2 + 2 + 1 + 1 + 2 + 3);
  });
});
