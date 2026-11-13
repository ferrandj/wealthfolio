import {
  getStatementLoans,
  reviewStatementDocument,
  validateStatementDocument,
  validateStatementLoan,
} from "@/adapters";
import { invalidateLoanCaches } from "../../loans/loan-caches";
import { LoanPaymentPicker } from "../../loans/loan-payment-picker";
import { QuickCategorizePopover } from "@/features/spending/components/quick-categorize-popover";
import {
  LOAN_KINDS,
  LoanValidationCard,
  blockingGaps,
  loanOfDocument,
} from "./loan-validation-card";
import { invalidateSpendingCaches } from "@/features/spending/lib/invalidation";
import { useTaxonomy } from "@/hooks/use-taxonomies";
import { QueryKeys } from "@/lib/query-keys";
import { getCategoryLabel } from "@/lib/taxonomy-labels";
import type {
  ReviewDecision,
  ReviewedOperation,
  StatementContinuity,
  StatementDocument,
  StatementReview,
} from "@/lib/types/statements";
import { cn } from "@/lib/utils";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  Icons,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  confirmDoubts,
  ownTransferChoice,
  suggestedChoice,
  type ReviewChoice,
} from "../statement-review";
import { useDownloadDocument } from "../use-download-document";

const SPENDING = "spending_categories";
const INCOME = "income_sources";
const SAVINGS = "savings_categories";

export type Choice = ReviewChoice;

export function money(value: string | null | undefined, currency: string): string {
  if (value == null) return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return value;
  try {
    return new Intl.NumberFormat(undefined, {
      style: "currency",
      currency,
    }).format(n);
  } catch {
    return n.toFixed(2);
  }
}

/** Names of the seeded and custom categories, to label a category id. */
export function useCategoryNames() {
  const spending = useTaxonomy(SPENDING);
  const income = useTaxonomy(INCOME);
  const savings = useTaxonomy(SAVINGS);
  return useMemo(() => {
    const names = new Map<string, string>();
    for (const [taxonomyId, query] of [
      [SPENDING, spending],
      [INCOME, income],
      [SAVINGS, savings],
    ] as const) {
      for (const c of query.data?.categories ?? []) names.set(`${taxonomyId}:${c.id}`, c.name);
    }
    return names;
  }, [spending, income, savings]);
}

interface ReviewDialogProps {
  /** The document to open first; the review then goes on with the rest of the queue. */
  startId: string | null;
  queue: StatementDocument[];
  onClose: () => void;
}

const frDay = (value?: string | null) => (value ? value.slice(0, 10) : "?");

/**
 * How the statement sits among the others of its account: the first one, or how it follows the previous one (its
 * days, and its opening balance that must take up the previous closing balance), and how the next one follows it.
 */
function ContinuityPanel({
  continuity: c,
  onGoTo,
}: {
  continuity: StatementContinuity;
  onGoTo: (id: string) => void;
}) {
  const { t } = useTranslation();
  const previousId = c.previous?.id;
  const lines: { ok: boolean | null; text: string }[] = [];
  if (c.state === "first") {
    lines.push({ ok: null, text: t("settings:documents.chain_first") });
  } else if (c.previous) {
    const previous = {
      name: c.previous.fileName,
      end: frDay(c.previous.periodEnd),
    };
    if (c.state === "follows")
      lines.push({ ok: true, text: t("settings:documents.chain_follows", previous) });
    if (c.state === "gap")
      lines.push({
        ok: false,
        text: t("settings:documents.chain_gap", {
          ...previous,
          from: frDay(c.missingFrom),
          to: frDay(c.missingTo),
        }),
      });
    if (c.state === "overlap")
      lines.push({ ok: false, text: t("settings:documents.chain_overlap", previous) });
    if (c.balanceFollows === true)
      lines.push({
        ok: true,
        text: t("settings:documents.chain_balance_ok", { balance: c.previous.closingBalance }),
      });
    if (c.balanceFollows === false)
      lines.push({
        ok: false,
        text: t("settings:documents.chain_balance_off", {
          balance: c.previous.closingBalance,
          difference: c.balanceDifference,
        }),
      });
    if (c.previous.status !== "VALIDATED")
      lines.push({ ok: false, text: t("settings:documents.chain_previous_pending") });
  }
  if (c.earlierPending > 0)
    lines.push({
      ok: false,
      text: t("settings:documents.chain_earlier_pending", { count: c.earlierPending }),
    });
  if (c.next && c.nextState && c.nextState !== "follows")
    lines.push({
      ok: false,
      text: t(`settings:documents.chain_next_${c.nextState}`, { name: c.next.fileName }),
    });
  if (c.next && c.nextBalanceFollows === false)
    lines.push({
      ok: false,
      text: t("settings:documents.chain_next_balance_off", { name: c.next.fileName }),
    });
  return (
    <div className="space-y-1.5 rounded-md border p-3 text-sm" data-testid="statement-continuity">
      {lines.map((line) => (
        <div key={line.text} className="flex items-start gap-2">
          {line.ok === true ? (
            <Icons.CheckCircle className="mt-0.5 size-4 shrink-0 text-green-600" />
          ) : line.ok === false ? (
            <Icons.AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600" />
          ) : (
            <Icons.Info className="text-muted-foreground mt-0.5 size-4 shrink-0" />
          )}
          <span>{line.text}</span>
        </div>
      ))}
      {previousId && c.previous?.status !== "VALIDATED" && (
        <Button size="sm" variant="outline" onClick={() => onGoTo(previousId)}>
          {t("settings:documents.chain_go_previous")}
        </Button>
      )}
    </div>
  );
}

