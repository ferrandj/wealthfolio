import {
  deleteStatementDocument,
  getStatementOutdated,
  autoReviewStatementDocuments,
  invalidateAllStatementDocuments,
  getStatementLoans,
  linkStatementDocument,
  listStatementDocuments,
  getStatementRevalidationProgress,
  revalidateAllStatementDocuments,
  revalidateStatementDocument,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { invalidateLoanCaches } from "../../loans/loan-caches";
import { QueryKeys } from "@/lib/query-keys";
import type { StatementDocument } from "@/lib/types/statements";
import { invalidatePerformanceCaches } from "@/lib/performance-cache";
import { invalidateSpendingCaches } from "@/features/spending/lib/invalidation";
import { LoanAccountDialog } from "../../loans/loan-account-dialog";
import { LOAN_KINDS, loanOfDocument } from "./loan-validation-card";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Badge,
  Button,
  Card,
  CardContent,
  Icons,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { formatBytes } from "../statement-files";
import { useDownloadDocument } from "../use-download-document";
import { logicalOrder, parseReviewReason } from "../statement-review";
import { DocumentReviewDialog } from "./document-review-dialog";
import { DocumentGroupActions } from "./document-group-actions";
import { matchesHolderFilter, type HolderFilter } from "../holders";

const NO_ACCOUNT = "none";

export function DocumentsLibrary() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({
    filterActive: false,
    includeArchived: true,
  });
  const [toDelete, setToDelete] = useState<StatementDocument | null>(null);
  const [confirmAll, setConfirmAll] = useState(false);
  const [confirmInvalidate, setConfirmInvalidate] = useState(false);
  const [attaching, setAttaching] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState<string | null>(null);
  // one document opened from its own row: the review stops after it
  const [reviewingAlone, setReviewingAlone] = useState(false);
  const [holderFilter, setHolderFilter] = useState<HolderFilter>("all");
  const [viewMode, setViewMode] = useState<"list" | "tree">("tree");
  // opened from an account: only its documents
  const [params, setParams] = useSearchParams();
  const accountFilter = params.get("account");

  const { data: loans } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
  });
  const { data: documents, isLoading } = useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
    queryFn: listStatementDocuments,
  });

  const refresh = () =>
    Promise.all([
      invalidateLoanCaches(queryClient),
      queryClient.invalidateQueries({
        queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
      }),
      queryClient.invalidateQueries({
        queryKey: [QueryKeys.STATEMENT_COVERAGE],
      }),
      queryClient.invalidateQueries({
        queryKey: [QueryKeys.STATEMENT_OUTDATED],
      }),
    ]);

  const refreshOperations = async () => {
    await refresh();
    await queryClient.invalidateQueries({
      queryKey: [QueryKeys.ACTIVITY_DATA],
    });
    await queryClient.invalidateQueries({ queryKey: [QueryKeys.ACTIVITIES] });
    invalidatePerformanceCaches(queryClient);
    invalidateSpendingCaches(queryClient);
  };

  // oldest first, account by account: the order in which statements make sense one after the other
  const { data: outdated } = useQuery({
    queryKey: [QueryKeys.STATEMENT_OUTDATED],
    queryFn: getStatementOutdated,
  });
  const outdatedIds = useMemo(() => new Set(outdated?.documentIds ?? []), [outdated]);

  const ordered = useMemo(
    () =>
      [...(documents ?? [])]
        .filter((d) => matchesHolderFilter(d.holderName, holderFilter))
        .filter((d) => !accountFilter || d.accountId === accountFilter)
        .sort(logicalOrder),
    [documents, holderFilter, accountFilter],
  );
  const pending = ordered.filter((d) => d.status === "PENDING");

  const revalidateOne = useMutation({
    mutationFn: (id: string) => revalidateStatementDocument(id),
    onSuccess: refreshOperations,
    onError: (e) =>
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const revalidateAll = useMutation({
    mutationFn: revalidateAllStatementDocuments,
    onSuccess: async (result) => {
      setConfirmAll(false);
      await refreshOperations();
      toast({
        title: t("settings:documents.revalidate_all_done"),
        description: t("settings:documents.revalidate_all_summary", {
          validated: result.validated,
          pending: result.pending,
          failed: result.failed,
        }),
      });
    },
    onError: (e) => {
      setConfirmAll(false);
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      });
    },
  });

  // How far the re-reading is. It runs on the server: it is also followed when this page is opened (or opened
  // again) while it runs.
  const { data: progress } = useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENTS, "revalidation-progress"],
    queryFn: getStatementRevalidationProgress,
    refetchInterval: (query) =>
      revalidateAll.isPending || query.state.data?.running ? 800 : false,
  });
  const rereading = revalidateAll.isPending || !!progress?.running;
  const progressLabel =
    progress?.running && progress.total > 0
      ? t("settings:documents.revalidate_all_progress", {
          done: progress.done,
          total: progress.total,
        })
      : t("settings:documents.revalidate_all_running");

  // validates every waiting document that raises no alert; what stopped the others is summed up
  const autoReview = useMutation({
    mutationFn: autoReviewStatementDocuments,
    onSuccess: async (result) => {
      await Promise.all([
        refreshOperations(),
        queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_REVIEW_QUEUE] }),
      ]);
      const counts = new Map<string, number>();
      for (const skip of result.skipped) {
        for (const reason of skip.reasons) counts.set(reason, (counts.get(reason) ?? 0) + 1);
      }
      const why = [...counts.entries()]
        .map(([reason, n]) => `${t(`settings:documents.auto_review_reason_${reason}`)} (${n})`)
        .join(", ");
      const summary = t("settings:documents.auto_review_summary", {
        validated: result.validated,
        skipped: result.skipped.length,
      });
      toast({
        title: t("settings:documents.auto_review_done"),
        description: why ? `${summary} — ${why}` : summary,
      });
    },
    onError: (e) =>
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const invalidateAll = useMutation({
    mutationFn: invalidateAllStatementDocuments,
    onSuccess: async (count) => {
      setConfirmInvalidate(false);
      await Promise.all([
        refresh(),
        queryClient.invalidateQueries({
          queryKey: [QueryKeys.STATEMENT_REVIEW_QUEUE],
        }),
      ]);
      toast({
        title: t("settings:documents.invalidate_all_done"),
        description: t("settings:documents.invalidate_all_summary", { count }),
      });
    },
    onError: (e) => {
      setConfirmInvalidate(false);
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      });
    },
  });

  const link = useMutation({
    mutationFn: ({ id, accountId }: { id: string; accountId: string | null }) =>
      linkStatementDocument(id, accountId),
    onSuccess: refresh,
    onError: (e) =>
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const remove = useMutation({
    mutationFn: ({ id, removeOperations }: { id: string; removeOperations: boolean }) =>
      deleteStatementDocument(id, removeOperations),
    onSuccess: async (_d, vars) => {
      await refresh();
      if (vars.removeOperations) {
        await queryClient.invalidateQueries({
          queryKey: [QueryKeys.ACTIVITY_DATA],
        });
        await queryClient.invalidateQueries({
          queryKey: [QueryKeys.ACTIVITIES],
        });
        invalidatePerformanceCaches(queryClient);
      }
      setToDelete(null);
    },
    onError: (e) =>
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const downloadDocument = useDownloadDocument();
  const download = (doc: StatementDocument) => downloadDocument(doc.id);

  // A validated document was checked by a person: reading it again takes three clicks in a row.
  const [armed, setArmed] = useState<{ id: string; count: number } | null>(null);
  const clicksNeeded = (doc: StatementDocument) => (doc.status === "VALIDATED" ? 3 : 1);
  const onRevalidateClick = (doc: StatementDocument) => {
    const needed = clicksNeeded(doc);
    const count = armed?.id === doc.id ? armed.count + 1 : 1;
    if (count >= needed) {
      setArmed(null);
      revalidateOne.mutate(doc.id);
      return;
    }
    setArmed({ id: doc.id, count });
    window.setTimeout(
      () => setArmed((a) => (a?.id === doc.id && a.count === count ? null : a)),
      4000,
    );
  };
  const revalidateTitle = (doc: StatementDocument) =>
    doc.status === "VALIDATED"
      ? t("settings:documents.revalidate_protected", {
          left: Math.max(1, 3 - (armed?.id === doc.id ? armed.count : 0)),
        })
      : t("settings:documents.revalidate_one");

  const renderDoc = (doc: StatementDocument) => {
    const isLoan = LOAN_KINDS.includes(doc.documentKind ?? "");
    const loan = isLoan ? loanOfDocument(loans, doc.id) : undefined;
    return (
      <Card key={doc.id}>
        <CardContent className="flex flex-col gap-3 p-4 md:flex-row md:items-center">
          <Icons.FileText className="text-muted-foreground hidden size-5 shrink-0 md:block" />
          <div className="min-w-0 flex-1">
            <div className="font-medium break-all" title={doc.storedName}>
              {doc.storedName}
            </div>
            <div className="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
              <span>{doc.createdAt.slice(0, 10)}</span>
              <span>{formatBytes(doc.sizeBytes)}</span>
              {doc.periodStart && doc.periodEnd && (
                <span>
                  {doc.periodStart} → {doc.periodEnd}
                </span>
              )}
              {doc.status === "STORED" ? (
                <Badge variant="outline">{t("settings:documents.kept_only")}</Badge>
              ) : (
                <>
                  <Badge variant="secondary">
                    {t("settings:documents.operations_added", {
                      count: doc.importedCount,
                    })}
                  </Badge>
                  <StatusBadge doc={doc} />
                  {outdatedIds.has(doc.id) && (
                    <Badge variant="outline">{t("settings:documents.reader_outdated")}</Badge>
                  )}
                </>
              )}
            </div>
          </div>
          <div className="w-full md:w-56">
            {isLoan ? (
              <div className="flex items-center justify-between gap-2 text-sm">
                <div className="min-w-0 space-y-0.5">
                  <div className="font-medium break-words">
                    {loan?.liabilityName ?? t("settings:loans.account_none")}
                  </div>
                  <div className="text-muted-foreground text-xs">
                    {!loan?.liabilityId
                      ? t("settings:loans.account_to_attach")
                      : loan.liabilityPending
                        ? t("settings:loans.account_pending")
                        : t("settings:loans.account_validated")}
                  </div>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant={loan?.liabilityId ? "ghost" : "default"}
                  onClick={() => setAttaching(doc.id)}
                >
                  {loan?.liabilityId
                    ? t("settings:loans.account_change")
                    : t("settings:loans.account_open")}
                </Button>
              </div>
            ) : (
              <Select
                items={accountItems}
                value={doc.accountId ?? NO_ACCOUNT}
                onValueChange={(value) =>
                  link.mutate({
                    id: doc.id,
                    accountId: value === NO_ACCOUNT ? null : String(value),
                  })
                }
              >
                <SelectTrigger aria-label={t("settings:documents.account")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {accountItems.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
          <div className="flex gap-1">
            {doc.status !== "STORED" && (
              <Button
                type="button"
                variant={doc.status === "PENDING" ? "default" : "ghost"}
                size="icon"
                disabled={rereading}
                onClick={() => {
                  setReviewingAlone(true);
                  setReviewing(doc.id);
                }}
                aria-label={t(
                  doc.status === "PENDING"
                    ? "settings:documents.validate_one"
                    : "settings:documents.review_validated_one",
                )}
                title={t(
                  doc.status === "PENDING"
                    ? "settings:documents.validate_one"
                    : "settings:documents.review_validated_one",
                )}
              >
                <Icons.ListChecks className="size-4" />
              </Button>
            )}
            {doc.status !== "STORED" && (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                disabled={revalidateOne.isPending || rereading}
                onClick={() => onRevalidateClick(doc)}
                aria-label={t("settings:documents.revalidate_one")}
                title={revalidateTitle(doc)}
              >
                <Icons.RefreshCw className="size-4" />
              </Button>
            )}
            <Button
              type="button"
              variant="ghost"
              size="icon"
              onClick={() => void download(doc)}
              aria-label={t("settings:documents.download")}
            >
              <Icons.Download className="size-4" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              onClick={() => setToDelete(doc)}
              aria-label={t("settings:documents.delete")}
            >
              <Icons.Trash className="size-4" />
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  };

  // institution → account (a loan's papers under its loan account) → kind → year
  const renderTree = () => {
    const accountName = (id: string | null | undefined) =>
      id ? (accounts.find((a) => a.id === id)?.name ?? id) : t("settings:documents.no_account");
    const nodeOf = (d: StatementDocument) => {
      if (LOAN_KINDS.includes(d.documentKind ?? "")) {
        const loan = loanOfDocument(loans, d.id);
        const name = loan?.liabilityName ?? loan?.label;
        if (name) return t("settings:documents.tree_loan", { name });
      }
      return accountName(d.accountId);
    };
    const kindName = (kind: string | null | undefined) =>
      t(`settings:documents.kind.${kind ?? "document"}`, {
        defaultValue: kind ? kind.replace(/_/g, " ") : t("settings:documents.kind.document"),
      });
    const tree = new Map<string, Map<string, Map<string, StatementDocument[]>>>();
    for (const d of ordered) {
      const inst = d.institution ?? "—";
      const acc = nodeOf(d);
      const leaf = `${kindName(d.documentKind)} · ${(d.periodEnd ?? d.createdAt).slice(0, 4)}`;
      const byAcc = tree.get(inst) ?? new Map();
      const byLeaf = byAcc.get(acc) ?? new Map();
      byLeaf.set(leaf, [...(byLeaf.get(leaf) ?? []), d]);
      byAcc.set(acc, byLeaf);
      tree.set(inst, byAcc);
    }
    const docsOf = (leaves: Map<string, StatementDocument[]>) => [...leaves.values()].flat();
    return [...tree.entries()].map(([inst, accs]) => (
      <details key={inst} open className="rounded-lg border p-2">
        <summary className="cursor-pointer font-semibold">
          {inst}
          <DocumentGroupActions
            name={inst}
            documents={[...accs.values()].flatMap(docsOf)}
            onDone={refreshOperations}
          />
        </summary>
        {[...accs.entries()].map(([acc, leaves]) => (
          <details key={acc} open className="mt-2 ml-4">
            <summary className="cursor-pointer font-medium">
              {acc}
              <DocumentGroupActions
                name={acc}
                documents={docsOf(leaves)}
                onDone={refreshOperations}
              />
            </summary>
            {[...leaves.entries()].map(([leaf, docs]) => (
              <details key={leaf} open className="mt-2 ml-4">
                <summary className="text-muted-foreground cursor-pointer text-sm">
                  {leaf} ({docs.length})
                  <DocumentGroupActions name={leaf} documents={docs} onDone={refreshOperations} />
                </summary>
                <div className="mt-2 space-y-2">{docs.map(renderDoc)}</div>
              </details>
            ))}
          </details>
        ))}
      </details>
    ));
  };

  if (isLoading) return <Skeleton className="h-24" />;

  if (!documents || documents.length === 0) {
    return <p className="text-muted-foreground text-sm">{t("settings:documents.library_empty")}</p>;
  }

  const holderItems = [
    { value: "all", label: t("settings:documents.holder_all") },
    { value: "joint", label: t("settings:documents.holder_joint") },
    { value: "individual", label: t("settings:documents.holder_individual") },
  ];

  const accountItems = [
    { value: NO_ACCOUNT, label: t("settings:documents.no_account") },
    ...accounts.map((a) => ({ value: a.id, label: a.name })),
  ];

  return (
    <>
      <div className="mb-3 flex flex-wrap items-center justify-end gap-2">
        {accountFilter ? (
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="me-auto"
            onClick={() => {
              params.delete("account");
              setParams(params, { replace: true });
            }}
          >
            {t("settings:documents.only_account", {
              name: accounts.find((a) => a.id === accountFilter)?.name ?? accountFilter,
            })}
            <Icons.Close className="ms-1 size-3" />
          </Button>
        ) : null}
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setViewMode((m) => (m === "list" ? "tree" : "list"))}
        >
          {viewMode === "list"
            ? t("settings:documents.view_tree")
            : t("settings:documents.view_list")}
        </Button>
        <Select
          items={holderItems}
          value={holderFilter}
          onValueChange={(v) => v && setHolderFilter(v as HolderFilter)}
        >
          <SelectTrigger className="w-44" aria-label={t("settings:documents.holder_filter")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {holderItems.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={pending.length === 0}
          onClick={() => {
            setReviewingAlone(false);
            setReviewing(pending[0]?.id ?? null);
          }}
        >
          <Icons.ListChecks className="mr-1 size-4" />
          {pending.length === 0
            ? t("settings:documents.review_none")
            : t("settings:documents.review_start", { count: pending.length })}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          title={t("settings:documents.auto_review_hint")}
          disabled={pending.length === 0 || autoReview.isPending || rereading}
          onClick={() => autoReview.mutate()}
        >
          <Icons.Wand2 className={`me-1 size-4 ${autoReview.isPending ? "animate-pulse" : ""}`} />
          {autoReview.isPending
            ? t("settings:documents.auto_review_running")
            : t("settings:documents.auto_review")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={invalidateAll.isPending || !documents.some((d) => d.status === "VALIDATED")}
          onClick={() => setConfirmInvalidate(true)}
        >
          <Icons.AlertTriangle className="mr-1 size-4" />
          {t("settings:documents.invalidate_all")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={rereading}
          onClick={() => setConfirmAll(true)}
        >
          <Icons.RefreshCw className={`mr-1 size-4 ${rereading ? "animate-spin" : ""}`} />
          {rereading
            ? progressLabel
            : outdatedIds.size > 0
              ? t("settings:documents.update_reader", {
                  count: outdatedIds.size,
                })
              : t("settings:documents.revalidate_all")}
        </Button>
      </div>
      <div className="space-y-2">{viewMode === "list" ? ordered.map(renderDoc) : renderTree()}</div>

      <AlertDialog open={confirmInvalidate} onOpenChange={setConfirmInvalidate}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings:documents.invalidate_all_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:documents.invalidate_all_description")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings:documents.cancel")}</AlertDialogCancel>
            <Button disabled={invalidateAll.isPending} onClick={() => invalidateAll.mutate()}>
              {t("settings:documents.invalidate_all_confirm")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmAll} onOpenChange={setConfirmAll}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings:documents.revalidate_all_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:documents.revalidate_all_description")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {rereading && (
            <div className="space-y-1" role="status" aria-live="polite">
              <div className="text-sm font-medium tabular-nums">{progressLabel}</div>
              {progress?.running && progress.total > 0 && (
                <div className="bg-muted h-1.5 overflow-hidden rounded-full">
                  <div
                    className="bg-primary h-full transition-[width]"
                    style={{ width: `${Math.round((progress.done / progress.total) * 100)}%` }}
                  />
                </div>
              )}
              {progress?.running && progress.fileName && (
                <div className="text-muted-foreground text-xs break-all">{progress.fileName}</div>
              )}
            </div>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings:documents.cancel")}</AlertDialogCancel>
            <Button disabled={rereading} onClick={() => revalidateAll.mutate()}>
              {rereading ? progressLabel : t("settings:documents.revalidate_all_confirm")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <LoanAccountDialog
        documentId={attaching}
        onClose={() => {
          setAttaching(null);
          void refresh();
        }}
      />
      {reviewing && (
        <DocumentReviewDialog
          startId={reviewing}
          queue={reviewingAlone ? ordered.filter((d) => d.id === reviewing) : pending}
          onClose={() => {
            setReviewing(null);
            setReviewingAlone(false);
            void refresh();
          }}
        />
      )}

      <AlertDialog open={!!toDelete} onOpenChange={(open) => !open && setToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings:documents.delete_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:documents.delete_description", {
                name: toDelete?.storedName ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings:documents.cancel")}</AlertDialogCancel>
            <Button
              variant="outline"
              disabled={remove.isPending}
              onClick={() =>
                toDelete && remove.mutate({ id: toDelete.id, removeOperations: false })
              }
            >
              {t("settings:documents.delete_keep_operations")}
            </Button>
            {toDelete && toDelete.importedCount > 0 && (
              <AlertDialogAction
                disabled={remove.isPending}
                onClick={() => remove.mutate({ id: toDelete.id, removeOperations: true })}
              >
                {t("settings:documents.delete_with_operations")}
              </AlertDialogAction>
            )}
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function StatusBadge({ doc }: { doc: StatementDocument }) {
  const { t } = useTranslation();
  if (doc.status === "VALIDATED") {
    return (
      <Badge variant="secondary" className="text-green-700">
        <Icons.Check className="mr-1 size-3" />
        {t("settings:documents.status_validated")}
      </Badge>
    );
  }
  const reason = parseReviewReason(doc.reviewReason);
  const parts: string[] = [];
  if (reason?.balanceGap) {
    parts.push(t("settings:documents.reason_gap", { gap: reason.balanceGap }));
  }
  if (doc.toReviewCount > 0) {
    parts.push(t("settings:documents.reason_doubts", { count: doc.toReviewCount }));
  }
  if (reason && reason.warnings > 0) parts.push(t("settings:documents.reason_warnings"));
  if (reason?.chain) parts.push(t("settings:documents.reason_chain"));
  if (reason?.reopened) parts.push(t("settings:documents.reason_reopened"));
  return (
    <Badge variant="outline" className="border-amber-500 text-amber-700" title={parts.join(" · ")}>
      {t("settings:documents.status_pending")}
      {parts.length > 0 ? ` · ${parts.join(" · ")}` : ""}
    </Badge>
  );
}
