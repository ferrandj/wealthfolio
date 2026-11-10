import { describe, expect, it } from "vitest";
import {
  ACCOUNT_AUTO,
  ACCOUNT_NEW,
  base64ToBytes,
  fileToBase64,
  formatBytes,
  overrideFor,
} from "./statement-files";

describe("statement files", () => {
  it("round-trips the bytes of a file through base64", async () => {
    const bytes = new Uint8Array([0, 1, 2, 250, 255]);
    const base64 = await fileToBase64(new Blob([bytes]));
    expect(Array.from(base64ToBytes(base64))).toEqual(Array.from(bytes));
  });

  it("sends no override when the account is left automatic", () => {
    expect(overrideFor(0, { account: ACCOUNT_AUTO, newAccountName: "", skip: false })).toBeNull();
  });

  it("sends the chosen account, a new name, or a skip", () => {
    expect(overrideFor(1, { account: "acc-1", newAccountName: "", skip: false })).toEqual({
      index: 1,
      accountId: "acc-1",
    });
    expect(
      overrideFor(0, { account: ACCOUNT_NEW, newAccountName: " Joint ", skip: false }),
    ).toEqual({ index: 0, newAccountName: "Joint" });
    expect(overrideFor(2, { account: "acc-1", newAccountName: "", skip: true })).toEqual({
      index: 2,
      skip: true,
    });
  });

  it("formats sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
  });
});
