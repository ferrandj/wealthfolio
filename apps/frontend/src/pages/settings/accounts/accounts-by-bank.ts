import type { Account, Platform } from "@/lib/types";

export interface BankGroup {
  bank: string;
  accounts: Account[];
}

/**
 * The bank of an account: its broker when it is connected to one, else what its name starts with ("BoursoBank -
 * CEL ••3644" → BoursoBank, the name the documents give a new account), else nothing.
 */
export function bankOf(account: Account, platforms: Map<string, Platform>): string | null {
  const platform = account.platformId ? platforms.get(account.platformId) : undefined;
  if (platform?.name) return platform.name;
  const dash = account.name.indexOf(" - ");
  if (dash > 0) return account.name.slice(0, dash).trim();
  return null;
}

/** The accounts by bank, banks in alphabetical order; accounts without a known bank last, under `others`. */
export function groupByBank(
  accounts: Account[],
  platforms: Map<string, Platform>,
  others: string,
): BankGroup[] {
  const groups = new Map<string, Account[]>();
  const rest: Account[] = [];
  for (const account of accounts) {
    const bank = bankOf(account, platforms);
    if (!bank) {
      rest.push(account);
      continue;
    }
    const key = [...groups.keys()].find((k) => k.toLowerCase() === bank.toLowerCase()) ?? bank;
    groups.set(key, [...(groups.get(key) ?? []), account]);
  }
  const sorted = [...groups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([bank, list]) => ({ bank, accounts: list }));
  if (rest.length > 0) sorted.push({ bank: others, accounts: rest });
  return sorted;
}
