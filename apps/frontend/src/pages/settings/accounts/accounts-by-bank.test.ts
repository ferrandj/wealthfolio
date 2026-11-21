import { describe, expect, it } from "vitest";
import type { Account, Platform } from "@/lib/types";
import { groupByBank } from "./accounts-by-bank";

const account = (id: string, name: string, platformId?: string) =>
  ({ id, name, platformId }) as Account;

describe("groupByBank", () => {
  it("groups the accounts by the bank their name or broker gives", () => {
    const platforms = new Map<string, Platform>([
      ["p1", { id: "p1", name: "Trade Republic" } as Platform],
    ]);
    const groups = groupByBank(
      [
        account("a", "Fortuneo - Livret ••4201"),
        account("b", "BoursoBank - CEL ••3644"),
        account("c", "Mon épargne"),
        account("d", "Boursobank - PEL ••8738"),
        account("e", "Compte titres", "p1"),
      ],
      platforms,
      "Autres",
    );
    expect(groups.map((g) => [g.bank, g.accounts.map((a) => a.id)])).toEqual([
      ["BoursoBank", ["b", "d"]],
      ["Fortuneo", ["a"]],
      ["Trade Republic", ["e"]],
      ["Autres", ["c"]],
    ]);
  });
});
