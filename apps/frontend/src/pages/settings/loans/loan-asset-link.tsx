import { getAlternativeHoldings, linkStatementLoanAsset } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { AlternativeAssetHolding } from "@/lib/types";
import type { LoanAssetLink, LoanView, NewLoanAsset } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Button,
  Icons,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { addressLines, attachableAssets, newAssetOf, type NewAssetForm } from "./loans-utils";

const NEW_ASSET = "__new__";
const KINDS: NewLoanAsset["kind"][] = ["PROPERTY", "VEHICLE", "OTHER"];

const EMPTY_FORM: NewAssetForm = {
  kind: "PROPERTY",
  name: "",
  street: "",
  postalCode: "",
  city: "",
  value: "",
};

function AssetSummary({ asset }: { asset: AlternativeAssetHolding }) {
  const lines = addressLines(asset.metadata?.address);
  return (
    <div className="flex items-start gap-2 text-sm">
      <Icons.Home className="text-muted-foreground mt-0.5 size-4 shrink-0" />
      <div>
        <div className="font-medium">{asset.name}</div>
        {lines.map((line) => (
          <div key={line} className="text-muted-foreground text-xs">
            {line}
          </div>
        ))}
      </div>
    </div>
  );
}

/** Attach a loan to what it finances (a house, a car…) or detach it. */
export function LoanAssetLinkSection({ loan }: { loan: LoanView }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [choice, setChoice] = useState<string>("");
  const [form, setForm] = useState<NewAssetForm>(EMPTY_FORM);
  const [editing, setEditing] = useState(false);

  const { data } = useQuery({
    queryKey: [QueryKeys.ALTERNATIVE_HOLDINGS],
    queryFn: getAlternativeHoldings,
  });
  const assets = attachableAssets(data ?? []);
  const linked = assets.find((a) => a.id === loan.linkedAssetId);

  const save = useMutation({
    mutationFn: (link: LoanAssetLink) => linkStatementLoanAsset(loan.key, link),
    onSuccess: () => {
      setEditing(false);
      setChoice("");
      setForm(EMPTY_FORM);
      queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_LOANS] });
      queryClient.invalidateQueries({
        queryKey: [QueryKeys.ALTERNATIVE_HOLDINGS],
      });
      queryClient.invalidateQueries({ queryKey: [QueryKeys.NET_WORTH] });
    },
    onError: (e) =>
      toast({
        title: t("settings:loans.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  if (!loan.liabilityId) return null;

  const items = [
    ...assets.map((a) => ({ value: a.id, label: a.name })),
    { value: NEW_ASSET, label: t("settings:loans.asset_new") },
  ];
  const newAsset = choice === NEW_ASSET ? newAssetOf(form) : null;
  const canSave = choice === NEW_ASSET ? newAsset !== null : choice !== "";
  const set = (patch: Partial<NewAssetForm>) => setForm((f) => ({ ...f, ...patch }));

  return (
    <div className="space-y-2">
      <div className="text-muted-foreground text-xs">{t("settings:loans.asset")}</div>
      {linked && !editing ? (
        <div className="flex flex-wrap items-start justify-between gap-2">
          <AssetSummary asset={linked} />
          <div className="flex gap-2">
            <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
              {t("settings:loans.asset_change")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={save.isPending}
              onClick={() => save.mutate({})}
            >
              {t("settings:loans.asset_detach")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <p className="text-muted-foreground text-xs">{t("settings:loans.asset_hint")}</p>
          <div className="grid max-w-md gap-1.5">
            <Select items={items} value={choice} onValueChange={(v) => setChoice(v ?? "")}>
              <SelectTrigger aria-label={t("settings:loans.asset")}>
                <SelectValue placeholder={t("settings:loans.asset_choose")} />
              </SelectTrigger>
              <SelectContent>
                {items.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {choice === NEW_ASSET && (
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="grid gap-1.5">
                <Label>{t("settings:loans.asset_kind")}</Label>
                <Select
                  items={KINDS.map((k) => ({
                    value: k,
                    label: t(`settings:loans.asset_kind_${k.toLowerCase()}`),
                  }))}
                  value={form.kind}
                  onValueChange={(v) => v && set({ kind: v as NewLoanAsset["kind"] })}
                >
                  <SelectTrigger aria-label={t("settings:loans.asset_kind")}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {KINDS.map((k) => (
                      <SelectItem key={k} value={k}>
                        {t(`settings:loans.asset_kind_${k.toLowerCase()}`)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-1.5">
                <Label>{t("settings:loans.asset_name")}</Label>
                <Input value={form.name} onChange={(e) => set({ name: e.target.value })} />
              </div>
              <div className="grid gap-1.5 sm:col-span-2">
                <Label>{t("settings:loans.asset_street")}</Label>
                <Input value={form.street} onChange={(e) => set({ street: e.target.value })} />
              </div>
              <div className="grid gap-1.5">
                <Label>{t("settings:loans.asset_postal_code")}</Label>
                <Input
                  value={form.postalCode}
                  onChange={(e) => set({ postalCode: e.target.value })}
                />
              </div>
              <div className="grid gap-1.5">
                <Label>{t("settings:loans.asset_city")}</Label>
                <Input value={form.city} onChange={(e) => set({ city: e.target.value })} />
              </div>
              <div className="grid gap-1.5">
                <Label>{t("settings:loans.asset_value")}</Label>
                <Input
                  inputMode="decimal"
                  value={form.value}
                  onChange={(e) => set({ value: e.target.value })}
                />
              </div>
            </div>
          )}
          <div className="flex gap-2">
            <Button
              size="sm"
              disabled={!canSave || save.isPending}
              onClick={() => save.mutate(choice === NEW_ASSET ? { newAsset } : { assetId: choice })}
            >
              {t("settings:loans.asset_save")}
            </Button>
            {editing && (
              <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
                {t("settings:loans.asset_cancel")}
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
