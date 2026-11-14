import { updateQuoteMode } from "@/adapters";
import { useSyncMarketDataMutation } from "@/hooks/use-sync-market-data";
import { QueryKeys } from "@/lib/query-keys";
import type { Asset } from "@/lib/types";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { defaultExclusions, manualAssets } from "./bulk-quote-mode";

/**
 * Turn on automatic price updates for every security priced by hand at once. The ones unticked stay manual: that is
 * the exclusion list (a euro fund, which has no market price, is unticked from the start). The whole price history is
 * then fetched for the ones switched.
 */
export function BulkQuoteModeDialog({ assets, onClose }: { assets: Asset[]; onClose: () => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const candidates = manualAssets(assets);
  const [excluded, setExcluded] = useState<string[]>(() => defaultExclusions(candidates));
  const refetchHistory = useSyncMarketDataMutation(true);
  const chosen = candidates.filter((a) => !excluded.includes(a.id));
  const toggle = (id: string) =>
    setExcluded((list) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]));

  const apply = useMutation({
    mutationFn: async () => {
      const switched: string[] = [];
      const failed: string[] = [];
      for (const a of chosen) {
        try {
          await updateQuoteMode(a.id, "MARKET");
          switched.push(a.id);
        } catch {
          failed.push(a.name ?? a.displayCode ?? a.id);
        }
      }
      return { switched, failed };
    },
    onSuccess: ({ switched, failed }) => {
      queryClient.invalidateQueries({ queryKey: [QueryKeys.ASSETS] });
      if (switched.length > 0) refetchHistory.mutate(switched);
      toast({
        title: t("asset:bulk_quotes.done", { count: switched.length }),
        description: failed.length
          ? t("asset:bulk_quotes.failed", { names: failed.join(", ") })
          : undefined,
        variant: failed.length ? "destructive" : undefined,
      });
      onClose();
    },
  });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("asset:bulk_quotes.title")}</DialogTitle>
          <DialogDescription>{t("asset:bulk_quotes.description")}</DialogDescription>
        </DialogHeader>
        {candidates.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("asset:bulk_quotes.none")}</p>
        ) : (
          <>
            <div className="flex gap-2">
              <Button size="sm" variant="ghost" onClick={() => setExcluded([])}>
                {t("asset:bulk_quotes.all")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setExcluded(candidates.map((a) => a.id))}
              >
                {t("asset:bulk_quotes.nothing")}
              </Button>
            </div>
            <div className="divide-y rounded-md border" data-testid="bulk-quotes-list">
              {candidates.map((a) => (
                <label
                  key={a.id}
                  className="hover:bg-muted/40 flex cursor-pointer items-center gap-3 px-3 py-2"
                >
                  <Checkbox
                    checked={!excluded.includes(a.id)}
                    onCheckedChange={() => toggle(a.id)}
                  />
                  <span className="min-w-0 text-sm">
                    <span className="font-medium">{a.displayCode ?? a.instrumentSymbol}</span>{" "}
                    <span className="text-muted-foreground">{a.name}</span>
                  </span>
                </label>
              ))}
            </div>
            <p className="text-muted-foreground text-xs">
              {t("asset:bulk_quotes.excluded", { count: excluded.length })}
            </p>
          </>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t("asset:bulk_quotes.cancel")}
          </Button>
          <Button disabled={chosen.length === 0 || apply.isPending} onClick={() => apply.mutate()}>
            {t("asset:bulk_quotes.apply", { count: chosen.length })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
