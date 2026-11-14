import {
  getOperationsToReview,
  getStatementCoverage,
  getStatementLoans,
  listContacts,
  listMissingOwnAccounts,
  listStatementDocuments,
  listUnresolvedCounterparties,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { StatementDocument } from "@/lib/types/statements";
import { OperationsReviewDialog } from "@/pages/activity/components/operations-review-dialog";
import { useActivitySearch } from "@/pages/activity/hooks/use-activity-search";
import { AccountOwnerPrompt } from "@/pages/settings/contacts/components/account-owners";
import { DocumentReviewDialog } from "@/pages/settings/documents/components/document-review-dialog";
import { useQuery } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Card,
  CardContent,
  Icons,
  Page,
  PageContent,
  PageHeader,
  Skeleton,
} from "@wealthfolio/ui";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { buildTodo, OPERATION_KINDS, type OperationKind } from "./todo-model";

/** The number of activities flagged for review (imports, broker sync, migration). */
export function usePendingActivityCount() {
  const { totalRowCount, isLoading } = useActivitySearch({
    mode: "paginated",
    filters: { activityTypes: [], status: "pending" },
    searchQuery: "",
    sorting: [],
    pageIndex: 0,
    pageSize: 1,
  });
  return { count: totalRowCount, isLoading };
}

/** Everything the to-do screen lists, read from the app's own sources. */
export function useTodo() {
  const documents = useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
    queryFn: listStatementDocuments,
  });
  const operations = useQuery({
    queryKey: [QueryKeys.STATEMENT_OPERATIONS_REVIEW],
    queryFn: getOperationsToReview,
  });
  const loans = useQuery({ queryKey: [QueryKeys.STATEMENT_LOANS], queryFn: getStatementLoans });
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const coverage = useQuery({
    queryKey: [QueryKeys.STATEMENT_COVERAGE],
    queryFn: getStatementCoverage,
  });
  const missingOwnAccounts = useQuery({
    queryKey: [QueryKeys.MISSING_OWN_ACCOUNTS],
    queryFn: listMissingOwnAccounts,
  });
  const counterparties = useQuery({
    queryKey: [QueryKeys.UNRESOLVED_COUNTERPARTIES, false],
    queryFn: () => listUnresolvedCounterparties(false),
  });
  const { accounts, isLoading: accountsLoading } = useAccounts({
    filterActive: false,
    includeArchived: true,
  });
  const pending = usePendingActivityCount();
  const isLoading =
    documents.isLoading ||
    operations.isLoading ||
    loans.isLoading ||
    contacts.isLoading ||
    accountsLoading ||
    pending.isLoading;
  const todo = buildTodo({
    documents: documents.data,
    operations: operations.data,
    loans: loans.data,
    accounts,
    contacts: contacts.data,
    coverage: coverage.data,
    missingOwnAccounts: missingOwnAccounts.data,
    counterparties: counterparties.data,
    pendingActivities: pending.count,
  });
  return { todo, isLoading, accounts };
}

function Section({
  icon,
  title,
  count,
  description,
  children,
}: {
  icon: ReactNode;
  title: string;
  count: number;
  description?: string;
  children: ReactNode;
}) {
  if (count === 0) return null;
  return (
    <Card>
      <CardContent className="space-y-3 p-4">
        <div className="flex items-center gap-2">
          {icon}
          <h2 className="font-semibold">{title}</h2>
          <Badge variant="secondary">{count}</Badge>
        </div>
        {description ? <p className="text-muted-foreground text-sm">{description}</p> : null}
        {children}
      </CardContent>
    </Card>
  );
}

function Row({ label, detail, action }: { label: string; detail?: string; action: ReactNode }) {
  return (
    <li className="flex flex-wrap items-center justify-between gap-2 py-2">
      <div className="min-w-0">
        <div className="truncate text-sm font-medium">{label}</div>
        {detail ? <div className="text-muted-foreground text-xs">{detail}</div> : null}
      </div>
      {action}
    </li>
  );
}

