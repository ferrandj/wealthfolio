import type {
  Contact,
  ContactInput,
  ContactKind,
  ContactRole,
  ContactSummary,
} from "@/lib/types/contacts";

export const CONTACT_KINDS: ContactKind[] = ["person", "organization", "self", "other"];
export const CONTACT_ROLES: ContactRole[] = ["none", "beneficiary", "debtor", "both"];

/** The fields of the card as the form holds them: every one is a string. */
export interface ContactForm {
  kind: ContactKind;
  role: ContactRole;
  displayName: string;
  civility: string;
  lastName: string;
  firstName: string;
  birthName: string;
  address: string;
  phone: string;
  email: string;
  notes: string;
}

export function emptyForm(kind: ContactKind = "person"): ContactForm {
  return {
    kind,
    role: "none",
    displayName: "",
    civility: "",
    lastName: "",
    firstName: "",
    birthName: "",
    address: "",
    phone: "",
    email: "",
    notes: "",
  };
}

export function contactToForm(contact: Contact): ContactForm {
  return {
    kind: contact.kind,
    role: contact.role,
    displayName: contact.displayName,
    civility: contact.civility ?? "",
    lastName: contact.lastName ?? "",
    firstName: contact.firstName ?? "",
    birthName: contact.birthName ?? "",
    address: contact.address ?? "",
    phone: contact.phone ?? "",
    email: contact.email ?? "",
    notes: contact.notes ?? "",
  };
}

/** Whether the card is about someone with names (a person, or the user's own other accounts). */
export function hasPersonNames(kind: ContactKind): boolean {
  return kind === "person" || kind === "self";
}

export function composeName(firstName: string, lastName: string): string {
  return [firstName.trim(), lastName.trim()].filter(Boolean).join(" ");
}

/** What the form sends: trimmed text, an empty field becoming nothing. A person's name is left to the names. */
export function formToInput(form: ContactForm): ContactInput {
  const text = (v: string) => (v.trim() === "" ? null : v.trim());
  const named = hasPersonNames(form.kind);
  return {
    kind: form.kind,
    role: form.role,
    displayName: named && form.displayName.trim() === "" ? null : text(form.displayName),
    civility: named ? text(form.civility) : null,
    lastName: named ? text(form.lastName) : null,
    firstName: named ? text(form.firstName) : null,
    birthName: named ? text(form.birthName) : null,
    address: text(form.address),
    phone: text(form.phone),
    email: text(form.email),
    notes: text(form.notes),
  };
}

/** A card can be saved once it has a name, typed or composed from the names. */
export function isFormComplete(form: ContactForm): boolean {
  if (form.displayName.trim() !== "") return true;
  return hasPersonNames(form.kind) && composeName(form.firstName, form.lastName) !== "";
}

/** An IBAN in groups of four, as it is read out. */
export function formatIban(value: string): string {
  const compact = value.replace(/\s+/g, "").toUpperCase();
  return compact.replace(/(.{4})/g, "$1 ").trim();
}

const fold = (text: string) => text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

export interface ContactFilter {
  query: string;
  kind: ContactKind | "all";
  role: ContactRole | "all";
}

export function filterContacts(list: ContactSummary[], filter: ContactFilter): ContactSummary[] {
  const needle = fold(filter.query.trim());
  return list.filter(({ contact, identifiers }) => {
    if (filter.kind !== "all" && contact.kind !== filter.kind) return false;
    if (filter.role !== "all") {
      const role = contact.role;
      const matches =
        role === filter.role ||
        (role === "both" && (filter.role === "beneficiary" || filter.role === "debtor"));
      if (!matches) return false;
    }
    if (!needle) return true;
    const haystack = [
      contact.displayName,
      contact.firstName,
      contact.lastName,
      contact.birthName,
      contact.email,
      contact.phone,
      ...identifiers.flatMap((i) => [i.label, i.value]),
    ]
      .filter(Boolean)
      .map((v) => fold(String(v)))
      .join(" ");
    return needle.split(/\s+/).every((word) => haystack.includes(word));
  });
}

export function toNumber(value: string | number | null | undefined): number {
  const n = Number(value ?? 0);
  return Number.isFinite(n) ? n : 0;
}
