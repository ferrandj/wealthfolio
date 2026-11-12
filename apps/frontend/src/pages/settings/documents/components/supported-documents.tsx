import { listSupportedStatements } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import { Badge } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";

export function SupportedDocuments() {
  const { t } = useTranslation();
  const { data } = useQuery({
    queryKey: [QueryKeys.SUPPORTED_STATEMENTS],
    queryFn: listSupportedStatements,
    staleTime: Infinity,
  });

  if (!data) return null;
  return (
    <details className="text-sm">
      <summary className="cursor-pointer font-medium">
        {t("settings:documents.supported_title")}
      </summary>
      <div className="mt-3 space-y-4">
        {data.map((institution) => (
          <div key={institution.institution}>
            <div className="mb-1 font-medium">{institution.institution}</div>
            <ul className="space-y-1">
              {institution.adapters.map((adapter) => (
                <li
                  key={adapter.id}
                  className="flex flex-wrap items-center gap-2"
                >
                  <span>{adapter.label}</span>
                  {adapter.formats.map((f) => (
                    <Badge key={f} variant="outline" className="uppercase">
                      {f}
                    </Badge>
                  ))}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </details>
  );
}
