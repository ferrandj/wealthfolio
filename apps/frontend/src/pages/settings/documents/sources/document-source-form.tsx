import { setStatementDocumentSource } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { DocumentSource } from "@/lib/types/statements";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button, Label } from "@wealthfolio/ui";
import { Textarea } from "@wealthfolio/ui/components/ui/textarea";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RHYTHMS } from "./document-sources";

/** How the person gets a document and how often they want to bring it. */
export function DocumentSourceForm({
  source,
  accountName,
  onSaved,
}: {
  source: DocumentSource;
  accountName: string;
  onSaved?: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [howTo, setHowTo] = useState(source.howTo ?? "");
  const [every, setEvery] = useState<number>(source.everyDays);
  const save = useMutation({
    mutationFn: () =>
      setStatementDocumentSource(source.key, {
        howTo: howTo.trim() || null,
        everyDays: every,
        snoozedUntil: source.snoozedUntil ?? null,
      }),
    onSuccess: (sources) => {
      queryClient.setQueryData([QueryKeys.STATEMENT_DOCUMENT_SOURCES], sources);
      onSaved?.();
    },
  });
  return (
    <div className="grid gap-2 rounded-md border p-3" data-testid="document-source-form">
      <div>
        <div className="font-medium">{source.label}</div>
        <div className="text-muted-foreground text-xs">
          {accountName}
          {source.formats.length ? ` · ${source.formats.join(", ").toUpperCase()}` : ""}
        </div>
      </div>
      <Label htmlFor={`howto-${source.key}`}>{t("settings:documents.sources.how_to")}</Label>
      <Textarea
        id={`howto-${source.key}`}
        value={howTo}
        rows={2}
        placeholder={t("settings:documents.sources.how_to_placeholder")}
        onChange={(e) => setHowTo(e.target.value)}
      />
      <Label>{t("settings:documents.sources.every")}</Label>
      <div className="flex flex-wrap gap-1">
        {RHYTHMS.map((days) => (
          <Button
            key={days}
            type="button"
            size="sm"
            variant={every === days ? "default" : "outline"}
            onClick={() => setEvery(days)}
          >
            {t(`settings:documents.sources.rhythm_${days}`)}
          </Button>
        ))}
      </div>
      <div className="flex justify-end">
        <Button size="sm" disabled={save.isPending} onClick={() => save.mutate()}>
          {t("settings:documents.sources.save")}
        </Button>
      </div>
    </div>
  );
}
