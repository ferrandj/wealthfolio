import { categorizeStatementOperations, getOperationsToReview } from "@/adapters";
import { getCategoryLabel } from "@/lib/taxonomy-labels";
import { QueryKeys } from "@/lib/query-keys";
import type { OperationToReview, ReviewDecision } from "@/lib/types/statements";
import {
  OperationRow,
  useCategoryNames,
  type Choice,
} from "@/pages/settings/documents/components/document-review-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Which kind of review each operation needs, to filter the list. */
export const REVIEW_KINDS = ["doubt", "to_categorize", "loan_to_attach", "loan_proposed"] as const;

export const countByKind = (items: OperationToReview[]) =>
  Object.fromEntries(
    REVIEW_KINDS.map((k) => [k, items.filter((i) => i.why.includes(k)).length]),
  ) as Record<(typeof REVIEW_KINDS)[number], number>;

/**
 * Every operation of the imported statements that needs a look: a doubtful category, one left to categorize later,
 * a loan instalment to attach to its loan or a payment of another amount proposed for an instalment.
 */
export function OperationsReviewDialog({
  open,
  onClose,
  initialKind = null,
}: {
  open: boolean;
  onClose: () => void;
  /** Open on one kind of review (the operations to categorize later…), the others a click away. */
  initialKind?: string | null;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const names = useCategoryNames();
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [kind, setKind] = useState<string | null>(initialKind);
  const { data = [], isLoading } = useQuery({
    queryKey: [QueryKeys.STATEMENT_OPERATIONS_REVIEW],
    queryFn: getOperationsToReview,
    enabled: open,
  });
  const save = useMutation({
    mutationFn: () => {
      const decisions: ReviewDecision[] = Object.entries(choices).map(([activityId, c]) => ({
        activityId,
        taxonomyId: c.taxonomyId,
        categoryId: c.categoryId,
        remember: true,
      }));
      return categorizeStatementOperations(decisions);
    },
    onSuccess: () => {
      setChoices({});
      toast({ title: t("activity:operations_review.saved") });
      for (const key of [
        QueryKeys.STATEMENT_OPERATIONS_REVIEW,
        QueryKeys.STATEMENT_REVIEW,
        QueryKeys.STATEMENT_DOCUMENTS,
        QueryKeys.ACTIVITIES,
      ]) {
        queryClient.invalidateQueries({ queryKey: [key] });
      }
    },
    onError: (e) =>
      toast({
        title: t("activity:operations_review.error"),
        description: String(e),
        variant: "destructive",
      }),
  });
  const label = (taxonomyId: string, categoryId: string) =>
    getCategoryLabel(
      t,
      taxonomyId,
      categoryId,
      names.get(`${taxonomyId}:${categoryId}`) ?? categoryId,
    );
  const counts = countByKind(data);
  const shown = kind ? data.filter((i) => i.why.includes(kind)) : data;
  const chosen = Object.keys(choices).length;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex max-h-[92vh] flex-col overflow-hidden sm:max-w-4xl">
        <DialogTitle>{t("activity:operations_review.title")}</DialogTitle>
        <DialogDescription>{t("activity:operations_review.description")}</DialogDescription>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant={kind === null ? "default" : "outline"}
            onClick={() => setKind(null)}
          >
            {t("activity:operations_review.all", { count: data.length })}
          </Button>
          {REVIEW_KINDS.filter((k) => counts[k] > 0).map((k) => (
            <Button
              key={k}
              size="sm"
              variant={kind === k ? "default" : "outline"}
              onClick={() => setKind(k)}
            >
              {t(`activity:operations_review.kind.${k}`, { count: counts[k] })}
            </Button>
          ))}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {isLoading ? (
            <Skeleton className="h-40" />
          ) : shown.length === 0 ? (
            <p className="text-muted-foreground p-3 text-sm">
              {t("activity:operations_review.empty")}
            </p>
          ) : (
            <div className="divide-y rounded-md border">
              {shown.map((item) => (
                <div key={`${item.documentId}-${item.operation.activityId}`}>
                  <div className="text-muted-foreground flex flex-wrap items-center gap-2 px-3 pt-2 text-xs">
                    <span className="truncate">{item.accountName ?? item.fileName}</span>
                    {item.why.map((w) => (
                      <Badge key={w} variant="outline" className="border-amber-500 text-amber-700">
                        {t(`activity:operations_review.why.${w}`)}
                      </Badge>
                    ))}
                  </div>
                  <ul>
                    <OperationRow
                      op={item.operation}
                      currency={item.currency}
                      choice={
                        item.operation.activityId ? choices[item.operation.activityId] : undefined
                      }
                      label={label}
                      onChoose={(activityId, choice) =>
                        setChoices((c) => ({ ...c, [activityId]: choice }))
                      }
                    />
                  </ul>
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 border-t pt-3">
          <Button variant="ghost" onClick={onClose}>
            {t("activity:operations_review.close")}
          </Button>
          <Button disabled={chosen === 0 || save.isPending} onClick={() => save.mutate()}>
            {t("activity:operations_review.save", { count: chosen })}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
