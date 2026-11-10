import { getAccountIdentity, saveAccountIdentity } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { AccountIdentity, AccountIdentityInput } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Card, CardContent, CardHeader, CardTitle, Icons, Input } from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { copyText } from "@wealthfolio/ui/lib/clipboard";
import { useState } from "react";
import { useTranslation } from "react-i18next";

export const IDENTITY_FIELDS = [
  "holderName",
  "holderAddress",
  "iban",
  "bic",
  "institution",
  "bankCode",
  "branchCode",
  "accountNumber",
  "ribKey",
] as const;
export type IdentityField = (typeof IDENTITY_FIELDS)[number];

/** IBANs read better in groups of four; the copied value stays as printed on the document. */
export function displayValue(field: IdentityField, value: string): string {
  return field === "iban" ? value.replace(/(.{4})(?=.)/g, "$1 ") : value;
}

export function AccountIdentityCard({ accountId }: { accountId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<AccountIdentityInput>({});
  const [copied, setCopied] = useState<string | null>(null);

  const { data: identity } = useQuery<AccountIdentity | null>({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES, accountId],
    queryFn: () => getAccountIdentity(accountId),
  });

  const save = useMutation({
    mutationFn: (input: AccountIdentityInput) => saveAccountIdentity(accountId, input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: [QueryKeys.ACCOUNT_IDENTITIES] });
      setEditing(false);
      toast({ title: t("settings:identity.saved") });
    },
    onError: (e) =>
      toast({
        title: t("settings:identity.save_error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const value = (field: IdentityField) => identity?.[field] ?? "";
  const filled = IDENTITY_FIELDS.filter((f) => value(f));
  const label = (field: IdentityField) => t(`settings:identity.fields.${field}`);

  const copy = async (key: string, text: string) => {
    try {
      await copyText(text);
      setCopied(key);
      setTimeout(() => setCopied((c) => (c === key ? null : c)), 1500);
    } catch {
      toast({ title: t("settings:identity.copy_error"), variant: "destructive" });
    }
  };

  const startEdit = () => {
    setDraft(Object.fromEntries(IDENTITY_FIELDS.map((f) => [f, value(f)])) as AccountIdentityInput);
    setEditing(true);
  };

  const commit = () => {
    // Only what changed goes out; an emptied field is sent as "" which clears it.
    const changes: AccountIdentityInput = {};
    for (const f of IDENTITY_FIELDS) {
      if ((draft[f] ?? "") !== value(f)) changes[f] = draft[f] ?? "";
    }
    if (Object.keys(changes).length === 0) setEditing(false);
    else save.mutate(changes);
  };

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle className="text-base">{t("settings:identity.title")}</CardTitle>
        <div className="flex gap-1">
          {!editing && filled.length > 0 && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() =>
                void copy("all", filled.map((f) => `${label(f)}: ${value(f)}`).join("\n"))
              }
            >
              {copied === "all" ? (
                <Icons.Check className="me-1 size-4" />
              ) : (
                <Icons.Copy className="me-1 size-4" />
              )}
              {copied === "all" ? t("settings:identity.copied") : t("settings:identity.copy_all")}
            </Button>
          )}
          {!editing && (
            <Button type="button" variant="ghost" size="sm" onClick={startEdit}>
              {t("settings:identity.edit")}
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent className="space-y-2">
        {editing ? (
          <>
            {IDENTITY_FIELDS.map((f) => (
              <label key={f} className="grid gap-1 text-sm">
                <span className="text-muted-foreground text-xs">{label(f)}</span>
                <Input
                  value={draft[f] ?? ""}
                  onChange={(e) => setDraft((d) => ({ ...d, [f]: e.target.value }))}
                />
              </label>
            ))}
            <p className="text-muted-foreground text-xs">{t("settings:identity.edited_note")}</p>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setEditing(false)}>
                {t("settings:identity.cancel")}
              </Button>
              <Button type="button" onClick={commit} disabled={save.isPending}>
                {t("settings:identity.save")}
              </Button>
            </div>
          </>
        ) : filled.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("settings:identity.empty")}</p>
        ) : (
          <dl className="divide-border/60 divide-y">
            {filled.map((f) => (
              <div key={f} className="flex items-start justify-between gap-3 py-1.5">
                <div className="min-w-0">
                  <dt className="text-muted-foreground text-xs">{label(f)}</dt>
                  <dd
                    className={`text-sm break-words whitespace-pre-line ${f === "iban" ? "font-mono" : ""}`}
                  >
                    {displayValue(f, value(f))}
                  </dd>
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="shrink-0"
                  aria-label={t("settings:identity.copy_field", { field: label(f) })}
                  onClick={() => void copy(f, value(f))}
                >
                  {copied === f ? (
                    <Icons.Check className="size-4" />
                  ) : (
                    <Icons.Copy className="size-4" />
                  )}
                </Button>
              </div>
            ))}
          </dl>
        )}
      </CardContent>
    </Card>
  );
}
