import { describe, expect, it } from "vitest";
import type { Contact, ContactSummary } from "@/lib/types/contacts";
import {
  composeName,
  contactToForm,
  emptyForm,
  filterContacts,
  formatIban,
  formToInput,
  isFormComplete,
  joinFirstNames,
  parsePrintedName,
  splitFirstNames,
  suggestContact,
} from "./contacts-utils";

const contact = (over: Partial<Contact>): Contact => ({
  id: "c1",
  kind: "person",
  role: "none",
  displayName: "Jean Dupont",
  civility: null,
  lastName: "Dupont",
  firstName: "Jean",
  birthName: null,
  address: null,
  street: null,
  postalCode: null,
  city: null,
  phone: null,
  email: null,
  notes: null,
  createdAt: "2026-01-01",
  updatedAt: "2026-01-01",
  ...over,
});

const summary = (c: Contact, identifiers: ContactSummary["identifiers"] = []): ContactSummary => ({
  contact: c,
  identifiers,
  linkedAccountIds: [],
  accountShares: [],
  operationCount: 0,
  totalIn: "0",
  totalOut: "0",
  lastDate: null,
});

describe("contact form", () => {
  it("is complete with a name or with first and last names of a person", () => {
    expect(isFormComplete(emptyForm())).toBe(false);
    expect(isFormComplete({ ...emptyForm(), firstName: "Jean" })).toBe(true);
    expect(isFormComplete({ ...emptyForm("organization"), firstName: "Jean" })).toBe(false);
    expect(isFormComplete({ ...emptyForm("organization"), displayName: "EDF" })).toBe(true);
  });

  it("sends empty fields as nothing and leaves a person's name to the names", () => {
    const input = formToInput({
      ...emptyForm(),
      firstName: " Jean ",
      lastName: "DUPONT",
      phone: "  ",
      role: "beneficiary",
    });
    expect(input).toMatchObject({
      kind: "person",
      role: "beneficiary",
      displayName: null,
      firstName: "Jean",
      lastName: "DUPONT",
      phone: null,
    });
  });

  it("does not send names for an organisation", () => {
    const input = formToInput({ ...emptyForm("organization"), displayName: "EDF", firstName: "x" });
    expect(input.displayName).toBe("EDF");
    expect(input.firstName).toBeNull();
  });

  it("round-trips a card", () => {
    const form = contactToForm(contact({ phone: "0102030405", notes: "ami" }));
    expect(form.phone).toBe("0102030405");
    expect(formToInput(form).notes).toBe("ami");
  });

  it("composes a name and groups an IBAN by four", () => {
    expect(composeName(" Jean", "DUPONT ")).toBe("Jean DUPONT");
    expect(formatIban("fr7630006000011234567890189")).toBe("FR76 3000 6000 0112 3456 7890 189");
  });
});

describe("filterContacts", () => {
  const list = [
    summary(contact({}), [
      {
        id: "i1",
        contactId: "c1",
        kind: "iban",
        value: "FR7630006000011234567890189",
        label: null,
        bic: null,
        institution: null,
        createdAt: "",
      },
    ]),
    summary(
      contact({
        id: "c2",
        kind: "organization",
        role: "both",
        displayName: "Électricité EDF",
        firstName: null,
        lastName: null,
      }),
    ),
    summary(
      contact({
        id: "c3",
        kind: "organization",
        role: "debtor",
        displayName: "Mutuelle",
        firstName: null,
        lastName: null,
      }),
    ),
  ];
  const all = { query: "", kind: "all", role: "all" } as const;

  it("searches names without accents or case, and identifiers", () => {
    expect(filterContacts(list, { ...all, query: "electricite" })).toHaveLength(1);
    expect(filterContacts(list, { ...all, query: "dupont jean" })).toHaveLength(1);
    expect(filterContacts(list, { ...all, query: "FR7630006" })).toHaveLength(1);
    expect(filterContacts(list, { ...all, query: "zzz" })).toHaveLength(0);
  });

  it("filters by kind and by role, a contact that is both showing under each", () => {
    expect(filterContacts(list, { ...all, kind: "organization" })).toHaveLength(2);
    expect(filterContacts(list, { ...all, role: "beneficiary" }).map((s) => s.contact.id)).toEqual([
      "c2",
    ]);
    expect(filterContacts(list, { ...all, role: "debtor" }).map((s) => s.contact.id)).toEqual([
      "c2",
      "c3",
    ]);
  });
});

describe("names printed by a bank", () => {
  it("reads civility, family name and given names", () => {
    expect(parsePrintedName("M FERRAND JEREMIE")).toEqual({
      civility: "M.",
      lastName: "Ferrand",
      firstName: "Jeremie",
    });
    expect(parsePrintedName("MLE FERRAND AMANDINE")).toEqual({
      civility: "Mlle",
      lastName: "Ferrand",
      firstName: "Amandine",
    });
    expect(parsePrintedName("MME DUPONT-MARTIN ANNE SOPHIE").firstName).toBe("Anne Sophie");
    expect(parsePrintedName("Kraken")).toEqual({ civility: "", lastName: "Kraken", firstName: "" });
  });

  it("keeps up to three given names in three boxes", () => {
    expect(splitFirstNames("Anne Sophie")).toEqual(["Anne", "Sophie", ""]);
    expect(splitFirstNames("A B C D")).toEqual(["A", "B", "C D"]);
    expect(joinFirstNames(["Anne", "", " Sophie "])).toBe("Anne Sophie");
  });
});

describe("proposing the owner of an account", () => {
  const summary = (id: string, displayName: string, firstName: string, lastName: string) =>
    ({
      contact: { id, displayName, firstName, lastName, birthName: null },
      identifiers: [],
    }) as unknown as ContactSummary;
  const list = [
    summary("jeremie", "Jérémie Ferrand", "Jérémie", "Ferrand"),
    summary("amandine", "Amandine Ferrand", "Amandine", "Ferrand"),
  ];

  it("finds the same words in any order, accents and civility aside", () => {
    expect(suggestContact(list, "M FERRAND JEREMIE")).toBe("jeremie");
    expect(suggestContact(list, "MR Jérémie Ferrand")).toBe("jeremie");
    expect(suggestContact(list, "MLE FERRAND AMANDINE")).toBe("amandine");
  });

  it("does not guess from the family name alone", () => {
    expect(suggestContact(list, "FERRAND")).toBeNull();
    expect(suggestContact(list, "KRAKEN KRAKEN")).toBeNull();
  });
});
