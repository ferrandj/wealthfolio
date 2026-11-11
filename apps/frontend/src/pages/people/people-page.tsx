import { listContacts } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import { Button, Icons, Page, PageContent, PageHeader, Skeleton } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router-dom";
import { PersonOverviewSection } from "../settings/contacts/components/person-overview";

/** The contacts that hold at least one account: the people whose money can be looked at. */
export function usePeopleWithAccounts() {
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  return {
    isLoading: contacts.isLoading,
    people: (contacts.data ?? []).filter(
      (c) => c.contact.kind !== "other" && c.linkedAccountIds.length > 0,
    ),
  };
}

/**
 * The holdings of each contact that has accounts (imported from documents or attached to them): their part of every
 * account, at the latest statements. `/people` shows everybody, `/people/:contactId` one person.
 */
export default function PeoplePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { contactId } = useParams();
  const { isBalanceHidden: hidden } = useBalancePrivacy();
  const { people, isLoading } = usePeopleWithAccounts();
  const shown = contactId ? people.filter((p) => p.contact.id === contactId) : people;
  const name = (n: string) => (hidden ? maskText(n) : n);

  return (
    <Page>
      <PageHeader
        heading={
          contactId && shown[0]
            ? t("settings:contacts.people_of", { name: name(shown[0].contact.displayName) })
            : t("settings:contacts.people_title")
        }
        text={t("settings:contacts.people_hint")}
      />
      <PageContent className="mt-4 grid gap-6">
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant={contactId ? "outline" : "default"}
            onClick={() => navigate("/people")}
          >
            {t("settings:contacts.people_all")}
          </Button>
          {people.map((p) => (
            <Button
              key={p.contact.id}
              size="sm"
              variant={contactId === p.contact.id ? "default" : "outline"}
              onClick={() => navigate(`/people/${p.contact.id}`)}
            >
              <Icons.User className="mr-1 size-3.5" />
              {name(p.contact.displayName)}
            </Button>
          ))}
        </div>
        {isLoading ? (
          <Skeleton className="h-40 w-full" />
        ) : shown.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("settings:contacts.people_none")}</p>
        ) : (
          shown.map((p) => (
            <div key={p.contact.id} className="grid gap-2">
              {!contactId && (
                <h2 className="text-lg font-semibold">{name(p.contact.displayName)}</h2>
              )}
              <PersonOverviewSection contactId={p.contact.id} />
            </div>
          ))
        )}
      </PageContent>
    </Page>
  );
}
