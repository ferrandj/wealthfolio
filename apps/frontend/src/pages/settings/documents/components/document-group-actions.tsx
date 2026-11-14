import { invalidateStatementDocuments, purgeStatementDocuments } from "@/adapters";
import type { PurgeReport, StatementDocument } from "@/lib/types/statements";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  Icons,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/**
 * Actions on a group of documents of the tree (a bank, an account, a kind): put them back to review, or remove them
 * with everything they brought. Removing first shows what would go (operations, emptied accounts, unpaired
 * transfers), then asks.
 */
export function DocumentGroupActions({
  name,
  documents,
  onDone,
}: {
  name: string;
  documents: StatementDocument[];
  onDone: () => Promise<unknown> | void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const ids = documents.map((d) => d.id);
  const [mode, setMode] = useState<"invalidate" | "purge" | null>(null);
  const [preview, setPreview] = useState<PurgeReport | null>(null);
  const fail = (e: unknown) =>
    toast({ title: t("settings:documents.error"), description: String(e), variant: "destructive" });
  const look = useMutation({
    mutationFn: () => purgeStatementDocuments(ids, true),
    onSuccess: (r) => {
      setPreview(r);
      setMode("purge");
    },
    onError: fail,
  });
  const purge = useMutation({
    mutationFn: () => purgeStatementDocuments(ids, false),
    onSuccess: async (r) => {
      setMode(null);
      setPreview(null);
      await onDone();
      // accounts, holders, positions, spending…: everything may have changed
      await queryClient.invalidateQueries();
      toast({
        title: t("settings:documents.group.purged", { name }),
        description: t("settings:documents.group.purged_summary", {
          documents: r.documents,
          operations: r.operations,
          accounts: r.accounts.length,
        }),
      });
    },
    onError: fail,
  });
  const invalidate = useMutation({
    mutationFn: () => invalidateStatementDocuments(ids),
    onSuccess: async (count) => {
      setMode(null);
      await onDone();
      toast({ title: t("settings:documents.invalidate_all_summary", { count }) });
    },
    onError: fail,
  });
  const validated = documents.some((d) => d.status === "VALIDATED");

  return (
    <span className="ms-2 inline-flex gap-1" onClick={(e) => e.preventDefault()}>
      <Button
        size="sm"
        variant="ghost"
        className="h-6 px-2 text-xs"
        disabled={!validated}
        onClick={() => setMode("invalidate")}
        title={t("settings:documents.group.invalidate_hint", { name })}
      >
        <Icons.AlertTriangle className="mr-1 size-3" />
        {t("settings:documents.group.invalidate")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        className="text-destructive h-6 px-2 text-xs"
        disabled={look.isPending}
        onClick={() => look.mutate()}
        title={t("settings:documents.group.purge_hint", { name })}
      >
        <Icons.Trash className="mr-1 size-3" />
        {t("settings:documents.group.purge")}
      </Button>

      <AlertDialog open={mode === "invalidate"} onOpenChange={(o) => !o && setMode(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("settings:documents.group.invalidate_title", { name })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:documents.group.invalidate_description", { count: documents.length })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings:documents.cancel")}</AlertDialogCancel>
            <Button disabled={invalidate.isPending} onClick={() => invalidate.mutate()}>
              {t("settings:documents.group.invalidate")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={mode === "purge" && !!preview}
        onOpenChange={(o) => {
          if (!o) {
            setMode(null);
            setPreview(null);
          }
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("settings:documents.group.purge_title", { name })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:documents.group.purge_intro")}
            </AlertDialogDescription>
            <div className="space-y-2 text-sm" data-testid="purge-preview">
              <ul className="list-disc space-y-1 ps-5">
                <li>
                  {t("settings:documents.group.will_documents", { count: preview?.documents ?? 0 })}
                </li>
                <li>
                  {t("settings:documents.group.will_operations", {
                    count: preview?.operations ?? 0,
                  })}
                </li>
                {preview?.accounts.length ? (
                  <li>
                    {t("settings:documents.group.will_accounts", {
                      count: preview.accounts.length,
                      names: preview.accounts.join(", "),
                    })}
                  </li>
                ) : null}
                {preview?.transfersUnlinked ? (
                  <li>
                    {t("settings:documents.group.will_unpair", {
                      count: preview.transfersUnlinked,
                    })}
                  </li>
                ) : null}
                {preview?.accountsKept.length ? (
                  <li>
                    {t("settings:documents.group.kept_accounts", {
                      names: preview.accountsKept.join(", "),
                    })}
                  </li>
                ) : null}
              </ul>
              <p className="text-destructive">{t("settings:documents.group.cannot_undo")}</p>
            </div>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings:documents.cancel")}</AlertDialogCancel>
            <Button variant="destructive" disabled={purge.isPending} onClick={() => purge.mutate()}>
              {t("settings:documents.group.purge_confirm")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </span>
  );
}
