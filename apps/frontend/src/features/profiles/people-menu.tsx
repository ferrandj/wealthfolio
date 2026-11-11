import { usePeopleWithAccounts } from "@/pages/people/people-page";
import type { ContactSummary } from "@/lib/types/contacts";
import { DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, Icons } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";

/**
 * The contacts that hold accounts, as extra "profiles" next to the real ones: choosing one opens that person's
 * holdings. Nothing is rendered (and no router is needed) until at least one contact has an account.
 */
function useHoldings() {
  return usePeopleWithAccounts().people;
}

function DesktopLinks({ people }: { people: ContactSummary[] }) {
  const { t } = useTranslation("common");
  const navigate = useNavigate();
  return (
    <>
      <DropdownMenuLabel className="text-muted-foreground px-3 pt-1 text-xs">
        {t("profiles.holdingsOf")}
      </DropdownMenuLabel>
      {people.map((p) => (
        <DropdownMenuItem
          key={p.contact.id}
          className="h-10 gap-3"
          onClick={() => navigate(`/people/${p.contact.id}`)}
        >
          <Icons.User className="size-4" />
          <span className="min-w-0 truncate">{p.contact.displayName}</span>
        </DropdownMenuItem>
      ))}
      {people.length > 1 && (
        <DropdownMenuItem className="h-10 gap-3" onClick={() => navigate("/people")}>
          <Icons.Users className="size-4" />
          {t("profiles.holdingsAll")}
        </DropdownMenuItem>
      )}
      <DropdownMenuSeparator />
    </>
  );
}

export function PeopleMenuItems() {
  const people = useHoldings();
  return people.length > 0 ? <DesktopLinks people={people} /> : null;
}

function MobileLinks({
  people,
  rowClassName,
  onAction,
}: {
  people: ContactSummary[];
  rowClassName: string;
  onAction: (action: () => void) => void;
}) {
  const { t } = useTranslation("common");
  const navigate = useNavigate();
  return (
    <div className="py-3">
      <p className="text-muted-foreground pb-1 text-sm">{t("profiles.holdingsOf")}</p>
      {people.map((p) => (
        <button
          key={p.contact.id}
          type="button"
          className={rowClassName}
          onClick={() => onAction(() => navigate(`/people/${p.contact.id}`))}
        >
          <Icons.User className="size-6 shrink-0" />
          <span className="min-w-0 flex-1 truncate">{p.contact.displayName}</span>
        </button>
      ))}
      {people.length > 1 && (
        <button
          type="button"
          className={rowClassName}
          onClick={() => onAction(() => navigate("/people"))}
        >
          <Icons.Users className="size-6 shrink-0" />
          {t("profiles.holdingsAll")}
        </button>
      )}
    </div>
  );
}

export function MobilePeopleMenu(props: {
  rowClassName: string;
  onAction: (action: () => void) => void;
}) {
  const people = useHoldings();
  return people.length > 0 ? <MobileLinks people={people} {...props} /> : null;
}
