import { listUnresolvedCounterparties } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { maskIban, maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import type { UnresolvedCounterparty } from "@/lib/types/contacts";
import { useQuery } from "@tanstack/react-query";
import { Badge, Button, Card, CardContent, Icons, Skeleton, Switch } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formatIban, toNumber } from "../contacts-utils";
import { IdentifyWizard } from "./identify-wizard";

const amount = (value: string | number) =>
  new Intl.NumberFormat(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(
    toNumber(value),
  );

/** Who the imported operations were with, that no contact recognises yet: identified one account at a time. */
export function UnresolvedList() {
  const { t } = useTranslation();
  const { isBalanceHidden: hidden } = useBalancePrivacy();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const [shops, setShops] = useState(false);
  // the accounts being identified (one, or all of them one after the other)
  const [wizard, setWizard] = useState<UnresolvedCounterparty[] | null>(null);

  const list = useQuery({
    queryKey: [QueryKeys.UNRESOLVED_COUNTERPARTIES, shops],
    queryFn: () => listUnresolvedCounterparties(shops),
  });

  const accountName = (id: string) => accounts.find((a) => a.id === id)?.name ?? id;

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-muted-foreground max-w-2xl text-sm">
          {t("settings:contacts.unresolved_hint")}
        </p>
        <label className="flex items-center gap-2 text-sm">
          <Switch checked={shops} onCheckedChange={setShops} />
          {t("settings:contacts.unresolved_show_shops")}
        </label>
      </div>
      {(list.data ?? []).length > 1 && (
        <div>
          <Button variant="outline" size="sm" onClick={() => setWizard(list.data ?? [])}>
            {t("settings:contacts.wizard_identify_all", { count: (list.data ?? []).length })}
          </Button>
        </div>
      )}

      {list.isLoading ? (
        <Skeleton className="h-40 w-full" />
      ) : (list.data ?? []).length === 0 ? (
        <Card>
          <CardContent className="text-muted-foreground py-10 text-center text-sm">
            {t("settings:contacts.unresolved_empty")}
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3">
          {(list.data ?? []).map((item) => {
            const id = `${item.kind}:${item.key}`;
            return (
              <Card key={id}>
                <CardContent className="grid gap-3 py-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="grid min-w-0 gap-1">
                      <span className="font-medium">
                        {item.name
                          ? hidden
                            ? maskText(item.name)
                            : item.name
                          : hidden
                            ? maskIban(item.iban ?? "")
                            : formatIban(item.iban ?? "")}
                      </span>
                      {item.iban ? (
                        <code className="text-muted-foreground text-xs">
                          {hidden ? maskIban(item.iban) : formatIban(item.iban)}
                        </code>
                      ) : null}
                      <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">
                          {t("settings:contacts.operations_label", { count: item.count })}
                        </Badge>
                        {toNumber(item.totalOut) > 0 ? (
                          <span className="text-muted-foreground text-xs">
                            {t("settings:contacts.total_out")} {amount(item.totalOut)}
                          </span>
                        ) : null}
                        {toNumber(item.totalIn) > 0 ? (
                          <span className="text-success text-xs">
                            {t("settings:contacts.total_in")} {amount(item.totalIn)}
                          </span>
                        ) : null}
                        <span className="text-muted-foreground text-xs">
                          {t("settings:contacts.last_operation", { date: item.lastDate })}
                        </span>
                      </div>
                      {item.ownAccountId ? (
                        <Badge variant="outline" className="w-fit">
                          <Icons.CreditCard className="mr-1 size-3" />
                          {t("settings:contacts.unresolved_own_account", {
                            name: accountName(item.ownAccountId),
                          })}
                        </Badge>
                      ) : null}
                    </div>
                    <Button size="sm" onClick={() => setWizard([item])}>
                      <Icons.Plus className="mr-1 size-4" />
                      {t("settings:contacts.wizard_identify")}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
      {wizard ? <IdentifyWizard items={wizard} onClose={() => setWizard(null)} /> : null}
    </div>
  );
}
