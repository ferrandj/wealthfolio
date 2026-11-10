import { describe, expect, it } from "vitest";
import { maskAddress, maskIban, maskText } from "./privacy-mask";

describe("privacy masks", () => {
  it("keeps the country and the last characters of an IBAN", () => {
    const masked = maskIban("FR76 3000 6000 0112 3456 7890 189");
    expect(masked.startsWith("FR••")).toBe(true);
    expect(masked.endsWith("189")).toBe(true);
    expect(masked).not.toContain("3000");
  });

  it("keeps the first letter of each word of a name", () => {
    expect(maskText("Jean Dupont")).toBe("J••• D•••••");
  });

  it("hides an address and keeps its line breaks", () => {
    expect(maskAddress("1 RUE DU TEST\n60200 COMPIEGNE")).toBe("• ••• •• ••••\n••••• •••••••••");
  });
});
