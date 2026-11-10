import type { Account } from "@/lib/types";
import type {
  AnalyzedStatement,
  StatementAnalysis,
  StatementImportResult,
} from "@/lib/types/statements";
import {
  Badge,
  Button,
  Card,
  CardContent,
  Icons,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import {
  ACCOUNT_AUTO,
  ACCOUNT_NEW,
  formatBytes,
  needsOverlapDecision,
  overlapsApply,
  type StatementChoice,
} from "../statement-files";

export type PendingStatus = "analyzing" | "ready" | "importing" | "done" | "error";

export interface PendingFile {
  id: string;
  fileName: string;
  sizeBytes: number;
  contentBase64: string;
  status: PendingStatus;
  analysis?: StatementAnalysis;
  /** The reading the user picked when the file could be read several ways. */
  adapterId?: string;
  choices: Record<number, StatementChoice>;
  allowDuplicateDocument: boolean;
  result?: StatementImportResult;
  error?: string;
}

interface Props {
  file: PendingFile;
  accounts: Account[];
  onChange: (patch: Partial<PendingFile>) => void;
  onChoose: (index: number, patch: Partial<StatementChoice>) => void;
  onPickReading: (adapterId: string) => void;
  onImport: () => void;
  onRemove: () => void;
}

function StatementBlock({
  statement,
  choice,
  accounts,
  disabled,
  onChoose,
}: {
  statement: AnalyzedStatement;
  choice: StatementChoice;
  accounts: Account[];
  disabled: boolean;
  onChoose: (patch: Partial<StatementChoice>) => void;
}) {
  const { t } = useTranslation();
  const { preview } = statement;
  const items = [
    {
      value: ACCOUNT_AUTO,
      label: statement.matchedAccount
        ? t("settings:documents.account_matched", { name: statement.matchedAccount.name })
        : t("settings:documents.account_create", { name: statement.suggestedAccountName }),
    },
    { value: ACCOUNT_NEW, label: t("settings:documents.account_new") },
    ...accounts.map((a) => ({ value: a.id, label: a.name })),
  ];
  const documentOnly = statement.mode === "documentOnly";

  return (
    <div className="border-border/60 space-y-3 rounded-lg border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">
          {preview.accountLabel ?? statement.suggestedAccountName}
        </span>
        {preview.iban && (
          <span className="text-muted-foreground font-mono text-xs">{preview.iban}</span>
        )}
        {preview.periodStart && preview.periodEnd && (
          <Badge variant="secondary">
            {preview.periodStart} → {preview.periodEnd}
          </Badge>
        )}
        {preview.confidence === "low" && (
          <Badge variant="destructive">{t("settings:documents.low_confidence")}</Badge>
        )}
      </div>

      {documentOnly ? (
        <p className="text-muted-foreground text-sm">{t("settings:documents.document_only")}</p>
      ) : (
        <p className="text-muted-foreground text-sm">
          {t("settings:documents.operations_count", { count: preview.transactions.length })}
          {statement.knownDuplicates > 0 &&
            ` · ${t("settings:documents.duplicates_count", { count: statement.knownDuplicates })}`}
        </p>
      )}

      {preview.warnings.length > 0 && (
        <ul className="text-warning list-disc ps-5 text-xs">
          {preview.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}

      {overlapsApply(statement, choice) && (
        <div
          className="border-warning/50 bg-warning/10 space-y-2 rounded-md border p-3 text-sm"
          role="alert"
        >
          <p className="font-medium">{t("settings:documents.overlap_title")}</p>
          <ul className="list-disc ps-5">
            {statement.overlaps.map((o) => (
              <li key={o.documentId}>
                {t("settings:documents.overlap_item", {
                  name: o.fileName,
                  start: o.periodStart ?? "?",
                  end: o.periodEnd ?? "?",
                  count: o.importedCount,
                })}
              </li>
            ))}
          </ul>
          <p className="text-muted-foreground">{t("settings:documents.overlap_question")}</p>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              disabled={disabled}
              variant={choice.overlap === "replace" ? "default" : "outline"}
              onClick={() => onChoose({ overlap: "replace" })}
            >
              {t("settings:documents.overlap_replace")}
            </Button>
            <Button
              type="button"
              size="sm"
              disabled={disabled}
              variant={choice.overlap === "keep" ? "default" : "outline"}
              onClick={() => onChoose({ overlap: "keep" })}
            >
              {t("settings:documents.overlap_keep")}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={disabled}
              onClick={() => onChoose({ skip: true })}
            >
              {t("settings:documents.overlap_skip")}
            </Button>
          </div>
        </div>
      )}

      <div className="grid gap-2 sm:grid-cols-[1fr_auto] sm:items-center">
        <Select
          items={items}
          value={choice.account}
          onValueChange={(value) => onChoose({ account: String(value), skip: false })}
          disabled={disabled || choice.skip}
        >
          <SelectTrigger aria-label={t("settings:documents.account")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {items.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          variant={choice.skip ? "default" : "ghost"}
          size="sm"
          disabled={disabled}
          onClick={() => onChoose({ skip: !choice.skip })}
        >
          {choice.skip ? t("settings:documents.include") : t("settings:documents.skip")}
        </Button>
      </div>

      {choice.account === ACCOUNT_NEW && !choice.skip && (
        <Input
          value={choice.newAccountName}
          placeholder={statement.suggestedAccountName}
          aria-label={t("settings:documents.account_name")}
          disabled={disabled}
          onChange={(e) => onChoose({ newAccountName: e.target.value })}
        />
      )}

      {!documentOnly && preview.transactions.length > 0 && (
        <details className="text-sm">
          <summary className="text-muted-foreground cursor-pointer">
            {t("settings:documents.preview_transactions")}
          </summary>
          <ul className="mt-2 max-h-56 space-y-1 overflow-auto">
            {preview.transactions.slice(0, 200).map((tx, i) => (
              <li key={`${tx.date}-${i}`} className="flex justify-between gap-3">
                <span className="min-w-0 truncate">
                  <span className="text-muted-foreground me-2">{tx.date}</span>
                  {tx.label}
                </span>
                <span className="shrink-0 tabular-nums">
                  {tx.amount} {tx.currency}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

export function PendingFileCard({
  file,
  accounts,
  onChange,
  onChoose,
  onPickReading,
  onImport,
  onRemove,
}: Props) {
  const { t } = useTranslation();
  const analysis = file.analysis;
  const busy = file.status === "analyzing" || file.status === "importing";
  const info = analysis?.analysis;
  const needsReading = !!info && !info.chosenAdapterId && info.candidates.length > 0;
  const existing = analysis?.existingDocument;
  const canImport =
    file.status === "ready" &&
    !!analysis &&
    analysis.statements.length > 0 &&
    (!existing || file.allowDuplicateDocument) &&
    !analysis.statements.some((s) => needsOverlapDecision(s, file.choices[s.preview.index]));

  return (
    <Card>
      <CardContent className="space-y-3 p-4">
        <div className="flex items-start gap-3">
          <Icons.FileText className="text-muted-foreground mt-0.5 size-5 shrink-0" />
          <div className="min-w-0 flex-1">
            <div className="truncate font-medium">{file.fileName}</div>
            <div className="text-muted-foreground text-xs">
              {formatBytes(file.sizeBytes)}
              {info?.institution && ` · ${info.institution}`}
              {info?.chosenAdapterId && ` · ${info.chosenAdapterId}`}
            </div>
          </div>
          {busy && <Icons.Spinner className="size-4 animate-spin" />}
          {file.status === "done" && <Icons.Check className="text-success size-5" />}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={onRemove}
            disabled={file.status === "importing"}
            aria-label={t("settings:documents.remove")}
          >
            <Icons.Close className="size-4" />
          </Button>
        </div>

        {file.status === "error" && (
          <p className="text-destructive text-sm" role="alert">
            {file.error}
          </p>
        )}

        {info?.error && (
          <p className="text-destructive text-sm" role="alert">
            {info.error}
          </p>
        )}

        {info && info.message && <p className="text-muted-foreground text-sm">{info.message}</p>}

        {needsReading && (
          <div className="space-y-2">
            <p className="text-sm font-medium">{t("settings:documents.choose_reading")}</p>
            <div className="flex flex-wrap gap-2">
              {info.candidates.map((c) => (
                <Button
                  key={c.adapterId}
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => onPickReading(c.adapterId)}
                >
                  {c.label}
                </Button>
              ))}
            </div>
          </div>
        )}

        {info && info.candidates.length === 0 && !info.error && (
          <p className="text-destructive text-sm">{t("settings:documents.unknown_format")}</p>
        )}

        {existing && file.status !== "done" && (
          <div className="bg-muted/50 flex flex-wrap items-center justify-between gap-2 rounded-md p-3 text-sm">
            <span>
              {t("settings:documents.already_imported", {
                date: existing.createdAt.slice(0, 10),
              })}
            </span>
            <Button
              type="button"
              size="sm"
              variant={file.allowDuplicateDocument ? "default" : "outline"}
              onClick={() => onChange({ allowDuplicateDocument: !file.allowDuplicateDocument })}
            >
              {t("settings:documents.import_anyway")}
            </Button>
          </div>
        )}

        {file.status !== "done" &&
          analysis?.statements.map((statement) => (
            <StatementBlock
              key={statement.preview.index}
              statement={statement}
              accounts={accounts}
              disabled={busy}
              choice={
                file.choices[statement.preview.index] ?? {
                  account: ACCOUNT_AUTO,
                  newAccountName: "",
                  skip: false,
                }
              }
              onChoose={(patch) => onChoose(statement.preview.index, patch)}
            />
          ))}

        {file.status === "done" && file.result && (
          <div className="space-y-1 text-sm">
            {file.result.outcomes.map((o) => (
              <div key={o.index}>
                <span className="font-medium">{o.accountName ?? "—"}</span>
                {o.createdAccount && ` · ${t("settings:documents.created_account")}`}
                {` · ${t("settings:documents.result_summary", {
                  imported: o.imported,
                  duplicates: o.duplicates,
                })}`}
                {o.openingBalanceAdded && ` · ${t("settings:documents.opening_balance")}`}
                {o.toReview > 0 &&
                  ` · ${t("settings:documents.review_to_check", { count: o.toReview })}`}
                {o.messages.map((m) => (
                  <div key={m} className="text-muted-foreground text-xs">
                    {m}
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}

        {(file.status === "ready" || file.status === "importing") && (
          <div className="flex justify-end">
            <Button type="button" onClick={onImport} disabled={!canImport || busy}>
              {file.status === "importing" && (
                <Icons.Spinner className="me-2 size-4 animate-spin" />
              )}
              {t("settings:documents.import_button")}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
