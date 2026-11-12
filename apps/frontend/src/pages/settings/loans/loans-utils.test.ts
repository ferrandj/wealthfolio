import { describe, expect, it } from "vitest";
import type { AlternativeAssetHolding } from "@/lib/types";
import type { LoanView } from "@/lib/types/statements";
import {
  addressLines,
  attachableAssets,
  day,
  needsFirstDue,
  newAssetOf,
  problemCount,
} from "./loans-utils";

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
    const loan = {
      plan: { rows: [row], checks: [], summary: null },
    } as unknown as LoanView;
    expect(needsFirstDue(loan)).toBe(true);
    const dated = {
      plan: {
        rows: [{ ...row, date: "2026-01-15" }],
        checks: [],
        summary: null,
      },
    } as unknown as LoanView;
    expect(needsFirstDue(dated)).toBe(false);
  });
});

describe("financed asset", () => {
  const holding = (id: string, kind: string) =>
    ({ id, kind, name: id }) as unknown as AlternativeAssetHolding;

  it("offers properties, vehicles and other assets, never another liability", () => {
    const ids = attachableAssets([
      holding("a", "PROPERTY"),
      holding("b", "LIABILITY"),
      holding("c", "vehicle"),
      holding("d", "COLLECTIBLE"),
      holding("e", "OTHER"),
    ]).map((h) => h.id);
    expect(ids).toEqual(["a", "c", "e"]);
  });

  it("cuts a stored address into lines", () => {
    expect(addressLines("190 rue Curie\n60280 Margny")).toEqual(["190 rue Curie", "60280 Margny"]);
    expect(addressLines(undefined)).toEqual([]);
  });

  it("builds the new asset only with a name and a number", () => {
    const form = {
      kind: "PROPERTY" as const,
      name: " Maison ",
      street: "1 rue des Lilas",
      postalCode: "75011",
      city: "Paris",
      value: "210 000,50",
    };
    expect(newAssetOf(form)).toEqual({
      kind: "PROPERTY",
      name: "Maison",
      street: "1 rue des Lilas",
      postalCode: "75011",
      city: "Paris",
      value: 210000.5,
    });
    expect(newAssetOf({ ...form, name: " " })).toBeNull();
    expect(newAssetOf({ ...form, value: "abc" })).toBeNull();
    expect(newAssetOf({ ...form, street: "", city: "", postalCode: "" })).toMatchObject({
      street: null,
    });
  });
});
