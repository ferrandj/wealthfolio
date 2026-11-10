/** Masks for personal data (IBAN, names, addresses) while the privacy mode hides the balances. */

const DOT = "•";

/** "FR7630006000011234567890189" → "FR•• •••• •••• •••• •••• 189" : country and last three characters stay. */
export function maskIban(value: string): string {
  const compact = value.replace(/\s+/g, "");
  if (compact.length <= 6) return DOT.repeat(compact.length);
  const hidden = compact.slice(2, -3).replace(/./g, DOT);
  return `${compact.slice(0, 2)}${hidden}${compact.slice(-3)}`.replace(/(.{4})(?=.)/g, "$1 ");
}

/** Keeps the first letter of each word: "Jean Dupont" → "J••• D•••••". */
export function maskText(value: string): string {
  return value.replace(
    /(\p{L})(\p{L}+)/gu,
    (_, first: string, rest: string) => first + DOT.repeat(rest.length),
  );
}

/** Address lines are hidden entirely: street names and numbers identify a person at least as well as a name. */
export function maskAddress(value: string): string {
  return value.replace(/[^\s\n]/g, DOT);
}
