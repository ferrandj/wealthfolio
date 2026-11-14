import { describe, expect, it } from "vitest";
import {
  guessInstitution,
  ibanTaken,
  isCurrentAccount,
  printedNameOwner,
} from "./account-holders-utils";

describe("guessInstitution", () => {
  const identity = { accountId: "a", editedByUser: false, updatedAt: "" };

  it("keeps the bank the documents recorded", () => {
    expect(guessInstitution({ ...identity, institution: "BoursoBank" }, "a", "x", [])).toBe(
      "BoursoBank",
    );
  });

  it("takes the bank of a document of the account", () => {
    const docs = [
      { accountId: "b", institution: "LCL" },
      { accountId: "a", institution: "Fortuneo" },
    ];
    expect(guessInstitution(identity, "a", "Assurance-vie", docs)).toBe("Fortuneo");
  });

  it("falls back on the name given at import", () => {
    expect(guessInstitution(undefined, "a", "Fortuneo - Assurance-vie ••7101", [])).toBe(
      "Fortuneo",
    );
    expect(guessInstitution(undefined, "a", "Compte courant", [])).toBe("");
  });
});

describe("isCurrentAccount", () => {
  it("follows the kind read on the documents", () => {
    expect(isCurrentAccount("a", "CASH", "x", [{ accountId: "a", accountType: "savings" }])).toBe(
      false,
    );
    expect(isCurrentAccount("a", "CASH", "x", [{ accountId: "a", accountType: "checking" }])).toBe(
      true,
    );
  });

  it("without documents, a cash account that is not a savings product", () => {
    expect(isCurrentAccount("a", "CASH", "Fortuneo - Livret ••4201", [])).toBe(false);
    expect(isCurrentAccount("a", "CASH", "Fortuneo - Compte courant ••4122", [])).toBe(true);
    expect(isCurrentAccount("a", "SECURITIES", "Assurance-vie", [])).toBe(false);
  });
});

describe("ibanTaken", () => {
  it("sees an IBAN another contact already carries", () => {
    const contacts = [{ identifiers: [{ kind: "iban", value: "FR76" }] }] as never;
    expect(ibanTaken(contacts, "FR76")).toBe(true);
    expect(ibanTaken(contacts, "FR77")).toBe(false);
  });
});

describe("printedNameOwner", () => {
  it("finds the contact that already bears a printed name", () => {
    const contacts = [
      {
        contact: { id: "me" },
        identifiers: [{ id: "i1", kind: "name", value: "chaco karla nnajera", label: null }],
      },
    ] as never;
    expect(printedNameOwner(contacts, "MME CHACO NNAJERA KARLA")).toEqual({
      id: "i1",
      contactId: "me",
    });
    expect(printedNameOwner(contacts, "M FERRAND JEREMIE")).toBeNull();
  });
});
