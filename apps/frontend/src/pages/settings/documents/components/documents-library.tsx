import {
  deleteStatementDocument,
  getStatementDocumentContent,
  linkStatementDocument,
  listStatementDocuments,
  openFileSaveDialog,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { StatementDocument } from "@/lib/types/statements";
import { invalidatePerformanceCaches } from "@/lib/performance-cache";
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
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { base64ToBytes, formatBytes } from "../statement-files";

const NO_ACCOUNT = "none";

export function DocumentsLibrary() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const [toDelete, setToDelete] = useState<StatementDocument | null>(null);

  const { data: documents, isLoading } = useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
    queryFn: listStatementDocuments,
  });

  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENTS] });

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
        await queryClient.invalidateQueries({ queryKey: [QueryKeys.ACTIVITY_DATA] });
        await queryClient.invalidateQueries({ queryKey: [QueryKeys.ACTIVITIES] });
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

  const download = async (doc: StatementDocument) => {
    try {
      const { contentBase64, document } = await getStatementDocumentContent(doc.id);
      await openFileSaveDialog(base64ToBytes(contentBase64), document.storedName);
    } catch (e) {
      toast({
        title: t("settings:documents.error"),
        description: String(e),
        variant: "destructive",
      });
    }
  };

  if (isLoading) return <Skeleton className="h-24" />;

  if (!documents || documents.length === 0) {
    return <p className="text-muted-foreground text-sm">{t("settings:documents.library_empty")}</p>;
  }

  const accountItems = [
    { value: NO_ACCOUNT, label: t("settings:documents.no_account") },
    ...accounts.map((a) => ({ value: a.id, label: a.name })),
  ];

  return (
    <>
      <div className="space-y-2">
        {documents.map((doc) => (
          <Card key={doc.id}>
            <CardContent className="flex flex-col gap-3 p-4 md:flex-row md:items-center">
              <Icons.FileText className="text-muted-foreground hidden size-5 shrink-0 md:block" />
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium" title={doc.storedName}>
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
                  {doc.status === "IMPORTED" ? (
                    <Badge variant="secondary">
                      {t("settings:documents.operations_added", { count: doc.importedCount })}
                    </Badge>
                  ) : (
                    <Badge variant="outline">{t("settings:documents.kept_only")}</Badge>
                  )}
                </div>
              </div>
              <div className="w-full md:w-56">
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
              </div>
              <div className="flex gap-1">
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
        ))}
      </div>

      <AlertDialog open={!!toDelete} onOpenChange={(open) => !open && setToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings:documents.delete_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:documents.delete_description", { name: toDelete?.storedName ?? "" })}
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
