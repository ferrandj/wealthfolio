import { getStatementDocumentStorage } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { formatBytes } from "../statement-files";

/** How much smaller the kept documents are once compressed, as a whole percent of their original size. */
export function savedPercent(originalBytes: number, storedBytes: number): number {
  if (originalBytes <= 0 || storedBytes >= originalBytes) return 0;
  return Math.round((1 - storedBytes / originalBytes) * 100);
}

/** How many documents are kept and the room they take: they are stored compressed, without loss. */
export function DocumentStorageLine() {
  const { t } = useTranslation();
  const { data } = useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENTS, "storage"],
    queryFn: getStatementDocumentStorage,
  });
  if (!data || data.documents === 0) return null;
  const saved = savedPercent(data.originalBytes, data.storedBytes);
  return (
    <p className="text-muted-foreground text-xs" data-testid="document-storage">
      {t("settings:documents.storage", {
        count: data.documents,
        original: formatBytes(data.originalBytes),
        stored: formatBytes(data.storedBytes),
      })}
      {saved > 0 && ` ${t("settings:documents.storage_saved", { percent: saved })}`}
      {data.toCompress > 0 &&
        ` ${t("settings:documents.storage_pending", { count: data.toCompress })}`}
    </p>
  );
}
