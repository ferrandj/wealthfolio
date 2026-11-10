import { getSpendingByBeneficiary } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import { Button, Card, CardContent, Input, Skeleton } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toNumber } from "../contacts-utils";

const money = (value: string | number, currency: string) => {
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(
      toNumber(value),
    );
  } catch {
    return `${toNumber(value).toFixed(2)} ${currency}`;
  }
};

interface Props {
  onIdentify: (name: string) => void;
}

/** Who the money went to over a period, biggest first. Transfers between own accounts are left out. */
export function SpendingByBeneficiary({ onIdentify }: Props) {
  const { t } = useTranslation();
  const { isBalanceHidden } = useBalancePrivacy();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const { data, isLoading } = useQuery({
    queryKey: [QueryKeys.BENEFICIARY_SPENDING, from, to],
    queryFn: () => getSpendingByBeneficiary(from || undefined, to || undefined),
  });
  const rows = (data ?? []).filter((r) => toNumber(r.totalOut) > 0 || toNumber(r.totalIn) > 0);

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">{t("settings:contacts.spending_hint")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Input
          type="date"
          className="w-40"
          value={from}
          aria-label={t("settings:contacts.spending_from")}
          onChange={(e) => setFrom(e.target.value)}
        />
        <Input
          type="date"
          className="w-40"
          value={to}
          aria-label={t("settings:contacts.spending_to")}
          onChange={(e) => setTo(e.target.value)}
        />
      </div>
      {isLoading ? (
        <Skeleton className="h-24 w-full" />
      ) : rows.length === 0 ? (
        <p className="text-muted-foreground text-sm">{t("settings:contacts.spending_empty")}</p>
      ) : (
        rows.map((r) => (
          <Card key={`${r.contactId ?? r.name}-${r.currency}`}>
            <CardContent className="flex items-center gap-3 p-3">
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">
                  {isBalanceHidden ? maskText(r.name) : r.name}
                </div>
                <div className="text-muted-foreground text-xs">
                  {t("settings:contacts.spending_count", { count: r.count })} · {r.lastDate}
                </div>
              </div>
              <div className="text-right text-sm tabular-nums">
                <div>{money(r.totalOut, r.currency)}</div>
                {toNumber(r.totalIn) > 0 && (
                  <div className="text-muted-foreground text-xs">
                    +{money(r.totalIn, r.currency)}
                  </div>
                )}
              </div>
              {!r.contactId && (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => onIdentify(r.name)}
                >
                  {t("settings:contacts.spending_identify")}
                </Button>
              )}
            </CardContent>
          </Card>
        ))
      )}
    </div>
  );
}
