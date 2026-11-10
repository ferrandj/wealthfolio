import { listContacts, listUnresolvedCounterparties } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { ContactKind, ContactRole, NewContactIdentifier } from "@/lib/types/contacts";
import { useQuery } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Card,
  CardContent,
  Icons,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@wealthfolio/ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsHeader } from "../settings-header";
import { ContactDialog } from "./components/contact-dialog";
import { UnresolvedList } from "./components/unresolved-list";
import {
  CONTACT_KINDS,
  CONTACT_ROLES,
  filterContacts,
  toNumber,
  type ContactForm,
} from "./contacts-utils";

const amount = (value: string | number) =>
  new Intl.NumberFormat(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(
    toNumber(value),
  );

interface DialogState {
  contactId?: string;
  initial?: { form?: Partial<ContactForm>; identifiers?: NewContactIdentifier[] };
}

export default function ContactsPage() {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<ContactKind | "all">("all");
  const [role, setRole] = useState<ContactRole | "all">("all");
  const [dialog, setDialog] = useState<DialogState | null>(null);

  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const unresolved = useQuery({
    queryKey: [QueryKeys.UNRESOLVED_COUNTERPARTIES, false],
    queryFn: () => listUnresolvedCounterparties(false),
  });

  const kindItems = [
    { value: "all", label: t("settings:contacts.filter_all_kinds") },
    ...CONTACT_KINDS.map((k) => ({ value: k, label: t(`settings:contacts.kind_${k}`) })),
  ];
  const roleItems = [
    { value: "all", label: t("settings:contacts.filter_all_roles") },
    ...CONTACT_ROLES.filter((r) => r !== "none").map((r) => ({
      value: r,
      label: t(`settings:contacts.role_${r}`),
    })),
  ];

  const visible = useMemo(
    () => filterContacts(contacts.data ?? [], { query, kind, role }),
    [contacts.data, query, kind, role],
  );

  return (
    <div className="space-y-6">
      <SettingsHeader heading={t("settings:contacts.title")} text={t("settings:contacts.subtitle")}>
        <Button onClick={() => setDialog({})}>
          <Icons.Plus className="mr-2 size-4" />
          {t("settings:contacts.new")}
        </Button>
      </SettingsHeader>

      <Tabs defaultValue="contacts">
        <TabsList>
          <TabsTrigger value="contacts">{t("settings:contacts.tab_contacts")}</TabsTrigger>
          <TabsTrigger value="unidentified">
            {t("settings:contacts.tab_unidentified")}
            {(unresolved.data ?? []).length > 0 ? (
              <Badge variant="secondary" className="ml-2">
                {(unresolved.data ?? []).length}
              </Badge>
            ) : null}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="contacts" className="grid gap-4">
          <div className="flex flex-wrap gap-2">
            <Input
              className="w-full sm:w-72"
              value={query}
              placeholder={t("settings:contacts.search_placeholder")}
              aria-label={t("settings:contacts.search_placeholder")}
              onChange={(e) => setQuery(e.target.value)}
            />
            <Select
              items={kindItems}
              value={kind}
              onValueChange={(v) => v && setKind(v as ContactKind | "all")}
            >
              <SelectTrigger className="w-48" aria-label={t("settings:contacts.field_kind")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("settings:contacts.filter_all_kinds")}</SelectItem>
                {CONTACT_KINDS.map((k) => (
                  <SelectItem key={k} value={k}>
                    {t(`settings:contacts.kind_${k}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select
              items={roleItems}
              value={role}
              onValueChange={(v) => v && setRole(v as ContactRole | "all")}
            >
              <SelectTrigger className="w-48" aria-label={t("settings:contacts.field_role")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("settings:contacts.filter_all_roles")}</SelectItem>
                {CONTACT_ROLES.filter((r) => r !== "none").map((r) => (
                  <SelectItem key={r} value={r}>
                    {t(`settings:contacts.role_${r}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {contacts.isLoading ? (
            <Skeleton className="h-40 w-full" />
          ) : (contacts.data ?? []).length === 0 ? (
            <Card>
              <CardContent className="grid gap-2 py-10 text-center">
                <p className="font-medium">{t("settings:contacts.empty_title")}</p>
                <p className="text-muted-foreground text-sm">{t("settings:contacts.empty_hint")}</p>
              </CardContent>
            </Card>
          ) : visible.length === 0 ? (
            <p className="text-muted-foreground py-6 text-center text-sm">
              {t("settings:contacts.no_match")}
            </p>
          ) : (
            <div className="grid gap-2">
              {visible.map((s) => (
                <button
                  key={s.contact.id}
                  type="button"
                  className="hover:bg-muted/50 focus-visible:ring-ring flex w-full flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3 text-left focus-visible:ring-2 focus-visible:outline-none"
                  onClick={() => setDialog({ contactId: s.contact.id })}
                >
                  <div className="grid min-w-0 gap-1">
                    <span className="truncate font-medium">{s.contact.displayName}</span>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="outline">
                        {t(`settings:contacts.kind_${s.contact.kind}`)}
                      </Badge>
                      {s.contact.role !== "none" ? (
                        <Badge variant="secondary">
                          {t(`settings:contacts.role_${s.contact.role}`)}
                        </Badge>
                      ) : null}
                      <span className="text-muted-foreground text-xs">
                        {t("settings:contacts.accounts_label", {
                          count:
                            s.identifiers.filter((i) => i.kind === "iban").length +
                            s.linkedAccountIds.length,
                        })}
                      </span>
                    </div>
                  </div>
                  <div className="text-muted-foreground grid text-right text-xs">
                    <span>
                      {t("settings:contacts.operations_label", { count: s.operationCount })}
                    </span>
                    {toNumber(s.totalOut) > 0 ? (
                      <span>
                        {t("settings:contacts.total_out")} {amount(s.totalOut)}
                      </span>
                    ) : null}
                    {toNumber(s.totalIn) > 0 ? (
                      <span className="text-success">
                        {t("settings:contacts.total_in")} {amount(s.totalIn)}
                      </span>
                    ) : null}
                  </div>
                </button>
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="unidentified">
          <UnresolvedList
            onCreate={({ name, identifiers }) =>
              setDialog({
                initial: { form: { displayName: name }, identifiers },
              })
            }
          />
        </TabsContent>
      </Tabs>

      {dialog ? (
        <ContactDialog
          key={dialog.contactId ?? "new"}
          open
          contactId={dialog.contactId}
          initial={dialog.initial}
          onOpenChange={(open) => {
            if (!open) setDialog(null);
          }}
        />
      ) : null}
    </div>
  );
}
