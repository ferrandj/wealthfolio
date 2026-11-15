import { describe, expect, it } from "vitest";

import { savedPercent } from "./document-storage-line";

describe("savedPercent", () => {
  it("says how much smaller the kept documents are once compressed", () => {
    expect(savedPercent(1000, 590)).toBe(41);
    expect(savedPercent(8_888_568, 5_244_255)).toBe(41);
  });

  it("says nothing was saved when nothing is kept or nothing shrank", () => {
    expect(savedPercent(0, 0)).toBe(0);
    expect(savedPercent(100, 100)).toBe(0);
    expect(savedPercent(100, 104)).toBe(0);
  });
});
