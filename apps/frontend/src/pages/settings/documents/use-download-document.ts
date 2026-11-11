import { getStatementDocumentContent, openFileSaveDialog } from "@/adapters";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { base64ToBytes } from "./statement-files";

/** Saves the original file of an imported document under its standard name. */
export function useDownloadDocument() {
  const { t } = useTranslation();
  return useCallback(
    async (documentId: string) => {
      try {
        const { contentBase64, document } = await getStatementDocumentContent(documentId);
        await openFileSaveDialog(base64ToBytes(contentBase64), document.storedName);
      } catch (e) {
        toast({
          title: t("settings:documents.error"),
          description: String(e),
          variant: "destructive",
        });
      }
    },
    [t],
  );
}
