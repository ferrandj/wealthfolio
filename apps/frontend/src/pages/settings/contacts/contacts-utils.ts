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
  street: string;
  postalCode: string;
  city: string;
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
    street: "",
    postalCode: "",
    city: "",
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
    street: contact.street ?? "",
    postalCode: contact.postalCode ?? "",
    city: contact.city ?? "",
    phone: contact.phone ?? "",
    email: contact.email ?? "",
    notes: contact.notes ?? "",
  };
}

/** Whether the card is about someone with names (a person, or the user's own other accounts). */
export function hasPersonNames(kind: ContactKind): boolean {
  return kind === "person" || kind === "self";
}

/** The given names are kept in one field, separated by spaces: the card shows up to three boxes for them. */
export const FIRST_NAME_BOXES = 3;

export function splitFirstNames(firstName: string): string[] {
  const parts = firstName.split(/\s+/).filter(Boolean);
  // more names than boxes: the last box holds the rest
  const boxes = parts.slice(0, FIRST_NAME_BOXES);
  if (parts.length > FIRST_NAME_BOXES)
    boxes[FIRST_NAME_BOXES - 1] = parts.slice(FIRST_NAME_BOXES - 1).join(" ");
  while (boxes.length < FIRST_NAME_BOXES) boxes.push("");
  return boxes;
}

export function joinFirstNames(boxes: string[]): string {
  return boxes
    .map((b) => b.trim())
    .filter(Boolean)
    .join(" ");
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
    street: text(form.street),
    postalCode: text(form.postalCode),
    city: text(form.city),
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

const CIVILITIES: Record<string, string> = {
  M: "M.",
  MR: "M.",
  MONSIEUR: "M.",
  MME: "Mme",
  MADAME: "Mme",
  MLE: "Mlle",
  MLLE: "Mlle",
  MADEMOISELLE: "Mlle",
};

const titleCase = (word: string) =>
  word
    .toLowerCase()
    .replace(/(^|[-'’])(\p{L})/gu, (_, sep: string, letter: string) => sep + letter.toUpperCase());

/** Words of a printed name, without the civility, accents or punctuation, for comparing. */
function nameTokens(text: string): string[] {
  return fold(text)
    .replace(/[^a-z0-9]+/g, " ")
    .split(" ")
    .filter((w) => w.length > 1 && !(w.toUpperCase() in CIVILITIES));
}

export interface PrintedName {
  civility: string;
  lastName: string;
  firstName: string;
}

/**
 * What a bank prints for a person ("M FERRAND JEREMIE", "MLE FERRAND AMANDINE"): civility, then the family name,
 * then the given names. A guess to prefill a card, the person corrects it.
 */
export function parsePrintedName(text: string): PrintedName {
  const words = text
    .replace(/[.,;—–]/g, " ")
    .split(/\s+/)
    .filter(Boolean);
  let civility = "";
  const first = words[0]?.toUpperCase();
  if (first && first in CIVILITIES) {
    civility = CIVILITIES[first];
    words.shift();
  }
  if (words.length === 0) return { civility, lastName: "", firstName: "" };
  if (words.length === 1) return { civility, lastName: titleCase(words[0]), firstName: "" };
  return {
    civility,
    lastName: titleCase(words[0]),
    firstName: words.slice(1).map(titleCase).join(" "),
  };
}

/**
 * The contact whose names look like a printed name: the same words in any order, or all the words of the shorter
 * name found in the other. Used to propose the owner of the next account to identify.
 */
export function suggestContact(list: ContactSummary[], printedName: string): string | null {
  const wanted = new Set(nameTokens(printedName));
  if (wanted.size === 0) return null;
  let best: { id: string; score: number } | null = null;
  for (const { contact, identifiers } of list) {
    const sources = [
      contact.displayName,
      [contact.firstName, contact.lastName].filter(Boolean).join(" "),
      [contact.firstName, contact.birthName].filter(Boolean).join(" "),
      ...identifiers.filter((i) => i.kind === "name").flatMap((i) => [i.label ?? "", i.value]),
    ];
    for (const source of sources) {
      const have = new Set(nameTokens(source));
      if (have.size === 0) continue;
      const common = [...wanted].filter((w) => have.has(w)).length;
      const smaller = Math.min(wanted.size, have.size);
      // all the words of the shorter name are in the other, and a lone word only matches a lone word
      if (common < smaller || (smaller < 2 && wanted.size !== have.size)) continue;
      const score = common / Math.max(wanted.size, have.size);
      if (!best || score > best.score) best = { id: contact.id, score };
    }
  }
  return best?.id ?? null;
}

/** Whether two printed names are the same words in any order (accents, case and civility aside). */
export function sameName(a: string, b: string): boolean {
  const x = nameTokens(a);
  const y = new Set(nameTokens(b));
  return x.length > 0 && x.length === y.size && x.every((w) => y.has(w));
}
