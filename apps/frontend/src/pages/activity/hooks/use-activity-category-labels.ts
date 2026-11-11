import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import { useTaxonomy } from "@/hooks/use-taxonomies";
import { listCashActivities } from "@/features/spending/adapters/cash-activities";
import type { CashActivity } from "@/features/spending/types/cash-activity";
import { QueryKeys } from "@/lib/query-keys";
import { getCategoryLabel } from "@/lib/taxonomy-labels";

const SPENDING_TAXONOMY = "spending_categories";
const INCOME_TAXONOMY = "income_sources";

/**
 * Category label per cash activity of an account: the same categories the
 * document import, the spending screens and the manual entry form assign.
 * Activities without a category are absent from the map.
 */
export function useActivityCategoryLabels(options: { accountId?: string; enabled?: boolean } = {}) {
  const { accountId, enabled = true } = options;
  const { t } = useTranslation();
  const filter = accountId ? { accountIds: [accountId] } : undefined;
  const { data: cash } = useQuery<CashActivity[], Error>({
    queryKey: [QueryKeys.SPENDING_TRANSACTIONS, filter ?? null],
    queryFn: () => listCashActivities(filter),
    enabled,
  });
  const { data: spending } = useTaxonomy(SPENDING_TAXONOMY);
  const { data: income } = useTaxonomy(INCOME_TAXONOMY);

  return useMemo(() => {
    const names = new Map<string, string>();
    for (const taxonomy of [spending, income]) {
      for (const category of taxonomy?.categories ?? []) {
        names.set(
          `${category.taxonomyId}:${category.id}`,
          getCategoryLabel(t, category.taxonomyId, category.id, category.name),
        );
      }
    }
    const labels = new Map<string, string>();
    for (const activity of cash ?? []) {
      const assignment = activity.assignments.find(
        (a) => a.taxonomyId === SPENDING_TAXONOMY || a.taxonomyId === INCOME_TAXONOMY,
      );
      const label = assignment && names.get(`${assignment.taxonomyId}:${assignment.categoryId}`);
      if (label) labels.set(activity.id, label);
    }
    return labels;
  }, [cash, spending, income, t]);
}
