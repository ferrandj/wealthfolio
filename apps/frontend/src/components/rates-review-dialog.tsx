import { confirmRates, getRatesReview, listRates, saveRate } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { RateView } from "@/lib/types/rates";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
} from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";

const SNOOZE_KEY = "wf-rates-review-snoozed";

const snoozed = (): boolean => {
  try {
    return window.sessionStorage.getItem(SNOOZE_KEY) === "1";
  } catch {
    return false;
  }
};

const day = (iso: string) => {
  const [y, m, d] = iso.split("-");
  return `${d}/${m}/${y}`;
};

/**
 * Every six months, after the revision of the regulated rates (1 February, 1 August), the first thing the app asks on
 * connection is to check them: they are shown prefilled, the person says they are unchanged or types the new ones.
 */
export function RatesReviewDialog() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [later, setLater] = useState(snoozed);
  const [typed, setTyped] = useState<Record<string, string>>({});
  const review = useQuery({
    queryKey: [QueryKeys.RATES_REVIEW],
    queryFn: getRatesReview,
    retry: false,
  });
  const due = review.data?.due === true && !later;
  const rates = useQuery({
    queryKey: [QueryKeys.RATES],
    queryFn: listRates,
    enabled: due,
    retry: false,
  });
  const regulated = (rates.data ?? []).filter((r) => r.kind === "regulated");

  const done = useMutation({
    mutationFn: async (save: boolean) => {
      if (save) {
        for (const r of regulated) {
          const text = typed[r.id];
          if (text === undefined) continue;
          const value = Number(text.replace(",", "."));
          if (Number.isFinite(value) && value !== Number(r.rate)) {
            await saveRate(r.id, { rate: value, source: t("rates_review.typed_source") });
          }
        }
      }
      return confirmRates();
    },
    onSuccess: (updated) => {
      queryClient.setQueryData([QueryKeys.RATES_REVIEW], updated);
      queryClient.invalidateQueries({ queryKey: [QueryKeys.RATES] });
    },
  });

  const postpone = () => {
    try {
      window.sessionStorage.setItem(SNOOZE_KEY, "1");
    } catch {
      // the question simply comes back at the next connection
    }
    setLater(true);
  };

  if (!due || !review.data) return null;
  return (
    <Dialog open onOpenChange={(open) => !open && postpone()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("rates_review.title")}</DialogTitle>
          <DialogDescription>
            {t("rates_review.description", { date: day(review.data.lastRevision) })}
          </DialogDescription>
        </DialogHeader>
        <div className="divide-y">
          {regulated.map((r: RateView) => (
            <div key={r.id} className="flex items-center gap-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="font-medium">{r.label}</div>
                {r.source && <div className="text-muted-foreground text-xs">{r.source}</div>}
              </div>
              <Input
                className="w-24 text-right"
                inputMode="decimal"
                aria-label={r.label}
                value={typed[r.id] ?? String(Number(r.rate))}
                onChange={(e) => setTyped({ ...typed, [r.id]: e.target.value })}
              />
              <span className="text-muted-foreground text-sm">%</span>
            </div>
          ))}
        </div>
        <p className="text-muted-foreground text-xs">
          {t("rates_review.next", { date: day(review.data.nextRevision) })}
        </p>
        <DialogFooter className="gap-2">
          <Button variant="ghost" onClick={postpone}>
            {t("rates_review.later")}
          </Button>
          <Button variant="outline" disabled={done.isPending} onClick={() => done.mutate(false)}>
            {t("rates_review.unchanged")}
          </Button>
          <Button disabled={done.isPending} onClick={() => done.mutate(true)}>
            {t("rates_review.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