export function DocumentReviewDialog({ startId, queue, onClose }: ReviewDialogProps) {
  const [currentId, setCurrentId] = useState<string | null>(startId);
  const [skipped, setSkipped] = useState<string[]>([]);
  const open = currentId !== null;

  const [done, setDone] = useState<string[]>([]);
  const next = (after: string, validated: string[] = []) => {
    const gone = [...done, ...validated, after];
    setDone(gone);
    const remaining = queue.filter((d) => !gone.includes(d.id) && !skipped.includes(d.id));
    setCurrentId(remaining[0]?.id ?? null);
    if (remaining.length === 0) onClose();
  };

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="flex max-h-[90vh] max-w-4xl flex-col gap-3 overflow-y-auto md:overflow-hidden">
        {currentId && (
          <ReviewBody
            key={currentId}
            id={currentId}
            remaining={queue.filter((d) => !skipped.includes(d.id) && !done.includes(d.id)).length}
            onGoTo={setCurrentId}
            onSkip={() => {
              setSkipped((s) => [...s, currentId]);
              next(currentId);
            }}
            onValidated={(validated) => next(currentId, validated)}
            onClose={onClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function ReviewBody({
  id,
  remaining,
  onGoTo,
  onSkip,
  onValidated,
  onClose,
}: {
  id: string;
  remaining: number;
  onGoTo: (id: string) => void;
  onSkip: () => void;
  /** The papers validated with this one (all the papers of a loan account). */
  onValidated: (validated: string[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const downloadDocument = useDownloadDocument();
  const queryClient = useQueryClient();
  const names = useCategoryNames();
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [onlyDoubts, setOnlyDoubts] = useState<boolean | null>(null);
  const [remember, setRemember] = useState(true);

  const {
    data: review,
    isLoading,
    error,
  } = useQuery({
    queryKey: [QueryKeys.STATEMENT_REVIEW, id],
    queryFn: () => reviewStatementDocument(id),
    staleTime: 0,
    gcTime: 0,
  });

  const isLoan = LOAN_KINDS.includes(review?.document.documentKind ?? "");
  // the review carries the loan; the list of loans is only a fallback
  const { data: loans, isLoading: loansLoading } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
    enabled: isLoan,
  });
  // the loans are refreshed whenever the person answers; the review carries the loan until they arrive
  const loan = isLoan ? (loanOfDocument(loans, id) ?? review?.loan) : undefined;
  const blocked = blockingGaps(loan).length > 0;

  const validate = useMutation({
    mutationFn: async () => {
      const decisions: ReviewDecision[] = Object.entries(choices).map(([activityId, c]) => ({
        activityId,
        taxonomyId: c.taxonomyId,
        categoryId: c.categoryId,
        remember,
      }));
      // a loan is validated as a whole: its account with all its papers, in their order
      if (loan) {
        await validateStatementLoan(loan.key);
        return loan.documents.map((d) => d.documentId);
      }
      await validateStatementDocument(id, decisions);
      return [id];
    },
    onSuccess: async (validated) => {
      toast({ title: t("settings:documents.review_validated_toast") });
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
        }),
        queryClient.invalidateQueries({
          queryKey: [QueryKeys.STATEMENT_REVIEW_QUEUE],
        }),
        queryClient.invalidateQueries({
          queryKey: [QueryKeys.STATEMENT_LOANS],
        }),
        queryClient.invalidateQueries({
          queryKey: [QueryKeys.ALTERNATIVE_HOLDINGS],
        }),
        queryClient.invalidateQueries({ queryKey: [QueryKeys.NET_WORTH] }),
      ]);
      invalidateSpendingCaches(queryClient);
      invalidateLoanCaches(queryClient);
      onValidated(validated);
    },
    onError: (e) =>
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const doubtCount = useMemo(
    () =>
      (review?.statements ?? []).reduce(
        (sum, s) =>
          sum +
          s.operations.filter((o) => o.needsReview && !(o.activityId && choices[o.activityId]))
            .length,
        0,
      ),
    [review, choices],
  );
  const showOnlyDoubts = onlyDoubts ?? doubtCount > 0;

  if (isLoading) {
    return (
      <>
        <DialogTitle>{t("settings:documents.review_title")}</DialogTitle>
        <DialogDescription>{t("settings:documents.review_loading")}</DialogDescription>
        <Skeleton className="h-48" />
      </>
    );
  }
  if (error || !review) {
    return (
      <>
        <DialogTitle>{t("settings:documents.review_title")}</DialogTitle>
        <DialogDescription>{String(error ?? "")}</DialogDescription>
        <div className="flex justify-end">
          <Button variant="outline" onClick={onClose}>
            {t("settings:documents.review_close")}
          </Button>
        </div>
      </>
    );
  }

  const { document } = review;
  const label = (taxonomyId: string, categoryId: string) =>
    getCategoryLabel(
      t,
      taxonomyId,
      categoryId,
      names.get(`${taxonomyId}:${categoryId}`) ?? categoryId,
    );

  return (
    <>
      <div className="flex items-start justify-between gap-4 pr-6">
        <div className="min-w-0">
          <DialogTitle className="truncate">{document.storedName}</DialogTitle>
          <DialogDescription>
            {document.periodStart && document.periodEnd
              ? `${document.periodStart} → ${document.periodEnd} · `
              : ""}
            {t("settings:documents.review_remaining", { count: remaining })}
          </DialogDescription>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="shrink-0"
          onClick={() => void downloadDocument(document.id)}
          aria-label={t("settings:documents.download")}
          title={t("settings:documents.download")}
        >
          <Icons.Download className="size-4" />
        </Button>
        {!isLoan && (
          <Badge variant={doubtCount > 0 ? "outline" : "secondary"} className="shrink-0">
            {doubtCount > 0
              ? t("settings:documents.review_to_check", { count: doubtCount })
              : t("settings:documents.review_nothing_doubtful")}
          </Badge>
        )}
      </div>

      {review.continuity && <ContinuityPanel continuity={review.continuity} onGoTo={onGoTo} />}

      {review.earlierPending.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
          <Icons.AlertTriangle className="size-4 shrink-0 text-amber-600" />
          <span className="min-w-0 flex-1">{t("settings:documents.review_earlier_pending")}</span>
          <Button size="sm" variant="outline" onClick={() => onGoTo(review.earlierPending[0])}>
            {t("settings:documents.review_go_oldest")}
          </Button>
        </div>
      )}

      {!isLoan && doubtCount > 0 && (
        <p className="text-muted-foreground text-sm">{t("settings:documents.review_help")}</p>
      )}

      {!isLoan && (
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <Button
            size="sm"
            variant={showOnlyDoubts ? "default" : "outline"}
            onClick={() => setOnlyDoubts(true)}
          >
            {t("settings:documents.review_only_doubts")}
          </Button>
          <Button
            size="sm"
            variant={showOnlyDoubts ? "outline" : "default"}
            onClick={() => setOnlyDoubts(false)}
          >
            {t("settings:documents.review_all")}
          </Button>
          {doubtCount > 0 && (
            <Button
              size="sm"
              variant="outline"
              className="ms-auto"
              onClick={() => setChoices((c) => confirmDoubts(review.statements, c))}
            >
              <Icons.Check className="mr-1 size-4" />
              {t("settings:documents.review_confirm_all", {
                count: doubtCount,
              })}
            </Button>
          )}
        </div>
      )}

      <div className="shrink-0 space-y-4 pr-1 md:min-h-0 md:flex-1 md:shrink md:overflow-y-auto">
        {isLoan && <LoanValidationCard loan={loan} loading={loansLoading} />}
        {!isLoan && review.statements.length === 0 && (
          <p className="text-muted-foreground text-sm">
            {t("settings:documents.review_no_operations")}
          </p>
        )}
        {!isLoan &&
          review.statements.map((statement) => (
            <StatementSection
              key={statement.index}
              statement={statement}
              choices={choices}
              onlyDoubts={showOnlyDoubts}
              label={label}
              onChoose={(activityId, choice) => setChoices((c) => ({ ...c, [activityId]: choice }))}
            />
          ))}
      </div>

      {!isLoan && (
        <label className="text-muted-foreground flex items-center gap-2 border-t pt-3 text-sm">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          {t("settings:documents.review_remember")}
        </label>
      )}
      <div className="flex flex-wrap items-center justify-end gap-2 pt-3">
        <Button variant="ghost" onClick={onClose}>
          {t("settings:documents.review_close")}
        </Button>
        <Button variant="outline" onClick={onSkip} disabled={validate.isPending}>
          {t("settings:documents.review_skip")}
        </Button>
        <Button onClick={() => validate.mutate()} disabled={validate.isPending || blocked}>
          <Icons.Check className="mr-1 size-4" />
          {t(
            isLoan
              ? "settings:documents.review_validate_loan"
              : "settings:documents.review_validate",
          )}
        </Button>
      </div>
    </>
  );
}

function StatementSection({
  statement,
  choices,
  onlyDoubts,
  label,
  onChoose,
}: {
  statement: StatementReview;
  choices: Record<string, Choice>;
  onlyDoubts: boolean;
  label: (taxonomyId: string, categoryId: string) => string;
  onChoose: (activityId: string, choice: Choice) => void;
}) {
  const { t } = useTranslation();
  const hasGap = !!statement.balanceGap;
  const operations = statement.operations.filter(
    (o) => !onlyDoubts || (o.needsReview && !(o.activityId && choices[o.activityId])),
  );
  return (
    <section className="space-y-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
        {statement.accountName && <span className="font-medium">{statement.accountName}</span>}
        <span className="text-muted-foreground">
          {t("settings:documents.review_opening")}{" "}
          {money(statement.openingBalance, statement.currency)}
        </span>
        <span className="text-muted-foreground">
          {t("settings:documents.review_closing")}{" "}
          {money(statement.closingBalance, statement.currency)}
        </span>
        {statement.computedBalance != null && (
          <span className="text-muted-foreground">
            {t("settings:documents.review_computed")}{" "}
            {money(statement.computedBalance, statement.currency)}
          </span>
        )}
        {statement.computedBalance != null &&
          (hasGap ? (
            <Badge variant="outline" className="border-amber-500 text-amber-700">
              {t("settings:documents.review_balance_gap", {
                gap: money(statement.balanceGap, statement.currency),
              })}
            </Badge>
          ) : (
            <Badge variant="secondary">
              <Icons.Check className="mr-1 size-3" />
              {t("settings:documents.review_balance_ok")}
            </Badge>
          ))}
      </div>
      <ul className="divide-y rounded-md border">
        {operations.map((op) => (
          <OperationRow
            key={`${statement.index}-${op.row}`}
            op={op}
            currency={statement.currency}
            choice={op.activityId ? choices[op.activityId] : undefined}
            label={label}
            onChoose={onChoose}
          />
        ))}
        {operations.length === 0 && (
          <li className="text-muted-foreground p-3 text-sm">
            {t("settings:documents.review_nothing_doubtful")}
          </li>
        )}
      </ul>
    </section>
  );
}

export function OperationRow({
  op,
  currency,
  choice,
  label,
  onChoose,
}: {
  op: ReviewedOperation;
  currency: string;
  choice?: Choice;
  label: (taxonomyId: string, categoryId: string) => string;
  onChoose: (activityId: string, choice: Choice) => void;
}) {
  const { t } = useTranslation();
  const outgoing = Number(op.amount) < 0;
  const taxonomyId = choice?.taxonomyId ?? op.taxonomyId ?? op.suggestedTaxonomyId;
  const categoryId = choice?.categoryId ?? op.categoryId ?? op.suggestedCategoryId;
  const doubtful = op.needsReview && !choice;
  const suggestion = suggestedChoice(op);
  const showCounterparty =
    !!op.counterparty && !op.label.toLowerCase().includes(op.counterparty.toLowerCase());
  return (
    <li
      className={cn(
        "flex flex-col gap-2 p-3 md:flex-row md:items-center",
        doubtful && "bg-amber-500/10",
      )}
    >
      <div className="text-muted-foreground w-24 shrink-0 text-xs tabular-nums">{op.date}</div>
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium break-words">{op.label}</div>
        {showCounterparty && (
          <div className="text-muted-foreground text-xs break-words">{op.counterparty}</div>
        )}
        {op.reason && (doubtful || op.confidence !== "high") && (
          <div className={cn("text-xs", doubtful ? "text-amber-700" : "text-muted-foreground")}>
            {op.reason}
          </div>
        )}
        {op.outdated && !choice && suggestion && (
          <div className="text-xs text-amber-700">
            {t("settings:documents.review_outdated", {
              category: label(suggestion.taxonomyId, suggestion.categoryId),
            })}
          </div>
        )}
        {op.otherAccountUnknown && (
          <div className="text-xs text-amber-700">
            {t("settings:documents.review_unknown_account")}
          </div>
        )}
      </div>
      <div
        className={cn(
          "w-28 shrink-0 text-right text-sm font-medium tabular-nums",
          outgoing ? "text-foreground" : "text-green-600",
        )}
      >
        {money(op.amount, op.currency || currency)}
      </div>
      <div className="flex w-full shrink-0 flex-wrap items-center gap-2 md:w-80">
        {doubtful ? (
          <Badge variant="outline" className="border-amber-500 text-amber-700">
            {t("settings:documents.review_doubt")}
          </Badge>
        ) : choice || op.categorySource === "manual" ? (
          <Badge variant="secondary">{t("settings:documents.review_confirmed")}</Badge>
        ) : op.confidence === "medium" ? (
          <Badge variant="outline">{t("settings:documents.review_estimated")}</Badge>
        ) : null}
        {op.activityId ? (
          <QuickCategorizePopover
            scope={outgoing ? "expense" : "income"}
            selectedCategoryId={categoryId}
            onSelect={(tax, cat) => onChoose(op.activityId!, { taxonomyId: tax, categoryId: cat })}
            trigger={
              <Button variant="outline" size="sm" className="min-w-0 flex-1 justify-start">
                <span className="truncate">
                  {taxonomyId && categoryId
                    ? label(taxonomyId, categoryId)
                    : t("settings:documents.review_category")}
                </span>
                <Icons.ChevronDown className="ms-auto size-3.5 shrink-0 opacity-60" />
              </Button>
            }
          />
        ) : null}
        <LoanPaymentPicker op={op} />
        {doubtful && op.activityId && (
          <div className="flex w-full flex-wrap gap-2">
            {suggestion && (
              <Button
                size="sm"
                onClick={() => onChoose(op.activityId!, suggestion)}
                title={t("settings:documents.review_confirm_hint")}
              >
                <Icons.Check className="mr-1 size-4" />
                {t("settings:documents.review_confirm")}
              </Button>
            )}
            <Button
              size="sm"
              variant="outline"
              onClick={() => onChoose(op.activityId!, ownTransferChoice(op.amount))}
              title={t("settings:documents.review_own_transfer_hint")}
            >
              <Icons.ArrowRightLeft className="mr-1 size-4" />
              {t("settings:documents.review_own_transfer")}
            </Button>
          </div>
        )}
      </div>
    </li>
  );
}
