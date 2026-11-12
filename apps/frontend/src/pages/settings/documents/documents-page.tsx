import { analyzeStatement, importStatement, logger } from "@/adapters";
import { invalidateLoanCaches } from "../loans/loan-caches";
import { useAccounts } from "@/hooks/use-accounts";
import { invalidatePerformanceCaches } from "@/lib/performance-cache";
import { QueryKeys } from "@/lib/query-keys";
import { invalidateSpendingCaches } from "@/features/spending/lib/invalidation";
import { useQueryClient } from "@tanstack/react-query";
import { Button, Icons, Separator } from "@wealthfolio/ui";
import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsHeader } from "../settings-header";
import { AccountOwnerPrompt, OwnerlessAccountsBanner } from "../contacts/components/account-owners";
import { CoverageCard } from "./components/coverage-card";
import { DocumentsLibrary } from "./components/documents-library";
import { PendingFileCard, type PendingFile } from "./components/pending-file-card";
import { SupportedDocuments } from "./components/supported-documents";
import {
  fileToBase64,
  needsOverlapDecision,
  overrideFor,
  replacedDocumentIds,
  type StatementChoice,
} from "./statement-files";

const message = (e: unknown) =>
  e && typeof e === "object" && "message" in e
    ? String((e as { message: unknown }).message)
    : String(e);

let counter = 0;

