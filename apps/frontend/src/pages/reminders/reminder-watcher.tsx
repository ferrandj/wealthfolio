import { QueryKeys } from "@/lib/query-keys";
import type { Reminder } from "@/lib/types/reminders";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import { newlyDue, nextCheck, toldKey } from "./reminders-model";
import { useReminders } from "./use-reminders";

const TOLD_KEY = "wealthfolio.reminders.told";

function readTold(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(TOLD_KEY) ?? "[]") as string[]);
  } catch {
    return new Set();
  }
}

function writeTold(told: Set<string>) {
  try {
    // only the last ones: what is no longer due is never asked about again
    localStorage.setItem(TOLD_KEY, JSON.stringify([...told].slice(-200)));
  } catch {
    // private window: told again at the next opening, nothing worse
  }
}

/** What a reminder says in a line. */
export function reminderTitle(reminder: Reminder, t: (key: string) => string): string {
  return reminder.kind === "rates" ? t("common:reminders.rates_title") : reminder.label;
}

/**
 * Tells the person, in the app, about the reminders that came due. It looks when the app opens and again at
 * each of the three hours reminders are checked (10:00, 15:00, 20:00): one timer, nothing in between.
 * Renders nothing.
 */
export function ReminderWatcher() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data } = useReminders();

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const arm = () => {
      // a few seconds past the hour, so that what is set at that hour is due
      const wait = nextCheck().getTime() - Date.now() + 10_000;
      timer = setTimeout(() => {
        void queryClient.invalidateQueries({ queryKey: [QueryKeys.REMINDERS] });
        void queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENT_SOURCES] });
        arm();
      }, wait);
    };
    arm();
    return () => clearTimeout(timer);
  }, [queryClient]);

  useEffect(() => {
    if (!data) return;
    const told = readTold();
    const fresh = newlyDue(data, told);
    if (fresh.length === 0) return;
    for (const reminder of fresh) told.add(toldKey(reminder));
    writeTold(told);
    if (fresh.length > 3) {
      toast({
        title: t("common:reminders.toast_many", { count: fresh.length }),
        description: t("common:reminders.toast_open"),
      });
      return;
    }
    for (const reminder of fresh) {
      toast({
        title:
          reminder.kind === "rates"
            ? t("common:reminders.rates_title")
            : t("common:reminders.document_title"),
        description:
          reminder.kind === "rates"
            ? t("common:reminders.rates_text")
            : [reminder.label, reminder.accountName].filter(Boolean).join(" — "),
      });
    }
  }, [data, t]);

  return null;
}
