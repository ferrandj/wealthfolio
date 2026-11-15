import {
  checkRemindersNow,
  getPushStatus,
  getStatementDocumentSources,
  pushAvailable,
  sendTestPush,
  snoozeStatementDocumentSources,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { Reminder } from "@/lib/types/reminders";
import { DocumentSourceForm } from "@/pages/settings/documents/sources/document-source-form";
import { dayAfter } from "@/pages/settings/documents/sources/document-sources";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Card,
  CardContent,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Icons,
  Page,
  PageContent,
  PageHeader,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";

import { disablePush, enablePush, pushState, type PushState } from "./push";
import { reminderTitle } from "./reminder-watcher";
import {
  byDay,
  dayKey,
  monthWeeks,
  reminderDate,
  reminderDay,
  REMINDER_HOURS,
} from "./reminders-model";
import { useReminders } from "./use-reminders";

/** A week put off: the usual "not now". */
const LATER_DAYS = 7;

/** The month, with the reminders on their days. A day can be picked to see only its reminders. */
function ReminderCalendar({
  reminders,
  selected,
  onSelect,
}: {
  reminders: Reminder[];
  selected: string | null;
  onSelect: (day: string | null) => void;
}) {
  const { t, i18n } = useTranslation();
  const today = new Date();
  const [month, setMonth] = useState(() => new Date(today.getFullYear(), today.getMonth(), 1));
  const weeks = useMemo(() => monthWeeks(month.getFullYear(), month.getMonth()), [month]);
  const days = useMemo(() => byDay(reminders), [reminders]);
  const monthName = new Intl.DateTimeFormat(i18n.language, { month: "long", year: "numeric" });
  const weekday = new Intl.DateTimeFormat(i18n.language, { weekday: "short" });
  const fullDay = new Intl.DateTimeFormat(i18n.language, { dateStyle: "full" });
  const move = (by: number) => setMonth((m) => new Date(m.getFullYear(), m.getMonth() + by, 1));
  const todayKey = dayKey(today);

  return (
    <Card>
      <CardContent className="space-y-3 p-4">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-lg font-semibold capitalize" aria-live="polite">
            {monthName.format(month)}
          </h2>
          <div className="flex items-center gap-1">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                setMonth(new Date(today.getFullYear(), today.getMonth(), 1));
                onSelect(null);
              }}
            >
              {t("common:reminders.today")}
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label={t("common:reminders.previous_month")}
              onClick={() => move(-1)}
            >
              <Icons.ChevronLeft className="size-4 rtl:rotate-180" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label={t("common:reminders.next_month")}
              onClick={() => move(1)}
            >
              <Icons.ChevronRight className="size-4 rtl:rotate-180" />
            </Button>
          </div>
        </div>
        <div className="grid grid-cols-7 gap-1 text-center" data-testid="reminder-calendar">
          {weeks[0].map((day) => (
            <div key={day.getDay()} className="text-muted-foreground py-1 text-xs capitalize">
              {weekday.format(day)}
            </div>
          ))}
          {weeks.flat().map((day) => {
            const key = dayKey(day);
            const ofDay = days.get(key) ?? [];
            const outside = day.getMonth() !== month.getMonth();
            const isSelected = selected === key;
            const anyDue = ofDay.some((r) => r.due);
            return (
              <button
                key={key}
                type="button"
                data-day={key}
                aria-pressed={isSelected}
                aria-label={
                  ofDay.length > 0
                    ? `${fullDay.format(day)} — ${t("common:reminders.count", { count: ofDay.length })}`
                    : fullDay.format(day)
                }
                onClick={() => onSelect(isSelected ? null : key)}
                className={[
                  "flex min-h-14 flex-col items-center gap-1 rounded-md border p-1 text-sm transition-colors",
                  "focus-visible:ring-ring focus-visible:ring-2 focus-visible:outline-none",
                  outside ? "text-muted-foreground/50 border-transparent" : "border-border/50",
                  isSelected ? "bg-primary text-primary-foreground" : "hover:bg-muted",
                  key === todayKey && !isSelected ? "border-primary font-semibold" : "",
                ].join(" ")}
              >
                <span className="tabular-nums">{day.getDate()}</span>
                {ofDay.length > 0 && (
                  <span
                    className={[
                      "rounded-full px-1.5 text-[11px] leading-4 font-medium tabular-nums",
                      isSelected
                        ? "bg-primary-foreground text-primary"
                        : anyDue
                          ? "bg-destructive text-white"
                          : "bg-primary/15 text-foreground",
                    ].join(" ")}
                  >
                    {ofDay.length}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

/** Whether this device is told when the app is closed, and the buttons to change it. */
function NotificationsCard() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const status = useQuery({
    queryKey: [QueryKeys.PUSH_STATUS],
    queryFn: getPushStatus,
    enabled: pushAvailable,
  });
  const [state, setState] = useState<PushState | null>(pushAvailable ? null : "unavailable");
  useEffect(() => {
    if (!pushAvailable) return;
    let alive = true;
    void pushState(status.data)
      .then((s) => alive && setState(s))
      .catch(() => alive && setState("unavailable"));
    return () => {
      alive = false;
    };
  }, [status.data]);

  const failed = (e: unknown) =>
    toast({
      title: t("common:reminders.push.failed"),
      description: e instanceof Error ? e.message : String(e),
      variant: "destructive",
    });
  const change = useMutation({
    mutationFn: (on: boolean) => (on ? enablePush() : disablePush()),
    onSuccess: (next) => {
      setState(next);
      void queryClient.invalidateQueries({ queryKey: [QueryKeys.PUSH_STATUS] });
      if (next === "denied") {
        toast({ title: t("common:reminders.push.denied"), variant: "destructive" });
      }
    },
    onError: failed,
  });
  const test = useMutation({
    mutationFn: sendTestPush,
    onSuccess: (delivery) =>
      toast({
        title: t("common:reminders.push.test_sent", { count: delivery.sent }),
        description:
          delivery.failed > 0
            ? t("common:reminders.push.test_failed", { count: delivery.failed })
            : undefined,
      }),
    onError: failed,
  });
  const check = useMutation({
    mutationFn: checkRemindersNow,
    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: [QueryKeys.REMINDERS] });
      toast({ title: t("common:reminders.push.checked", { count: result.reminders.length }) });
    },
    onError: failed,
  });
  const devices = status.data?.endpoints.length ?? 0;

  return (
    <Card>
      <CardContent className="space-y-3 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0 space-y-1">
            <h2 className="flex items-center gap-2 text-lg font-semibold">
              <Icons.Smartphone className="size-5" />
              {t("common:reminders.push.title")}
            </h2>
            <p className="text-muted-foreground text-sm">
              {t("common:reminders.push.text", { hours: REMINDER_HOURS.join(" h, ") + " h" })}
            </p>
          </div>
          {state !== null && state !== "unavailable" && (
            <Badge variant={state === "on" ? "default" : "outline"} data-testid="push-state">
              {t(`common:reminders.push.state_${state}`)}
            </Badge>
          )}
        </div>
        {state === "unavailable" ? (
          <p className="text-sm" data-testid="push-unavailable">
            {pushAvailable
              ? t("common:reminders.push.unavailable_browser")
              : t("common:reminders.push.unavailable_desktop")}
          </p>
        ) : (
          <>
            {state === "denied" && (
              <p className="text-destructive text-sm">{t("common:reminders.push.denied")}</p>
            )}
            <div className="flex flex-wrap gap-2">
              {state === "on" ? (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={change.isPending}
                  onClick={() => change.mutate(false)}
                >
                  {t("common:reminders.push.disable")}
                </Button>
              ) : (
                <Button
                  size="sm"
                  disabled={change.isPending || state === null || state === "denied"}
                  onClick={() => change.mutate(true)}
                >
                  {t("common:reminders.push.enable")}
                </Button>
              )}
              <Button
                size="sm"
                variant="outline"
                disabled={test.isPending || devices === 0}
                onClick={() => test.mutate()}
              >
                {t("common:reminders.push.test")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={check.isPending}
                onClick={() => check.mutate()}
              >
                {t("common:reminders.push.check_now")}
              </Button>
            </div>
            <p className="text-muted-foreground text-xs">
              {t("common:reminders.push.devices", { count: devices })}
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}

export default function RemindersPage() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: reminders, isLoading } = useReminders();
  const { data: sources } = useQuery({
    queryKey: [QueryKeys.STATEMENT_DOCUMENT_SOURCES],
    queryFn: getStatementDocumentSources,
  });
  const { accounts } = useAccounts({ filterActive: false });
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);

  const all = useMemo(() => reminders ?? [], [reminders]);
  const shown = selected ? all.filter((r) => reminderDay(r) === selected) : all;
  const dueCount = all.filter((r) => r.due).length;
  const when = new Intl.DateTimeFormat(i18n.language, {
    weekday: "short",
    day: "numeric",
    month: "long",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  const fullDay = new Intl.DateTimeFormat(i18n.language, { dateStyle: "full" });
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: [QueryKeys.REMINDERS] });
    void queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENT_SOURCES] });
  };
  const later = useMutation({
    mutationFn: (key: string) => snoozeStatementDocumentSources([key], dayAfter(LATER_DAYS)),
    onSuccess: refresh,
  });
  const editingSource = sources?.find((s) => s.key === editing);
  const rhythm = (days: number | null) =>
    days === null
      ? null
      : [7, 14, 30, 90, 365, 0].includes(days)
        ? t(`settings:documents.sources.rhythm_${days}`)
        : t("common:reminders.every_days", { count: days });

  return (
    <Page>
      <PageHeader heading={t("common:reminders.heading")} text={t("common:reminders.text")} />
      <PageContent className="mt-6 space-y-4">
        {isLoading ? (
          <Skeleton className="h-72 rounded-lg" />
        ) : (
          <>
            <ReminderCalendar reminders={all} selected={selected} onSelect={setSelected} />
            <Card>
              <CardContent className="space-y-2 p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h2 className="text-lg font-semibold">
                    {selected
                      ? fullDay.format(new Date(`${selected}T12:00:00`))
                      : t("common:reminders.list_title")}
                  </h2>
                  <div className="flex items-center gap-2">
                    {dueCount > 0 && !selected && (
                      <Badge variant="destructive">
                        {t("common:reminders.due_count", { count: dueCount })}
                      </Badge>
                    )}
                    {selected && (
                      <Button size="sm" variant="ghost" onClick={() => setSelected(null)}>
                        {t("common:reminders.show_all")}
                      </Button>
                    )}
                  </div>
                </div>
                {shown.length === 0 ? (
                  <p className="text-muted-foreground py-4 text-sm">
                    {selected ? t("common:reminders.none_that_day") : t("common:reminders.none")}
                  </p>
                ) : (
                  <ul className="divide-y" data-testid="reminder-list">
                    {shown.map((r) => (
                      <li
                        key={r.id}
                        className="flex flex-wrap items-start justify-between gap-3 py-3"
                      >
                        <div className="min-w-0 flex-1 space-y-0.5">
                          <div className="flex flex-wrap items-center gap-2 text-sm">
                            <time
                              dateTime={r.dueAt}
                              className="font-medium tabular-nums first-letter:uppercase"
                            >
                              {when.format(reminderDate(r))}
                            </time>
                            {r.due && (
                              <Badge variant="destructive">{t("common:reminders.due_now")}</Badge>
                            )}
                          </div>
                          <div className="font-medium break-words">{reminderTitle(r, t)}</div>
                          <div className="text-muted-foreground text-xs break-words">
                            {r.kind === "rates"
                              ? t("common:reminders.rates_text")
                              : [r.accountName, rhythm(r.everyDays)].filter(Boolean).join(" · ")}
                          </div>
                          {r.howTo && (
                            <div className="text-xs break-words whitespace-pre-line">{r.howTo}</div>
                          )}
                        </div>
                        <div className="flex flex-wrap gap-1">
                          {r.kind === "rates" ? (
                            <Button
                              size="sm"
                              variant={r.due ? "default" : "outline"}
                              onClick={() => navigate("/settings/rates")}
                            >
                              {t("common:reminders.open_rates")}
                            </Button>
                          ) : (
                            <>
                              <Button
                                size="sm"
                                variant={r.due ? "default" : "outline"}
                                onClick={() =>
                                  navigate(
                                    r.accountId
                                      ? `/settings/documents?account=${encodeURIComponent(r.accountId)}`
                                      : "/settings/documents",
                                  )
                                }
                              >
                                <Icons.Upload className="me-1 size-4" />
                                {t("common:reminders.drop")}
                              </Button>
                              {r.sourceKey && (
                                <>
                                  <Button
                                    size="sm"
                                    variant="outline"
                                    onClick={() => setEditing(r.sourceKey)}
                                  >
                                    <Icons.Pencil className="me-1 size-4" />
                                    {t("common:reminders.edit")}
                                  </Button>
                                  {r.due && (
                                    <Button
                                      size="sm"
                                      variant="ghost"
                                      disabled={later.isPending}
                                      onClick={() => r.sourceKey && later.mutate(r.sourceKey)}
                                    >
                                      {t("common:reminders.later", { count: LATER_DAYS })}
                                    </Button>
                                  )}
                                </>
                              )}
                            </>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          </>
        )}
        <NotificationsCard />
      </PageContent>

      <Dialog open={!!editingSource} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t("common:reminders.edit_title")}</DialogTitle>
          </DialogHeader>
          {editingSource && (
            <DocumentSourceForm
              key={editingSource.key}
              source={editingSource}
              accountName={
                accounts?.find((a) => a.id === editingSource.accountId)?.name ??
                editingSource.accountId
              }
              onSaved={() => {
                setEditing(null);
                refresh();
              }}
            />
          )}
        </DialogContent>
      </Dialog>
    </Page>
  );
}
