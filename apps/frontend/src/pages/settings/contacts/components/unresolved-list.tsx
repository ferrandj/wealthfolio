import { addContactIdentifier, listContacts, listUnresolvedCounterparties } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { NewContactIdentifier, UnresolvedCounterparty } from "@/lib/types/contacts";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
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
  Switch,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formatIban, toNumber } from "../contacts-utils";

interface Props {
  /** Open the contact dialog to create a card for this counterparty. */
  onCreate: (initial: { name: string; identifiers: NewContactIdentifier[] }) => void;
}

const amount = (value: string | number) =>
  new Intl.NumberFormat(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(
    toNumber(value),
  );

/** Who the imported operations were with, that no contact recognises yet: create a card or attach to one. */
export function UnresolvedList({ onCreate }: Props) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const [shops, setShops] = useState(false);
  const [target, setTarget] = useState<Record<string, string>>({});

  const list = useQuery({
    queryKey: [QueryKeys.UNRESOLVED_COUNTERPARTIES, shops],
    queryFn: () => listUnresolvedCounterparties(shops),
  });
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });

  const attach = useMutation({
    mutationFn: ({ item, contactId }: { item: UnresolvedCounterparty; contactId: string }) => {
      const identifier: NewContactIdentifier =
        item.kind === "iban" && item.iban
          ? { kind: "iban", value: item.iban, label: item.name }
          : { kind: "name", value: item.name ?? item.key };
      return addContactIdentifier(contactId, identifier);
    },
    onSuccess: () => {
      for (const key of [
        QueryKeys.CONTACTS,
        QueryKeys.CONTACT,
        QueryKeys.UNRESOLVED_COUNTERPARTIES,
      ]) {
        queryClient.invalidateQueries({ queryKey: [key] });
      }
      toast({ title: t("settings:contacts.attached") });
    },
    onError: (e) =>
      toast({
        title: t("settings:contacts.error"),
        description: e instanceof Error ? e.message : String(e),
        variant: "destructive",
      }),
  });

  const accountName = (id: string) => accounts.find((a) => a.id === id)?.name ?? id;

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-muted-foreground max-w-2xl text-sm">
          {t("settings:contacts.unresolved_hint")}
        </p>
        <label className="flex items-center gap-2 text-sm">
          <Switch checked={shops} onCheckedChange={setShops} />
          {t("settings:contacts.unresolved_show_shops")}
        </label>
      </div>

      {list.isLoading ? (
        <Skeleton className="h-40 w-full" />
      ) : (list.data ?? []).length === 0 ? (
        <Card>
          <CardContent className="text-muted-foreground py-10 text-center text-sm">
            {t("settings:contacts.unresolved_empty")}
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3">
          {(list.data ?? []).map((item) => {
            const id = `${item.kind}:${item.key}`;
            const chosen = target[id] ?? "";
            return (
              <Card key={id}>
                <CardContent className="grid gap-3 py-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="grid min-w-0 gap-1">
                      <span className="font-medium">
                        {item.name ?? formatIban(item.iban ?? "")}
                      </span>
                      {item.iban ? (
                        <code className="text-muted-foreground text-xs">
                          {formatIban(item.iban)}
                        </code>
                      ) : null}
                      <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">
                          {t("settings:contacts.operations_label", { count: item.count })}
                        </Badge>
                        {toNumber(item.totalOut) > 0 ? (
                          <span className="text-muted-foreground text-xs">
                            {t("settings:contacts.total_out")} {amount(item.totalOut)}
                          </span>
                        ) : null}
                        {toNumber(item.totalIn) > 0 ? (
                          <span className="text-success text-xs">
                            {t("settings:contacts.total_in")} {amount(item.totalIn)}
                          </span>
                        ) : null}
                        <span className="text-muted-foreground text-xs">
                          {t("settings:contacts.last_operation", { date: item.lastDate })}
                        </span>
                      </div>
                      {item.ownAccountId ? (
                        <Badge variant="outline" className="w-fit">
                          <Icons.CreditCard className="mr-1 size-3" />
                          {t("settings:contacts.unresolved_own_account", {
                            name: accountName(item.ownAccountId),
                          })}
                        </Badge>
                      ) : null}
                    </div>
                    <Button
                      size="sm"
                      onClick={() =>
                        onCreate({
                          name: item.name ?? "",
                          identifiers:
                            item.kind === "iban" && item.iban
                              ? [
                                  { kind: "iban", value: item.iban, label: item.name },
                                  ...(item.name
                                    ? [{ kind: "name" as const, value: item.name }]
                                    : []),
                                ]
                              : [{ kind: "name", value: item.name ?? item.key }],
                        })
                      }
                    >
                      <Icons.Plus className="mr-1 size-4" />
                      {t("settings:contacts.unresolved_create")}
                    </Button>
                  </div>
                  {(contacts.data ?? []).length > 0 ? (
                    <div className="flex flex-wrap items-center gap-2">
                      <Select
                        items={(contacts.data ?? []).map((c) => ({
                          value: c.contact.id,
                          label: c.contact.displayName,
                        }))}
                        value={chosen}
                        onValueChange={(v) =>
                          setTarget((current) => ({ ...current, [id]: v ?? "" }))
                        }
                      >
                        <SelectTrigger
                          className="w-64"
                          aria-label={t("settings:contacts.unresolved_attach_placeholder")}
                        >
                          <SelectValue
                            placeholder={t("settings:contacts.unresolved_attach_placeholder")}
                          />
                        </SelectTrigger>
                        <SelectContent>
                          {(contacts.data ?? []).map((c) => (
                            <SelectItem key={c.contact.id} value={c.contact.id}>
                              {c.contact.displayName}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={!chosen || attach.isPending}
                        onClick={() => attach.mutate({ item, contactId: chosen })}
                      >
                        {t("settings:contacts.unresolved_attach")}
                      </Button>
                    </div>
                  ) : null}
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
