import {
  convertToInternalTransfer,
  findTransferMatchCandidates,
  linkTransferActivities,
  listAccountIdentities,
  searchActivities,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { ActivityDetails, TransferMatchCandidate } from "@/lib/types";
import type { AccountIdentity, ReviewedOperation } from "@/lib/types/statements";
import { TransferMatchDialog } from "@/pages/activity/components/transfer-match-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Icons } from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** The candidate certain enough to propose in one click: the core's only "high" one. */
export function bestCandidate(candidates: TransferMatchCandidate[]): TransferMatchCandidate | null {
  const sure = candidates.filter((c) => c.confidence === "high");
  return sure.length === 1 ? sure[0] : null;
}

/**
 * The account a label names by its number ("VRST FORTUNEO VIE 734639427101" names the contract 734639427101): its
 * account number, or the last eleven digits of its IBAN, found in the label.
 */
export function accountNamedIn(
  label: string,
  identities: Pick<AccountIdentity, "accountId" | "accountNumber" | "iban">[],
  except?: string,
): string | null {
  const digits = label.replace(/\s+/g, "");
  const found = identities.filter((i) => {
    if (i.accountId === except) return false;
    const number = i.accountNumber?.replace(/\s+/g, "");
    const iban = i.iban?.replace(/\s+/g, "").slice(-11);
    return (
      (!!number && number.length >= 6 && digits.includes(number)) ||
      (!!iban && iban.length === 11 && digits.includes(iban))
    );
  });
  return found.length === 1 ? found[0].accountId : null;
}

/**
 * For an "account to account" operation: the own account on the other side. Linked by itself when the other leg is
 * already imported on the same day; otherwise the likely leg is proposed in one click, and any account can be chosen
 * (an account without statements gets the other leg created).
 */
export function OwnTransferAccount({ op }: { op: ReviewedOperation }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const [source, setSource] = useState<ActivityDetails | null>(null);
  const activityId = op.activityId ?? "";
  const linked = op.transferAccountId
    ? accounts.find((a) => a.id === op.transferAccountId)?.name
    : null;
  const identities = useQuery({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES],
    queryFn: listAccountIdentities,
  });
  const candidates = useQuery({
    queryKey: ["own-transfer-candidates", activityId],
    queryFn: () => findTransferMatchCandidates({ activityId, windowDays: 5, limit: 5 }),
    enabled: !!activityId && !op.transferAccountId,
  });
  const refresh = () => {
    for (const key of [
      QueryKeys.STATEMENT_REVIEW,
      QueryKeys.STATEMENT_DOCUMENTS,
      QueryKeys.STATEMENT_OPERATIONS_REVIEW,
      QueryKeys.ACTIVITIES,
    ]) {
      queryClient.invalidateQueries({ queryKey: [key] });
    }
    queryClient.invalidateQueries({ queryKey: ["own-transfer-candidates", activityId] });
  };
  const toAccount = useMutation({
    mutationFn: (accountId: string) =>
      convertToInternalTransfer({ activityId, counterpartAccountId: accountId }),
    onSuccess: refresh,
    onError: (e) =>
      toast({
        title: t("settings:documents.transfer_link_error"),
        description: String(e),
        variant: "destructive",
      }),
  });
  const link = useMutation({
    mutationFn: async (other: string) => {
      try {
        await convertToInternalTransfer({ activityId, counterpartActivityId: other });
      } catch {
        // both legs already are transfers: they only need to be paired
        await linkTransferActivities(activityId, other);
      }
    },
    onSuccess: refresh,
    onError: (e) =>
      toast({
        title: t("settings:documents.transfer_link_error"),
        description: String(e),
        variant: "destructive",
      }),
  });
  const open = useMutation({
    mutationFn: () => searchActivities(0, 1, { activityIds: [activityId] }, ""),
    onSuccess: (r) => setSource(r.data[0] ?? null),
  });
  if (!activityId) return null;
  if (linked) {
    return (
      <div className="text-muted-foreground flex w-full items-center gap-1 text-xs">
        <Icons.ArrowRightLeft className="size-3.5" />
        {t("settings:documents.transfer_linked", { account: linked })}
      </div>
    );
  }
  const best = bestCandidate(candidates.data ?? []);
  const bestAccount = best ? accounts.find((a) => a.id === best.activity.accountId)?.name : null;
  // no operation to pair with yet, but the label names one of the accounts (a contract number…)
  const named = best ? null : accountNamedIn(op.label, identities.data ?? []);
  const namedAccount = named ? accounts.find((a) => a.id === named)?.name : null;
  return (
    <div className="flex w-full flex-wrap items-center gap-2" data-testid="own-transfer-account">
      {best && bestAccount ? (
        <Button
          size="sm"
          variant="outline"
          disabled={link.isPending}
          onClick={() => link.mutate(best.activity.id)}
        >
          <Icons.ArrowRightLeft className="mr-1 size-4" />
          {t("settings:documents.transfer_with", { account: bestAccount })}
        </Button>
      ) : named && namedAccount ? (
        <Button
          size="sm"
          variant="outline"
          disabled={toAccount.isPending}
          onClick={() => toAccount.mutate(named)}
        >
          <Icons.ArrowRightLeft className="mr-1 size-4" />
          {t("settings:documents.transfer_with", { account: namedAccount })}
        </Button>
      ) : (
        <span className="text-xs text-amber-700">{t("settings:documents.transfer_which")}</span>
      )}
      <Button size="sm" variant="ghost" disabled={open.isPending} onClick={() => open.mutate()}>
        {best || named
          ? t("settings:documents.transfer_other")
          : t("settings:documents.transfer_choose")}
      </Button>
      {source ? (
        <TransferMatchDialog
          open
          mode="link"
          sourceActivity={source}
          accounts={accounts}
          onOpenChange={(o) => !o && setSource(null)}
          onComplete={refresh}
        />
      ) : null}
    </div>
  );
}