export default function DocumentsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false });
  const [files, setFiles] = useState<PendingFile[]>([]);
  const [dragging, setDragging] = useState(false);
  // accounts this session's imports created: their owner is asked once the imports are done
  const [newAccounts, setNewAccounts] = useState<string[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const patchFile = useCallback((id: string, patch: Partial<PendingFile>) => {
    setFiles((current) => current.map((f) => (f.id === id ? { ...f, ...patch } : f)));
  }, []);

  const analyse = useCallback(
    async (id: string, fileName: string, contentBase64: string, adapterId?: string) => {
      patchFile(id, { status: "analyzing", error: undefined });
      try {
        const analysis = await analyzeStatement(fileName, contentBase64, adapterId);
        patchFile(id, { status: "ready", analysis, adapterId });
      } catch (e) {
        logger.error(`Error analyzing statement: ${String(e)}`);
        patchFile(id, { status: "error", error: message(e) });
      }
    },
    [patchFile],
  );

  const addFiles = useCallback(
    async (list: FileList | File[]) => {
      for (const file of Array.from(list)) {
        counter += 1;
        const id = `${Date.now()}-${counter}`;
        setFiles((current) => [
          ...current,
          {
            id,
            fileName: file.name,
            sizeBytes: file.size,
            contentBase64: "",
            status: "analyzing",
            choices: {},
            allowDuplicateDocument: false,
          },
        ]);
        try {
          const contentBase64 = await fileToBase64(file);
          patchFile(id, { contentBase64 });
          await analyse(id, file.name, contentBase64);
        } catch (e) {
          patchFile(id, { status: "error", error: message(e) });
        }
      }
    },
    [analyse, patchFile],
  );

  const importOne = useCallback(
    async (file: PendingFile) => {
      patchFile(file.id, { status: "importing", error: undefined });
      try {
        const overrides = (file.analysis?.statements ?? []).flatMap((s) => {
          const choice = file.choices[s.preview.index];
          const o = choice ? overrideFor(s.preview.index, choice) : null;
          return o ? [o] : [];
        });
        const result = await importStatement({
          fileName: file.fileName,
          contentBase64: file.contentBase64,
          adapterId: file.adapterId ?? null,
          overrides,
          allowDuplicateDocument: file.allowDuplicateDocument,
          replaceDocumentIds: replacedDocumentIds(file.analysis?.statements ?? [], file.choices),
        });
        patchFile(file.id, { status: "done", result });
        const created = result.outcomes.flatMap((o) =>
          o.createdAccount && o.accountId ? [o.accountId] : [],
        );
        if (created.length > 0) setNewAccounts((list) => [...new Set([...list, ...created])]);
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENTS] }),
          queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_COVERAGE] }),
          queryClient.invalidateQueries({ queryKey: [QueryKeys.ACCOUNT_IDENTITIES] }),
          queryClient.invalidateQueries({ queryKey: [QueryKeys.ACCOUNTS] }),
          queryClient.invalidateQueries({ queryKey: [QueryKeys.ACTIVITY_DATA] }),
          queryClient.invalidateQueries({ queryKey: [QueryKeys.ACTIVITIES] }),
          invalidateLoanCaches(queryClient),
        ]);
        invalidatePerformanceCaches(queryClient);
        invalidateSpendingCaches(queryClient);
      } catch (e) {
        logger.error(`Error importing statement: ${String(e)}`);
        patchFile(file.id, { status: "error", error: message(e) });
      }
    },
    [patchFile, queryClient],
  );

  const importable = files.filter(
    (f) =>
      f.status === "ready" &&
      f.analysis &&
      f.analysis.statements.length > 0 &&
      (!f.analysis.existingDocument || f.allowDuplicateDocument) &&
      !f.analysis.statements.some((s) => needsOverlapDecision(s, f.choices[s.preview.index])),
  );

  const importAll = async () => {
    // One after the other: the writes go through a single database connection anyway.
    for (const file of importable) await importOne(file);
  };

  const setChoice = (file: PendingFile, index: number, patch: Partial<StatementChoice>) => {
    const current = file.choices[index] ?? { account: "auto", newAccountName: "", skip: false };
    patchFile(file.id, { choices: { ...file.choices, [index]: { ...current, ...patch } } });
  };

  return (
    <div className="space-y-6">
      <SettingsHeader
        heading={t("settings:documents.title")}
        text={t("settings:documents.subtitle")}
      />
      <Separator />

      <div
        role="button"
        tabIndex={0}
        aria-label={t("settings:documents.drop_title")}
        onClick={() => inputRef.current?.click()}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            inputRef.current?.click();
          }
        }}
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          if (e.dataTransfer.files.length > 0) void addFiles(e.dataTransfer.files);
        }}
        className={`flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed p-10 text-center transition-colors ${
          dragging ? "border-primary bg-primary/5" : "border-border hover:bg-muted/40"
        }`}
      >
        <Icons.Upload className="text-muted-foreground size-8" />
        <div className="font-medium">{t("settings:documents.drop_title")}</div>
        <div className="text-muted-foreground text-sm">{t("settings:documents.drop_hint")}</div>
        <Button type="button" variant="outline" size="sm" tabIndex={-1}>
          {t("settings:documents.browse")}
        </Button>
        <input
          ref={inputRef}
          type="file"
          multiple
          hidden
          accept=".pdf,.csv,.ofx,.qfx,.qif,.txt,application/pdf,text/csv"
          onChange={(e) => {
            if (e.target.files) void addFiles(e.target.files);
            e.target.value = "";
          }}
        />
      </div>

      {files.length > 0 && (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold">{t("settings:documents.to_review")}</h2>
            <div className="flex gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setFiles((c) => c.filter((f) => f.status !== "done"))}
              >
                {t("settings:documents.clear_done")}
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={importable.length === 0}
                onClick={() => void importAll()}
              >
                {t("settings:documents.import_all", { count: importable.length })}
              </Button>
            </div>
          </div>
          {files.map((file) => (
            <PendingFileCard
              key={file.id}
              file={file}
              accounts={accounts}
              onChange={(patch) => patchFile(file.id, patch)}
              onChoose={(index, patch) => setChoice(file, index, patch)}
              onPickReading={(adapterId) =>
                void analyse(file.id, file.fileName, file.contentBase64, adapterId)
              }
              onImport={() => void importOne(file)}
              onRemove={() => setFiles((c) => c.filter((f) => f.id !== file.id))}
            />
          ))}
        </div>
      )}

      <OwnerlessAccountsBanner />

      <CoverageCard />

      <SupportedDocuments />

      <Separator />
      <div className="space-y-3">
        <h2 className="text-lg font-semibold">{t("settings:documents.library_title")}</h2>
        <DocumentsLibrary />
      </div>
      {newAccounts.length > 0 && !files.some((f) => f.status === "importing") ? (
        <AccountOwnerPrompt accountIds={newAccounts} onClose={() => setNewAccounts([])} />
      ) : null}
    </div>
  );
}
