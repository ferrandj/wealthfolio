/** Postal addresses kept as one text ("street\npostal code city") and edited as street, postal code and city. Mirrors
 * `crates/core/src/address.rs`: the stored text is the same one the statements print, so nothing is migrated. */
export interface AddressParts {
  street: string;
  postalCode: string;
  city: string;
}

const isPostalCode = (token: string) => /^\d{4,6}$/.test(token);

export function splitAddress(address: string | null | undefined): AddressParts {
  const lines = (address ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  const last = lines.pop();
  if (!last) return { street: "", postalCode: "", city: "" };
  const tokens = last.split(/\s+/);
  const at = tokens.findIndex((t, i) => isPostalCode(t) && i + 1 < tokens.length);
  if (at < 0) return { street: [...lines, last].join(", "), postalCode: "", city: "" };
  const street = [...lines, ...(at > 0 ? [tokens.slice(0, at).join(" ")] : [])];
  return {
    street: street.join(", "),
    postalCode: tokens[at] ?? "",
    city: tokens.slice(at + 1).join(" "),
  };
}

/** The stored text for the parts; `null` when there is nothing. */
export function joinAddress(parts: AddressParts): string | null {
  const town = [parts.postalCode, parts.city]
    .map((p) => p.trim())
    .filter(Boolean)
    .join(" ");
  const lines = [parts.street.trim(), town].filter(Boolean);
  return lines.length ? lines.join("\n") : null;
}
