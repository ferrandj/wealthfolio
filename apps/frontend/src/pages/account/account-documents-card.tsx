import { listStatementDocuments } from "@/adapters";
import { useDownloadDocument } from "@/pages/settings/documents/use-download-document";
import { formatBytes } from "@/pages/settings/documents/statement-files";
import { QueryKeys } from "@/lib/query-keys";
import type { StatementDocument } from "@/lib/types/statements";
import { useQuery } from "@tanstack/react-query";
import { Button, Card, CardContent, CardHeader, CardTitle, Icons } from "@wealthfolio/ui";
import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";

/** How many documents the card shows before the rest is folded. */
const SHOWN = 3;

/**
 * The documents imported into an account, newest period first, each one downloadable as it was dropped. Only the
 * latest few are shown; the others unfold on demand, and the documents page opens on this account.
 */
export function AccountDocumentsCard({ accountId }: { accountId: string }) {
  const { t } = useTranslation();
  const download = useDownloadDocument();
  const navigate = useNavigate();
  const [all, setAll] = useState(false);
  const { data } = useQuery<StatementDocument[]>({
    queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
    queryFn: listStatementDocuments,
  });
  const documents = useMemo(
    () =>
      (data ?? [])
        .filter((d) => d.accountId === accountId)
        .sort((a, b) => (b.periodEnd ?? b.createdAt).localeCompare(a.periodEnd ?? a.createdAt)),
    [data, accountId],
  );
  if (documents.length === 0) return null;

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-base">{t("settings:documents.account_card_title")}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-1">
        {(all ? documents : documents.slice(0, SHOWN)).map((doc) => (
          <div key={doc.id} className="flex items-center justify-between gap-2 text-sm">
            <div className="min-w-0">
              <p className="font-medium break-all">{doc.storedName}</p>
              <p className="text-muted-foreground text-xs">
                {doc.periodStart && doc.periodEnd ? `${doc.periodStart} → ${doc.periodEnd} · ` : ""}
                {formatBytes(doc.sizeBytes)}
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              onClick={() => void download(doc.id)}
              aria-label={t("settings:documents.download")}
            >
              <Icons.Download className="size-4" />
            </Button>
          </div>
        ))}
        <div className="flex flex-wrap gap-2 pt-2">
          {documents.length > SHOWN ? (
            <Button type="button" variant="ghost" size="sm" onClick={() => setAll(!all)}>
              <Icons.ChevronDown className={`mr-1 size-4 ${all ? "rotate-180" : ""}`} />
              {all
                ? t("settings:documents.account_card_less")
                : t("settings:documents.account_card_more", { count: documents.length - SHOWN })}
            </Button>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => navigate(`/settings/documents?account=${encodeURIComponent(accountId)}`)}
          >
            {t("settings:documents.account_card_open")}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
