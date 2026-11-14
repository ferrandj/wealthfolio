import { describe, expect, it } from "vitest";
import { defaultExclusions, manualAssets } from "./bulk-quote-mode";

const asset = (id: string, name: string, quoteMode = "MANUAL", kind = "INVESTMENT") =>
  ({ id, name, displayCode: id, quoteMode, kind }) as never;

describe("bulk automatic prices", () => {
  it("offers the securities priced by hand", () => {
    const list = [
      asset("a", "VARENNE VALEUR"),
      asset("b", "APPLE", "MARKET"),
      asset("c", "Maison", "MANUAL", "PROPERTY"),
    ];
    expect(manualAssets(list).map((a: { id: string }) => a.id)).toEqual(["a"]);
  });

  it("leaves out a euro fund by default", () => {
    const list = [asset("sur", "FDS EUROS SURAVENIR RENDEMENT"), asset("v", "VARENNE VALEUR")];
    expect(defaultExclusions(list)).toEqual(["sur"]);
  });
});