/**
 * One screen for everything that is missing or waits: documents to validate, operations to categorize (the ones left
 * for later included), loan instalments to attach or confirm, loans whose papers are incomplete, accounts nobody
 * holds, months without a statement, own accounts and counterparties not identified, activities to review. Each line
 * opens what fixes it.
 */
export default function TodoPage() {
  const { t } = useTranslation();
  const { todo, isLoading, accounts } = useTodo();
  const navigate = useNavigate();
  const [review, setReview] = useState<{ startId: string; queue: StatementDocument[] } | null>(
    null,
  );
  const [operationKind, setOperationKind] = useState<OperationKind | null | undefined>(undefined);
  const [holders, setHolders] = useState<string[] | null>(null);
  const accountName = (id: string | null | undefined) =>
    accounts.find((a) => a.id === id)?.name ?? t("activity:todo.no_account");
  const operationsTotal = OPERATION_KINDS.reduce((n, k) => n + todo.operations[k], 0);

  return (
    <Page>
      <PageHeader heading={t("activity:todo.heading")} text={t("activity:todo.text")} />
      <PageContent className="mt-6 space-y-4">
        {isLoading ? (
          <div className="space-y-3">
            <Skeleton className="h-24 rounded-lg" />
            <Skeleton className="h-24 rounded-lg" />
          </div>
        ) : todo.total === 0 ? (
          <Card>
            <CardContent className="flex items-center gap-2 p-6 text-sm">
              <Icons.CheckCircle className="text-success size-5" />
              {t("activity:todo.all_done")}
            </CardContent>
          </Card>
        ) : (
          <>
            <Section
              icon={<Icons.FileText className="size-5" />}
              title={t("activity:todo.documents.title")}
              description={t("activity:todo.documents.description")}
              count={todo.documents.length}
            >
              <div>
                <Button
                  size="sm"
                  onClick={() =>
                    setReview({ startId: todo.documents[0].id, queue: todo.documents })
                  }
                >
                  {t("activity:todo.documents.review_all")}
                </Button>
              </div>
              <ul className="divide-y">
                {todo.documents.map((d) => (
                  <Row
                    key={d.id}
                    label={d.fileName}
                    detail={[
                      accountName(d.accountId),
                      d.toReviewCount > 0
                        ? t("activity:todo.documents.to_review", { count: d.toReviewCount })
                        : null,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                    action={
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => setReview({ startId: d.id, queue: todo.documents })}
                      >
                        {t("activity:todo.open")}
                      </Button>
                    }
                  />
                ))}
              </ul>
            </Section>

            <Section
              icon={<Icons.ListChecks className="size-5" />}
              title={t("activity:todo.operations.title")}
              description={t("activity:todo.operations.description")}
              count={operationsTotal}
            >
              <ul className="divide-y">
                {OPERATION_KINDS.filter((k) => todo.operations[k] > 0).map((k) => (
                  <Row
                    key={k}
                    label={t(`activity:operations_review.kind.${k}`, {
                      count: todo.operations[k],
                    })}
                    action={
                      <Button size="sm" variant="outline" onClick={() => setOperationKind(k)}>
                        {t("activity:todo.fix")}
                      </Button>
                    }
                  />
                ))}
              </ul>
            </Section>

            <Section
              icon={<Icons.Banknote className="size-5" />}
              title={t("activity:todo.loans.title")}
              description={t("activity:todo.loans.description")}
              count={todo.loans.length}
            >
              <ul className="divide-y">
                {todo.loans.map(({ loan, gaps }) => (
                  <Row
                    key={loan.key}
                    label={loan.liabilityName ?? loan.label}
                    detail={
                      gaps.length > 0
                        ? gaps.map((g) => g.text).join(" · ")
                        : t("activity:todo.loans.to_validate")
                    }
                    action={
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => navigate("/settings/loans")}
                      >
                        {t("activity:todo.fix")}
                      </Button>
                    }
                  />
                ))}
              </ul>
            </Section>

            <Section
              icon={<Icons.Users className="size-5" />}
              title={t("activity:todo.holders.title")}
              description={t("activity:todo.holders.description")}
              count={todo.ownerlessAccounts.length}
            >
              <div>
                <Button
                  size="sm"
                  onClick={() => setHolders(todo.ownerlessAccounts.map((a) => a.id))}
                >
                  {t("activity:todo.holders.set_all")}
                </Button>
              </div>
              <ul className="divide-y">
                {todo.ownerlessAccounts.map((a) => (
                  <Row
                    key={a.id}
                    label={a.name}
                    action={
                      <Button size="sm" variant="outline" onClick={() => setHolders([a.id])}>
                        {t("activity:todo.fix")}
                      </Button>
                    }
                  />
                ))}
              </ul>
            </Section>

            <Section
              icon={<Icons.Files className="size-5" />}
              title={t("activity:todo.statements.title")}
              description={t("activity:todo.statements.description")}
              count={todo.statements.length}
            >
              <ul className="divide-y">
                {todo.statements.map((s) => (
                  <Row
                    key={s.accountId}
                    label={accountName(s.accountId)}
                    detail={[
                      s.missingMonths.length > 0
                        ? t("activity:todo.statements.months", {
                            count: s.missingMonths.length,
                            months: s.missingMonths.join(", "),
                          })
                        : null,
                      s.balanceBreaks > 0
                        ? t("activity:todo.statements.breaks", { count: s.balanceBreaks })
                        : null,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                    action={
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => navigate("/settings/documents")}
                      >
                        {t("activity:todo.fix")}
                      </Button>
                    }
                  />
                ))}
              </ul>
            </Section>

            <Section
              icon={<Icons.Link className="size-5" />}
              title={t("activity:todo.contacts.title")}
              description={t("activity:todo.contacts.description")}
              count={todo.missingOwnAccounts.length + todo.counterparties}
            >
              <ul className="divide-y">
                {todo.missingOwnAccounts.map((m) => (
                  <Row
                    key={`${m.contactId}-${m.iban ?? m.label ?? ""}`}
                    label={m.label ?? m.iban ?? m.contactName}
                    detail={t("activity:todo.contacts.missing_account", {
                      name: m.contactName,
                      count: m.count,
                    })}
                    action={
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => navigate("/settings/accounts")}
                      >
                        {t("activity:todo.contacts.create")}
                      </Button>
                    }
                  />
                ))}
                {todo.counterparties > 0 ? (
                  <Row
                    label={t("activity:todo.contacts.counterparties", {
                      count: todo.counterparties,
                    })}
                    action={
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => navigate("/settings/contacts")}
                      >
                        {t("activity:todo.fix")}
                      </Button>
                    }
                  />
                ) : null}
              </ul>
            </Section>

            <Section
              icon={<Icons.AlertTriangle className="size-5" />}
              title={t("activity:todo.activities.title")}
              description={t("activity:todo.activities.description")}
              count={todo.pendingActivities}
            >
              <div>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => navigate("/activities?needsReview=true")}
                >
                  {t("activity:todo.activities.open", { count: todo.pendingActivities })}
                </Button>
              </div>
            </Section>
          </>
        )}
      </PageContent>
      {review ? (
        <DocumentReviewDialog
          startId={review.startId}
          queue={review.queue}
          onClose={() => setReview(null)}
        />
      ) : null}
      {operationKind !== undefined ? (
        <OperationsReviewDialog
          open
          initialKind={operationKind}
          onClose={() => setOperationKind(undefined)}
        />
      ) : null}
      {holders ? (
        <AccountOwnerPrompt accountIds={holders} onClose={() => setHolders(null)} />
      ) : null}
    </Page>
  );
}
