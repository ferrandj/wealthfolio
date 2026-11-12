import {
  attachStatementLoanDocument,
  getAlternativeHoldings,
  getStatementLoans,
  setStatementLoanFirstDue,
} from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { AlternativeAssetHolding } from "@/lib/types";
import type { LoanAccountChoice, LoanView } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Separator,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invalidateLoanCaches } from "./loan-caches";
import { LoanFacts } from "./loan-details";
import { loanOfDocument } from "./loans-utils";

const NEW_ACCOUNT = "__new__";
const LOAN_KEY = "statement_loan_key";
const LOAN_STATUS = "statement_loan_status";

/** The liabilities a loan paper can be attached to. */
export const loanAccounts = (holdings: AlternativeAssetHolding[]) =>
  holdings.filter((h) => h.kind.toLowerCase() === "liability");

const isPending = (h: AlternativeAssetHolding) => h.metadata?.[LOAN_STATUS] === "pending";

/**
 * The loan account of an imported paper: proposed as soon as the paper is imported, so every paper of the same loan
 * goes to the same account. The account is made now, empty and not validated, prefilled with what the paper says;
 * the person can edit it, or attach the paper to a loan account they already have.
 */
export function LoanAccountDialog({
  documentId,
  onClose,
}: {
  documentId: string | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { data: loans, isLoading } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
    enabled: !!documentId,
  });
  const loan = documentId ? loanOfDocument(loans, documentId) : undefined;
  return (
    <Dialog open={!!documentId} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogTitle>{t("settings:loans.account_title")}</DialogTitle>
        <DialogDescription>{t("settings:loans.account_intro")}</DialogDescription>
        {isLoading || !documentId ? (
          <Skeleton className="h-40" />
        ) : (
          <AccountForm
            key={`${documentId}-${loan?.key ?? ""}`}
            documentId={documentId}
            loan={loan}
            onDone={onClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function AccountForm({
  documentId,
  loan,
  onDone,
}: {
  documentId: string;
  loan: LoanView | undefined;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: holdings = [] } = useQuery({
    queryKey: [QueryKeys.ALTERNATIVE_HOLDINGS],
    queryFn: getAlternativeHoldings,
  });
  const accounts = loanAccounts(holdings);
  const known = loan?.liabilityId ?? accounts.find((a) => a.metadata?.[LOAN_KEY] === loan?.key)?.id;
  const [choice, setChoice] = useState<string>(known ?? NEW_ACCOUNT);
  const [name, setName] = useState(loan?.liabilityName ?? loan?.label ?? "");
  const [reference, setReference] = useState(loan && loan.key !== "?" ? loan.key : "");
  const [firstDue, setFirstDue] = useState(loan?.firstDue ?? loan?.inferredFirstDue ?? "");
  const papersDate = loan?.inferredFirstDue ?? null;

  const save = useMutation({
    mutationFn: async () => {
      const request: LoanAccountChoice =
        choice === NEW_ACCOUNT
          ? { newAccount: { name: name.trim(), key: reference.trim() || null } }
          : { liabilityId: choice };
      let result = await attachStatementLoanDocument(documentId, request);
      // a date the papers do not give is what the person typed
      const attached = loanOfDocument(result, documentId);
      if (attached && firstDue && firstDue !== papersDate && firstDue !== attached.firstDue) {
        result = await setStatementLoanFirstDue(attached.key, firstDue);
      }
      return result;
    },
    onSuccess: (loans) => {
      queryClient.setQueryData([QueryKeys.STATEMENT_LOANS], loans);
      queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENTS] });
      queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_REVIEW] });
      invalidateLoanCaches(queryClient);
      onDone();
    },
    onError: (e) =>
      toast({ title: t("settings:loans.error"), description: String(e), variant: "destructive" }),
  });

  const items = [
    ...accounts.map((a) => ({
      value: a.id,
      label: isPending(a) ? `${a.name} (${t("settings:loans.account_pending")})` : a.name,
    })),
    { value: NEW_ACCOUNT, label: t("settings:loans.account_new") },
  ];
  const canSave = choice === NEW_ACCOUNT ? name.trim().length > 0 : choice !== "";

  return (
    <div className="space-y-4 text-sm">
      {loan && (
        <>
          <div className="font-medium">{loan.label}</div>
          <LoanFacts loan={loan} />
          <Separator />
        </>
      )}
      <div className="grid gap-1.5">
        <Label>{t("settings:loans.account_choice")}</Label>
        <Select items={items} value={choice} onValueChange={(v) => setChoice(v ?? NEW_ACCOUNT)}>
          <SelectTrigger aria-label={t("settings:loans.account_choice")}>
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
      </div>
      {choice === NEW_ACCOUNT && (
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="grid gap-1.5 sm:col-span-2">
            <Label>{t("settings:loans.account_name")}</Label>
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className="grid gap-1.5">
            <Label>{t("settings:loans.account_reference")}</Label>
            <Input
              value={reference}
              placeholder={t("settings:loans.account_reference_auto")}
              onChange={(e) => setReference(e.target.value)}
            />
          </div>
        </div>
      )}
      <div className="grid gap-1.5 sm:w-1/2">
        <Label>{t("settings:loans.first_due")}</Label>
        <Input type="date" value={firstDue} onChange={(e) => setFirstDue(e.target.value)} />
        <span className="text-muted-foreground text-xs">
          {papersDate
            ? t("settings:loans.account_first_due_read")
            : t("settings:loans.account_first_due_missing")}
        </span>
      </div>
      <p className="text-muted-foreground text-xs">{t("settings:loans.account_empty_hint")}</p>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone}>
          {t("settings:loans.account_later")}
        </Button>
        <Button disabled={!canSave || save.isPending} onClick={() => save.mutate()}>
          {choice === NEW_ACCOUNT
            ? t("settings:loans.account_create")
            : t("settings:loans.account_attach")}
        </Button>
      </div>
    </div>
  );
}
