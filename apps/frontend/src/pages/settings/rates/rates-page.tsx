import { deleteRate, listRates, saveRate } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { RateInput, RateView } from "@/lib/types/rates";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Card, CardContent, Icons, Input, Separator, Skeleton } from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsHeader } from "../settings-header";

const num = (v: number | string) => Number(v);
const pct = (v: number | string) =>
  `${num(v).toLocaleString(undefined, { maximumFractionDigits: 4 })} %`;

function RateField({
  rate,
  onSave,
  disabled,
}: {
  rate: number | string;
  onSave: (value: number) => void;
  disabled?: boolean;
}) {
  const [text, setText] = useState(String(num(rate)));
  useEffect(() => setText(String(num(rate))), [rate]);
  const commit = () => {
    const value = Number(text.replace(",", "."));
    if (Number.isFinite(value) && value !== num(rate)) onSave(value);
    else setText(String(num(rate)));
  };
  return (
    <div className="flex items-center gap-1">
      <Input
        className="w-24 text-right"
        inputMode="decimal"
        value={text}
        disabled={disabled}
        aria-label="%"
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      <span className="text-muted-foreground text-sm">%</span>
    </div>
  );
}

export default function RatesPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false });
  const { data, isLoading } = useQuery({ queryKey: [QueryKeys.RATES], queryFn: listRates });

  const refresh = () => queryClient.invalidateQueries({ queryKey: [QueryKeys.RATES] });
  const onError = (e: unknown) =>
    toast({ title: t("settings:rates.error"), description: String(e), variant: "destructive" });
  const save = useMutation({
    mutationFn: (v: { id: string; input: RateInput }) => saveRate(v.id, v.input),
    onSuccess: refresh,
    onError,
  });
  const remove = useMutation({ mutationFn: deleteRate, onSuccess: refresh, onError });

  const rates = data ?? [];
  const regulated = rates.filter((r) => r.kind === "regulated");
  const tax = rates.filter((r) => r.kind === "tax");
  const byAccount = new Map(rates.filter((r) => r.accountId).map((r) => [r.accountId!, r]));

  const row = (r: RateView) => (
    <div key={r.id} className="flex flex-wrap items-center gap-3 py-2">
      <div className="min-w-0 flex-1">
        <div className="font-medium">{r.label}</div>
        {r.source && <div className="text-muted-foreground text-xs">{r.source}</div>}
      </div>
      <RateField rate={r.rate} onSave={(rate) => save.mutate({ id: r.id, input: { rate } })} />
    </div>
  );

  return (
    <div className="space-y-6">
      <SettingsHeader heading={t("settings:rates.title")} text={t("settings:rates.subtitle")} />
      <Separator />
      {isLoading ? (
        <Skeleton className="h-40 w-full" />
      ) : (
        <>
          <section className="space-y-2">
            <h2 className="text-lg font-semibold">{t("settings:rates.regulated_title")}</h2>
            <p className="text-muted-foreground text-sm">{t("settings:rates.regulated_hint")}</p>
            <Card>
              <CardContent className="divide-y p-4">{regulated.map(row)}</CardContent>
            </Card>
          </section>

          <section className="space-y-2">
            <h2 className="text-lg font-semibold">{t("settings:rates.tax_title")}</h2>
            <p className="text-muted-foreground text-sm">{t("settings:rates.tax_hint")}</p>
            <Card>
              <CardContent className="divide-y p-4">{tax.map(row)}</CardContent>
            </Card>
          </section>

          <section className="space-y-2">
            <h2 className="text-lg font-semibold">{t("settings:rates.accounts_title")}</h2>
            <p className="text-muted-foreground text-sm">{t("settings:rates.accounts_hint")}</p>
            <Card>
              <CardContent className="divide-y p-4">
                {(accounts ?? []).map((a) => {
                  const r = byAccount.get(a.id);
                  const id = `account.${a.id}`;
                  return (
                    <div key={a.id} className="flex flex-wrap items-center gap-3 py-2">
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-medium">{a.name}</div>
                        {r && (
                          <div className="text-muted-foreground text-xs">
                            {t("settings:rates.gross_net", {
                              gross: pct(r.grossRate),
                              net: pct(r.netRate),
                            })}
                          </div>
                        )}
                      </div>
                      {r && (
                        <>
                          <label className="flex items-center gap-1 text-xs">
                            <input
                              type="checkbox"
                              checked={r.gross}
                              onChange={(e) =>
                                save.mutate({ id, input: { gross: e.target.checked } })
                              }
                            />
                            {t("settings:rates.entered_gross")}
                          </label>
                          <label className="flex items-center gap-1 text-xs">
                            <input
                              type="checkbox"
                              checked={r.taxExempt}
                              onChange={(e) =>
                                save.mutate({ id, input: { taxExempt: e.target.checked } })
                              }
                            />
                            {t("settings:rates.tax_exempt")}
                          </label>
                        </>
                      )}
                      <RateField
                        rate={r?.rate ?? 0}
                        onSave={(rate) => save.mutate({ id, input: { rate, label: a.name } })}
                      />
                      {r && (
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          aria-label={t("settings:rates.remove")}
                          onClick={() => remove.mutate(id)}
                        >
                          <Icons.Trash className="size-4" />
                        </Button>
                      )}
                    </div>
                  );
                })}
              </CardContent>
            </Card>
          </section>
        </>
      )}
    </div>
  );
}
