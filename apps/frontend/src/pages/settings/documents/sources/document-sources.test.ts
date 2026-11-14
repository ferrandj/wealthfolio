import { describe, expect, it } from "vitest";
import { dayAfter, dueSources, sourcesToDescribe } from "./document-sources";

describe("document sources", () => {
  it("asks for the due ones and describes the new ones", () => {
    const sources = [
      { key: "a", due: true, needsSetup: false },
      { key: "b", due: false, needsSetup: true },
    ] as never;
    expect(dueSources(sources).map((s: { key: string }) => s.key)).toEqual(["a"]);
    expect(sourcesToDescribe(sources).map((s: { key: string }) => s.key)).toEqual(["b"]);
  });

  it("puts off by whole days", () => {
    expect(dayAfter(3, new Date("2026-11-14T10:00:00Z"))).toBe("2026-11-17");
  });
});
