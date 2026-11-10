/**
 * A holder name printed as "M DUPONT JEAN ET MME DUPONT MARIE" (or "OU", "&") belongs to a joint account.
 * Banks truncate long names, so a name that ends on a conjunction is joint as well.
 */
export function isJointHolder(holderName: string | null | undefined): boolean {
  if (!holderName) return false;
  const name = ` ${holderName.toUpperCase().replace(/[.,]/g, " ").replace(/\s+/g, " ").trim()} `;
  if (/ (ET|OU|&) /.test(name)) return true;
  // two civilities in one name: "M DUPONT JEAN MME DUPONT MARIE"
  const civilities = name.match(/ (M|MR|MME|MLLE|MONSIEUR|MADAME) /g);
  return (civilities?.length ?? 0) >= 2;
}

export type HolderFilter = "all" | "joint" | "individual";

export function matchesHolderFilter(holderName: string | null | undefined, filter: HolderFilter) {
  if (filter === "all") return true;
  return filter === "joint" ? isJointHolder(holderName) : !isJointHolder(holderName);
}
