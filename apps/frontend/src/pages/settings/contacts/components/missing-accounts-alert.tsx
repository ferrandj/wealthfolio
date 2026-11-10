import { listMissingOwnAccounts } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import { Button, Card, CardContent, Icons } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toNumber } from "../contacts-utils";

const eur = (value: string | number) => {
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency: "EUR" }).format(
      toNumber(value),
    );
  } catch {
    return String(toNumber(value));
  }
};

/**
 * Transfers go to (or come from) an account of the person that Wealthfolio does not have. They are counted as
 * transfers, but the missing account is said clearly: it must be created or imported.
 */
export function MissingAccountsAlert() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { isBalanceHidden } = useBalancePrivacy();
  const { data } = useQuery({
    queryKey: [QueryKeys.MISSING_OWN_ACCOUNTS],
    queryFn: listMissingOwnAccounts,
  });
  const missing = data ?? [];
  if (missing.length === 0) return null;
  return (
    <Card className="border-warning/60">
      <CardContent className="space-y-3 p-4" role="alert">
        <div className="flex items-center gap-2 font-medium">
          <Icons.AlertTriangle className="text-warning size-5" />
          {t("settings:contacts.missing_title", { count: missing.length })}
        </div>
        <p className="text-muted-foreground text-sm">{t("settings:contacts.missing_hint")}</p>
        <ul className="space-y-1 text-sm">
          {missing.map((m) => (
            <li key={`${m.contactId}-${m.iban}`}>
              <span className="font-medium">
                {m.label ?? t("settings:contacts.missing_unnamed")}
              </span>{" "}
              <span className="text-muted-foreground font-mono text-xs">
                {isBalanceHidden && m.iban ? maskText(m.iban) : m.iban}
              </span>{" "}
              —{" "}
              {t("settings:contacts.missing_line", {
                count: m.count,
                out: eur(m.totalOut),
                in: eur(m.totalIn),
              })}
            </li>
          ))}
        </ul>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" onClick={() => navigate("/settings/accounts")}>
            {t("settings:contacts.missing_create")}
          </Button>
          <Button size="sm" variant="outline" onClick={() => navigate("/settings/documents")}>
            {t("settings:contacts.missing_import")}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
