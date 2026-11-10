import { getPersonOverview } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import { Input, Skeleton } from "@wealthfolio/ui";
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

/** The money of one person: their part of every account they hold, balances at the latest statement, flows. */
export function PersonOverviewSection({ contactId }: { contactId: string }) {
  const { t } = useTranslation();
  const { isBalanceHidden } = useBalancePrivacy();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const { data, isLoading } = useQuery({
    queryKey: [QueryKeys.PERSON_OVERVIEW, contactId, from, to],
    queryFn: () => getPersonOverview(contactId, from || undefined, to || undefined),
  });
  const show = (value: string | number | null, currency: string) =>
    value === null
      ? "—"
      : isBalanceHidden
        ? maskText(money(value, currency))
        : money(value, currency);

  return (
    <section className="grid gap-3">
      <div>
        <h3 className="text-sm font-semibold">{t("settings:contacts.overview_title")}</h3>
        <p className="text-muted-foreground text-xs">{t("settings:contacts.overview_hint")}</p>
      </div>
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
      ) : !data || data.accounts.length === 0 ? (
        <p className="text-muted-foreground text-sm">{t("settings:contacts.overview_none")}</p>
      ) : (
        <>
          {data.unbalancedAccounts.length > 0 && (
            <p className="text-warning text-sm" role="alert">
              {t("settings:contacts.shares_unbalanced", {
                accounts: data.unbalancedAccounts.join(", "),
              })}
            </p>
          )}
          <div className="overflow-x-auto rounded-md border">
            <table className="w-full text-sm">
              <thead className="text-muted-foreground text-left text-xs">
                <tr>
                  <th className="px-3 py-2">{t("settings:contacts.overview_account")}</th>
                  <th className="px-3 py-2 text-right">{t("settings:contacts.overview_share")}</th>
                  <th className="px-3 py-2 text-right">
                    {t("settings:contacts.overview_balance")}
                  </th>
                  <th className="px-3 py-2 text-right">{t("settings:contacts.overview_in")}</th>
                  <th className="px-3 py-2 text-right">{t("settings:contacts.overview_out")}</th>
                </tr>
              </thead>
              <tbody>
                {data.accounts.map((a) => (
                  <tr key={a.accountId} className="border-t">
                    <td className="px-3 py-2">
                      <div>{a.name}</div>
                      {a.joint && (
                        <div className="text-muted-foreground text-xs">
                          {t("settings:contacts.overview_joint", {
                            others: a.coHolders
                              .map((c) => `${c.name} ${toNumber(c.sharePercent)} %`)
                              .join(", "),
                          })}
                        </div>
                      )}
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums">
                      {toNumber(a.sharePercent)} %
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums">
                      {show(a.balance, a.currency)}
                      {a.balanceDate && (
                        <div className="text-muted-foreground text-xs">{a.balanceDate}</div>
                      )}
                    </td>
                    <td className="text-success px-3 py-2 text-right tabular-nums">
                      {show(a.income, a.currency)}
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums">
                      {show(a.spending, a.currency)}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                {data.totals.map((tot) => (
                  <tr key={tot.currency} className="border-t font-medium">
                    <td className="px-3 py-2">
                      {t("settings:contacts.overview_total", { currency: tot.currency })}
                    </td>
                    <td />
                    <td className="px-3 py-2 text-right tabular-nums">
                      {show(tot.balance, tot.currency)}
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums">
                      {show(tot.income, tot.currency)}
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums">
                      {show(tot.spending, tot.currency)}
                    </td>
                  </tr>
                ))}
              </tfoot>
            </table>
          </div>
        </>
      )}
    </section>
  );
}
