import { getStatementDocumentSources, snoozeStatementDocumentSources } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { DocumentSource } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { DocumentSourceForm } from "./document-source-form";
import { dayAfter, dueSources, sourcesToDescribe } from "./document-sources";

/** How many days "later" puts a reminder off. */
const LATER_DAYS = 3;

export function useDocumentSources() {
  return useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENT_SOURCES],
    queryFn: getStatementDocumentSources,
  });
}

function useAccountName() {
  const { t } = useTranslation();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  return (id: string) =>
    accounts.find((a) => a.id === id)?.name ?? t("settings:documents.no_account");
}

// asked once per opening of the app
let remindedThisSession = false;

/**
 * When the app opens: the documents to bring back now (a life insurance's positions every week…), each with what to
 * bring, in which format and how the person said they get it; then to the documents page to drop them.
 */
export function DocumentSourcesReminder() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const accountName = useAccountName();
  const { data } = useDocumentSources();
  const [open, setOpen] = useState(true);
  const due = dueSources(data ?? []);
  const later = useMutation({
    mutationFn: () =>
      snoozeStatementDocumentSources(
        due.map((s) => s.key),
        dayAfter(LATER_DAYS),
      ),
    onSuccess: (sources) =>
      queryClient.setQueryData([QueryKeys.STATEMENT_DOCUMENT_SOURCES], sources),
  });
  if (remindedThisSession || !open || due.length === 0) return null;
  const close = () => {
    remindedThisSession = true;
    setOpen(false);
  };
  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent
        className="max-h-[90vh] overflow-y-auto sm:max-w-xl"
        data-testid="sources-reminder"
      >
        <DialogHeader>
          <DialogTitle>
            {t("settings:documents.sources.reminder_title", { count: due.length })}
          </DialogTitle>
          <DialogDescription>{t("settings:documents.sources.reminder_text")}</DialogDescription>
        </DialogHeader>
        <ul className="space-y-3">
          {due.map((s) => (
            <SourceLine key={s.key} source={s} accountName={accountName(s.accountId)} />
          ))}
        </ul>
        <DialogFooter>
          <Button
            variant="ghost"
            disabled={later.isPending}
            onClick={() => later.mutate(undefined, { onSettled: close })}
          >
            {t("settings:documents.sources.later", { count: LATER_DAYS })}
          </Button>
          <Button
            onClick={() => {
              close();
              navigate("/settings/documents");
            }}
          >
            {t("settings:documents.sources.drop_now")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SourceLine({ source, accountName }: { source: DocumentSource; accountName: string }) {
  const { t } = useTranslation();
  return (
    <li className="rounded-md border p-3 text-sm">
      <div className="font-medium">{source.label}</div>
      <div className="text-muted-foreground text-xs">
        {[
          accountName,
          source.formats.length
            ? t("settings:documents.sources.format", {
                formats: source.formats.join(", ").toUpperCase(),
              })
            : null,
          source.lastImport
            ? t("settings:documents.sources.last", { date: source.lastImport })
            : null,
        ]
          .filter(Boolean)
          .join(" · ")}
      </div>
      <p className="mt-1 whitespace-pre-line">
        {source.howTo ?? (
          <span className="text-muted-foreground italic">
            {source.description || t("settings:documents.sources.no_how_to")}
          </span>
        )}
      </p>
    </li>
  );
}

/**
 * After an import: the sources brought for the first time, to say once how the document is obtained and how often
 * to ask for it. Skipping keeps the usual rhythm (two weeks) and does not ask again.
 */
export function DescribeNewSourcesDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const accountName = useAccountName();
  const { data } = useDocumentSources();
  const [frozen] = useState<string[] | null>(() =>
    data ? sourcesToDescribe(data).map((s) => s.key) : null,
  );
  const keys = frozen ?? sourcesToDescribe(data ?? []).map((s) => s.key);
  const left = (data ?? []).filter((s) => keys.includes(s.key) && s.needsSetup);
  if (!data || left.length === 0) return null;
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        className="max-h-[90vh] overflow-y-auto sm:max-w-xl"
        data-testid="describe-sources"
      >
        <DialogHeader>
          <DialogTitle>{t("settings:documents.sources.describe_title")}</DialogTitle>
          <DialogDescription>{t("settings:documents.sources.describe_text")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          {left.map((s) => (
            <DocumentSourceForm key={s.key} source={s} accountName={accountName(s.accountId)} />
          ))}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t("settings:documents.sources.skip")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Every source, to change how it is obtained or how often, at any time. */
export function DocumentSourcesPanel() {
  const { t } = useTranslation();
  const accountName = useAccountName();
  const { data } = useDocumentSources();
  if (!data || data.length === 0) return null;
  return (
    <details className="rounded-lg border p-3" data-testid="sources-panel">
      <summary className="cursor-pointer font-semibold">
        {t("settings:documents.sources.panel_title", { count: data.length })}
      </summary>
      <p className="text-muted-foreground mt-1 text-sm">
        {t("settings:documents.sources.panel_text")}
      </p>
      <div className="mt-3 space-y-3">
        {data.map((s) => (
          <div key={s.key}>
            {s.due ? (
              <div className="text-warning mb-1 text-xs">
                {t("settings:documents.sources.due_now")}
              </div>
            ) : s.nextDue && s.everyDays > 0 ? (
              <div className="text-muted-foreground mb-1 text-xs">
                {t("settings:documents.sources.next", { date: s.nextDue })}
              </div>
            ) : null}
            <DocumentSourceForm source={s} accountName={accountName(s.accountId)} />
          </div>
        ))}
      </div>
    </details>
  );
}
